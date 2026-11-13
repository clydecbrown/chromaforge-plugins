//! The round trip against the stand-in for PureRAW (`tools/fake-pureraw`): send, PureRAW's run on the
//! batch the plug-in wrote, then `program.end`, for PureRAW 3, 2 and an experimental version. The host can't start the
//! stand-in the way it starts PureRAW (the manifest names PureRAW by bundle id only), so each test runs
//! the steps itself.

mod support;

use std::fs;

use ccb_dxo_pureraw::versions::{VERSION_FILE, label, results_file};
use ccb_dxo_pureraw::{end, send};
use fake_pureraw::{Mode, output_of};
use lcc_plugin_sdk::serde_json::{Value, json};
use support::FakeHost;

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Sends `photos`, runs the stand-in in `mode` on what was sent, and ends the run.
fn round_trip(h: &mut FakeHost, photos: &[&str], mode: Mode) -> Value {
    round_trip_as(h, photos, mode, 3)
}

/// The same with PureRAW `version`, chosen in the dialog.
fn round_trip_as(h: &mut FakeHost, photos: &[&str], mode: Mode, version: u32) -> Value {
    if !h.installed.contains(&version) {
        h.installed.push(version);
    }
    send(h, &ids(photos), &json!(label(version))).unwrap();
    let run = h.last_run().clone();
    assert_eq!(run.version, version);
    fake_pureraw::run_as(&run.args, &run.tmp, mode, version).unwrap();
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
    assert!(summary(&h).starts_with("DxO PureRAW 3: added 2 DNGs, stacked on their originals."), "{}", summary(&h));
}

#[test]
fn an_output_listed_twice_is_added_once() {
    let mut h = FakeHost::new("rt-repeat");
    let a = h.raw("a.dng");
    let reply = round_trip(&mut h, &[&a], Mode::RepeatOutputs);
    let list = fs::read_to_string(h.last_run().tmp.join(fake_pureraw::results_file(3))).unwrap();
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
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    let run = h.last_run().clone();
    // PureRAW set to save into a folder of its own
    let elsewhere = h.root.join("Pictures/DxO/a_DxO.dng");
    fs::create_dir_all(elsewhere.parent().unwrap()).unwrap();
    fs::write(&elsewhere, b"dng").unwrap();
    let list =
        format!("DxO PureRAW 3\nEND_OF_COLLECTION_SET_SECTION\nxEND_OF_COLLECTION_NAME_SECTION\n- {}\n0 {}\n", h.path_of(&a), elsewhere.display());
    fs::write(run.tmp.join(fake_pureraw::results_file(3)), list).unwrap();
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
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
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
    assert_eq!(summary(&h), "DxO PureRAW 3: added no new DNGs; 1 DNG was made again and read again.");
}

#[test]
fn pureraw_2_s_pairs_and_final_block_stack_each_dng_once_on_its_original() {
    let mut h = FakeHost::new("rt-v2");
    h.installed = vec![2];
    let b = h.raw("b/IMG_1.CR3");
    let a = h.raw("a/IMG_2.CR3");
    let reply = round_trip_as(&mut h, &[&b, &a], Mode::Process, 2);
    let list = fs::read_to_string(h.last_run().tmp.join(results_file(2))).unwrap();
    assert_eq!(list.matches("_DxO_DeepPRIME.dng").count(), 4, "a pair per photo, then the final block: {list}");
    assert_eq!(h.adds.len(), 2, "each output once");
    assert_eq!(reply["added"], 2, "{reply}");
    for id in [&a, &b] {
        let stacked: Vec<_> = h.stacked_on(id).iter().map(|p| p.path.clone().unwrap()).collect();
        assert_eq!(stacked, [output_of(&h.path_of(id)).unwrap()], "photo {id}");
    }
    assert_eq!(summary(&h), "DxO PureRAW 2: added 2 DNGs, stacked on their originals.");
}

#[test]
fn a_cancelled_pureraw_2_run_adds_the_finished_photo_and_names_the_rest() {
    let mut h = FakeHost::new("rt-v2-cancel");
    let first = h.raw("a/1.CR3");
    let second = h.raw("b/2.CR3");
    let reply = round_trip_as(&mut h, &[&second, &first], Mode::Cancel, 2);
    assert_eq!(reply["added"], 1, "{reply}");
    assert_eq!(h.stacked_on(&first).len(), 1);
    assert!(h.stacked_on(&second).is_empty());
    assert!(summary(&h).contains("not processed (failed or cancelled in DxO PureRAW): 2.CR3"), "{}", summary(&h));
}

