//! The round trip against the stand-in for PureRAW (`tools/fake-pureraw`): send, PureRAW's run on the
//! batch the plug-in wrote, then `program.end` (AC-4 to AC-6, AC-8, AC-9). The host can't start the
//! stand-in the way it starts PureRAW (the manifest names PureRAW by bundle id only), so each test runs
//! the steps itself.

mod support;

use std::fs;

use ccb_dxo_pureraw::{end, send};
use fake_pureraw::{Mode, output_of};
use lcc_plugin_sdk::serde_json::Value;
use support::FakeHost;

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Sends `photos`, runs the stand-in in `mode` on what was sent, and ends the run.
fn round_trip(h: &mut FakeHost, photos: &[&str], mode: Mode) -> Value {
    send(h, &ids(photos)).unwrap();
    let run = h.last_run().clone();
    fake_pureraw::run(&run.args, &run.tmp, mode).unwrap();
    let req = h.end_request("exited");
    end(h, &req).unwrap()
}

fn summary(h: &FakeHost) -> &str {
    h.log.last().unwrap()
}

#[test]
fn each_dng_is_stacked_on_the_photo_its_index_points_at_not_on_the_batch_position() {
    let mut h = FakeHost::new("rt-sorted");
    // sent b, a: PureRAW lists them a, b
    let b = h.raw("b/IMG_2.CR3");
    let a = h.raw("a/IMG_1.CR3");
    let reply = round_trip(&mut h, &[&b, &a], Mode::Process);
    assert_eq!(reply["added"], 2, "{reply}");
    for id in [&a, &b] {
        let out = output_of(&h.path_of(id)).unwrap();
        let stacked: Vec<_> = h.stacked_on(id).iter().map(|p| p.path.clone().unwrap()).collect();
        assert_eq!(stacked, [out], "photo {id}");
    }
    assert!(h.adds.iter().all(|a| a["stack"] == true));
    assert!(summary(&h).starts_with("DxO PureRAW: added 2 DNGs, stacked on their originals."), "{}", summary(&h));
}

#[test]
fn an_output_listed_twice_is_added_once() {
    let mut h = FakeHost::new("rt-repeat");
    let a = h.raw("a.dng");
    let reply = round_trip(&mut h, &[&a], Mode::RepeatOutputs);
    let list = fs::read_to_string(h.last_run().tmp.join(fake_pureraw::RESULTS)).unwrap();
    assert_eq!(list.matches("_DxO_DeepPRIME.dng").count(), 2, "the stand-in lists it twice: {list}");
    assert_eq!(h.adds.len(), 1);
    assert_eq!(reply["added"], 1);
}

#[test]
fn a_photo_missing_from_the_results_gets_nothing_and_is_named_as_not_processed() {
    let mut h = FakeHost::new("rt-partial");
    let ok = h.raw("ok.dng");
    let failed = h.raw("will-fail.dng");
    let reply = round_trip(&mut h, &[&ok, &failed], Mode::Process);
    assert_eq!(reply["added"], 1);
    assert!(h.stacked_on(&failed).is_empty());
    assert_eq!(reply["notProcessed"], lcc_plugin_sdk::serde_json::json!([h.path_of(&failed)]));
    assert!(summary(&h).contains("not processed (failed or cancelled in DxO PureRAW): will-fail.dng"), "{}", summary(&h));
}

#[test]
fn a_missing_original_is_named_as_not_processed() {
    let mut h = FakeHost::new("rt-missing");
    let ok = h.raw("ok.dng");
    let missing = h.raw("missing.dng");
    fs::remove_file(h.path_of(&missing)).unwrap();
    round_trip(&mut h, &[&ok, &missing], Mode::Process);
    assert!(summary(&h).contains("not processed (failed or cancelled in DxO PureRAW): missing.dng"), "{}", summary(&h));
}

#[test]
fn a_dng_saved_outside_its_original_s_folder_is_named_with_what_to_change() {
    let mut h = FakeHost::new("rt-outside");
    let a = h.raw("photos/a.dng");
    send(&mut h, &ids(&[&a])).unwrap();
    let run = h.last_run().clone();
    // PureRAW set to save into a folder of its own
    let elsewhere = h.root.join("Pictures/DxO/a_DxO.dng");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, b"dng").unwrap();
    let list =
        format!("DxO PureRAW 3\nEND_OF_COLLECTION_SET_SECTION\nxEND_OF_COLLECTION_NAME_SECTION\n- {}\n0 {}\n", h.path_of(&a), elsewhere.display());
    fs::write(run.tmp.join(fake_pureraw::RESULTS), list).unwrap();
    let req = h.end_request("exited");
    let reply = end(&mut h, &req).unwrap();
    assert_eq!(reply["added"], 0);
    let s = summary(&h);
    assert!(s.contains("outside their originals' folders (see above): a_DxO.dng"), "{s}");
    assert!(h.logged("Set its output folder back to the default (a \"DxO\" folder next to the originals)"), "{:?}", h.log);
    assert!(!s.contains("not processed"), "it was processed: {s}");
}

#[test]
fn closing_pureraw_without_processing_adds_nothing_and_says_so() {
    let mut h = FakeHost::new("rt-close");
    let a = h.raw("a.dng");
    let reply = round_trip(&mut h, &[&a], Mode::Close);
    assert_eq!(reply["added"], 0);
    assert!(h.adds.is_empty());
    assert!(summary(&h).contains("processed nothing"), "{}", summary(&h));
}

#[test]
fn a_batch_pureraw_never_read_says_it_may_already_have_been_open() {
    let mut h = FakeHost::new("rt-busy");
    let a = h.raw("a.dng");
    round_trip(&mut h, &[&a], Mode::Busy);
    assert!(h.adds.is_empty());
    let s = summary(&h);
    assert!(s.contains("already open from another app or an earlier send"), "{s}");
    assert!(s.contains("Lightroom Classic's DxO plug-in"), "{s}");
}

#[test]
fn stopping_the_wait_adds_nothing_even_when_results_are_there() {
    let mut h = FakeHost::new("rt-cancel");
    let a = h.raw("a.dng");
    send(&mut h, &ids(&[&a])).unwrap();
    let run = h.last_run().clone();
    fake_pureraw::run(&run.args, &run.tmp, Mode::Process).unwrap();
    let req = h.end_request("cancelled");
    let reply = end(&mut h, &req).unwrap();
    assert_eq!(reply["added"], 0);
    assert!(h.adds.is_empty());
    let s = summary(&h);
    assert!(s.contains("It keeps running") && s.contains("won't be added"), "{s}");
}

#[test]
fn sending_a_photo_again_keeps_one_stacked_version_read_again() {
    let mut h = FakeHost::new("rt-again");
    let a = h.raw("a.CR3");
    round_trip(&mut h, &[&a], Mode::Process);
    // the original changed, so PureRAW's new DNG differs from the first one
    fs::write(h.path_of(&a), b"raw data, edited").unwrap();
    let reply = round_trip(&mut h, &[&a], Mode::Process);
    assert_eq!(h.stacked_on(&a).len(), 1, "one stacked version, not two");
    assert_eq!((reply["added"].as_u64(), reply["updated"].as_u64()), (Some(0), Some(1)), "{reply}");
    assert!(summary(&h).contains("1 DNG was made again and read again"), "{}", summary(&h));
}