#[test]
fn a_pureraw_2_run_closed_without_processing_adds_nothing_and_says_so() {
    let mut h = FakeHost::new("rt-v2-close");
    let a = h.raw("a.CR3");
    let reply = round_trip_as(&mut h, &[&a], Mode::Close, 2);
    assert_eq!(reply["added"], 0);
    assert!(h.adds.is_empty());
    assert!(summary(&h).starts_with("DxO PureRAW 2 processed nothing"), "{}", summary(&h));
}

#[test]
fn an_experimental_version_s_run_reads_its_own_files_and_says_it_s_experimental() {
    let mut h = FakeHost::new("rt-exp");
    let a = h.raw("a/1.CR3");
    let b = h.raw("b/2.CR3");
    let reply = round_trip_as(&mut h, &[&a, &b], Mode::Process, 6);
    assert!(h.last_run().tmp.join(results_file(6)).is_file());
    assert_eq!(reply["added"], 2, "{reply}");
    assert_eq!(summary(&h), "DxO PureRAW 6 (experimental): added 2 DNGs, stacked on their originals.", "nothing to report");
}

#[test]
fn an_experimental_version_that_leaves_no_results_says_how_to_report_it() {
    let mut h = FakeHost::new("rt-exp-none");
    let a = h.raw("a.CR3");
    round_trip_as(&mut h, &[&a], Mode::Close, 6);
    let s = summary(&h);
    assert!(s.starts_with("DxO PureRAW 6 (experimental) processed nothing"), "{s}");
    assert!(
        s.contains("If it did process them")
            && s.contains("Please report what happened at https://github.com/clydecbrown/chromaforge-plugins/issues"),
        "{s}"
    );
}

#[test]
fn an_experimental_version_s_unexpected_lines_say_how_to_report_them() {
    for (name, extra) in [("junk", "nonsense\n"), ("index", "7 /p/DxO/x.dng\n"), ("unsent", "- /elsewhere/x.dng\n0 /elsewhere/DxO/x.dng\n")] {
        let mut h = FakeHost::new(&format!("rt-exp-{name}"));
        h.installed = vec![5];
        let a = h.raw("a.CR3");
        send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
        let run = h.last_run().clone();
        fake_pureraw::run_as(&run.args, &run.tmp, Mode::Process, 5).unwrap();
        let path = run.tmp.join(results_file(5));
        let list = fs::read_to_string(&path).unwrap() + extra;
        fs::write(&path, list).unwrap();
        let req = h.end_request("exited");
        let reply = end(&mut h, &req).unwrap();
        assert_eq!(reply["added"], 1, "{name}: the rest still comes in");
        let s = summary(&h);
        assert!(s.contains("Some results weren't as expected") && s.contains("tested with DxO PureRAW 5"), "{name}: {s}");
    }
}

#[test]
fn a_tested_version_s_unexpected_lines_are_logged_without_asking_for_a_report() {
    let mut h = FakeHost::new("rt-v3-junk");
    let a = h.raw("a.CR3");
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    let run = h.last_run().clone();
    fake_pureraw::run(&run.args, &run.tmp, Mode::Process).unwrap();
    let path = run.tmp.join(results_file(3));
    fs::write(&path, fs::read_to_string(&path).unwrap() + "nonsense\n").unwrap();
    let req = h.end_request("exited");
    end(&mut h, &req).unwrap();
    assert!(h.logged("isn't a result line"), "{:?}", h.log);
    assert!(!summary(&h).contains("report"), "{}", summary(&h));
}

#[test]
fn a_run_sent_before_there_were_versions_reads_pureraw_3_s_results() {
    let mut h = FakeHost::new("rt-no-version");
    let a = h.raw("a.CR3");
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    let run = h.last_run().clone();
    fs::remove_file(run.tmp.join(VERSION_FILE)).unwrap();
    fake_pureraw::run(&run.args, &run.tmp, Mode::Process).unwrap();
    let req = h.end_request("exited");
    let reply = end(&mut h, &req).unwrap();
    assert_eq!(reply["added"], 1, "{reply}");
}

#[test]
fn an_experimental_version_s_results_in_a_format_the_reader_doesn_t_know_say_how_to_report_them() {
    let mut h = FakeHost::new("rt-exp-format");
    h.installed = vec![5];
    let a = h.raw("a.CR3");
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    let run = h.last_run().clone();
    fake_pureraw::run_as(&run.args, &run.tmp, Mode::Process, 5).unwrap();
    fs::write(run.tmp.join(results_file(5)), "{\"processed\": [\"a.CR3\"]}\n").unwrap();
    let req = h.end_request("exited");
    let reply = end(&mut h, &req).unwrap();
    assert_eq!(reply["added"], 0);
    let s = summary(&h);
    assert!(s.contains("Some results weren't as expected") && s.contains("tested with DxO PureRAW 5"), "{s}");
}
