//! Send Selected Photos (AC-2, AC-3).

mod support;

use ccb_dxo_pureraw::{MAX_PHOTOS, MAX_SELECTED};
use lcc_plugin_sdk::serde_json::json;
use support::{FakeHost, send};

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn message(v: &lcc_plugin_sdk::serde_json::Value) -> &str {
    v["message"].as_str().unwrap()
}

#[test]
fn the_batch_holds_each_path_once_and_pureraw_starts_with_exactly_the_handoff_arguments() {
    let mut h = FakeHost::new("send-batch");
    let b = h.raw("b/IMG_2.CR3");
    let a = h.raw("a b/é/IMG_1.dng");
    let reply = send(&mut h, &ids(&[&b, &a, &b])).unwrap();
    assert_eq!(h.started.len(), 1, "one run");
    let run = h.last_run().clone();
    let batch = run.files["batch"].as_str().unwrap();
    assert_eq!(batch, format!("{}\n{}\n", h.path_of(&b), h.path_of(&a)), "one absolute path per line, each once, each ending in \\n");
    assert_eq!(run.args, ["--as-lightroom-plugin", "--lr-version", "99.0", "--batch-file", &format!("{}/batch", run.tmp.display())]);
    assert!(run.files.contains_key("run.json"), "the run record goes next to the batch");
    assert_eq!(reply["sent"], 2);
    assert!(message(&reply).starts_with("Sent 2 photos to DxO PureRAW 3."), "{reply}");
    assert!(h.adds.is_empty(), "it returns at once: nothing is added until PureRAW ends");
}

#[test]
fn photos_pureraw_can_t_take_are_left_out_and_named() {
    let mut h = FakeHost::new("send-skip");
    let raw = h.raw("ok.CR3");
    let demo = h.photo(None, "raw");
    let jpeg = h.file("holiday.jpg", "image");
    let video = h.file("clip.mov", "video");
    let newline = h.raw("bad\nname.dng");
    let cr = h.raw("bad\rname.dng");
    let deleted = h.raw("gone.dng");
    h.photos.iter_mut().find(|p| p.id == deleted).unwrap().deleted = true;
    let browsed = h.raw("browsed.dng");
    h.photos.iter_mut().find(|p| p.id == browsed).unwrap().local = true;
    let reply = send(&mut h, &ids(&[&raw, &demo, &jpeg, &video, &newline, &cr, &deleted, &browsed])).unwrap();
    assert_eq!(h.last_run().files["batch"].as_str().unwrap(), format!("{}\n", h.path_of(&raw)));
    let text = message(&reply);
    for needle in [
        "Demo (it has no file on disk)",
        "holiday.jpg (it isn't a raw file)",
        "clip.mov (it isn't a raw file)",
        "gone.dng (it's in Recently Deleted)",
        "browsed.dng (it isn't in the library",
    ] {
        assert!(text.contains(needle), "{needle:?} in {text}");
    }
    assert_eq!(text.matches("line break").count(), 2, "{text}");
    assert_eq!(reply["skipped"].as_array().unwrap().len(), 7);
}

#[test]
fn with_nothing_left_to_send_nothing_starts() {
    let mut h = FakeHost::new("send-none");
    let demo = h.photo(None, "raw");
    let jpeg = h.file("a.jpg", "image");
    let reply = send(&mut h, &ids(&[&demo, &jpeg])).unwrap();
    assert!(h.started.is_empty());
    assert!(message(&reply).starts_with("Nothing was sent to DxO PureRAW: Demo (it has no file on disk) and a.jpg (it isn't a raw file)"), "{reply}");
    let reply = send(&mut h, &[]).unwrap();
    assert!(h.started.is_empty());
    assert!(message(&reply).contains("Select the raw photos"), "{reply}");
}

#[test]
fn pureraw_missing_not_allowed_or_still_open_says_what_to_do_and_starts_nothing() {
    // the last number: the host calls made. Not installed or not allowed is found out before any
    // photo is read.
    type Case = (&'static str, fn(&mut FakeHost), &'static str, usize);
    let cases: [Case; 6] = [
        ("not-installed", |h| h.installed.clear(), "Install one, then send the photos again. (This plug-in works on macOS only", 2),
        ("not-granted", |h| h.granted = false, "tick DxO PureRAW on this plug-in's row", 2),
        ("old-host", |h| h.old = true, "needs a newer Chromaforge", 2),
        ("no-library-add", |h| h.library_add = false, "Allow \"Add files made from photos to the library\" for this plug-in first", 3),
        ("no-catalog", |h| h.catalog = false, "Allow \"Read the catalog\"", 4),
        ("busy", |h| h.busy = true, "still open from the last send", 5),
    ];
    for (name, set, needle, calls) in cases {
        let mut h = FakeHost::new(&format!("send-{name}"));
        let a = h.raw("a.dng");
        set(&mut h);
        let reply = send(&mut h, &ids(&[&a])).unwrap();
        assert!(message(&reply).contains(needle), "{name}: {reply}");
        assert!(h.started.is_empty(), "{name}: nothing starts");
        assert_eq!(h.calls, calls, "{name}: host calls");
    }
}

#[test]
fn one_photo_is_told_about_in_the_singular() {
    let mut h = FakeHost::new("send-one");
    let a = h.raw("a.CR3");
    let reply = send(&mut h, &ids(&[&a])).unwrap();
    assert_eq!(
        message(&reply),
        "Sent 1 photo to DxO PureRAW 3. Choose its settings in its window; the DNG it makes is added to the library when it's done. \
         It won't go into an album: you sent the photos from a folder, not an album."
    );
    h.set("stack", json!(true));
    h.set("addToAlbum", json!(false));
    let reply = send(&mut h, &ids(&[&a])).unwrap();
    assert_eq!(
        message(&reply),
        "Sent 1 photo to DxO PureRAW 3. Choose its settings in its window; the DNG it makes is stacked on its original when it's done."
    );
}

#[test]
fn a_selection_too_big_to_read_is_refused_before_anything_is_read() {
    let mut h = FakeHost::new("send-many");
    let many: Vec<String> = (0..=MAX_SELECTED).map(|i| i.to_string()).collect();
    let reply = send(&mut h, &many).unwrap();
    let expected = format!("ccb: Edit in DxO PureRAW can process up to {MAX_PHOTOS} DNGs at a time. {} are selected.", MAX_SELECTED + 1);
    assert_eq!(message(&reply), expected, "{reply}");
    assert_eq!(h.calls, 0);
}

#[test]
fn more_raw_photos_than_one_send_takes_are_refused_with_the_limit_and_nothing_starts() {
    let mut h = FakeHost::new("send-over");
    let raws: Vec<String> = (0..=MAX_PHOTOS).map(|i| h.raw(&format!("r{i}.CR3"))).collect();
    let reply = send(&mut h, &raws).unwrap();
    let expected = format!("ccb: Edit in DxO PureRAW can process up to {MAX_PHOTOS} DNGs at a time. {} are selected.", MAX_PHOTOS + 1);
    assert_eq!(message(&reply), expected, "{reply}");
    assert!(h.started.is_empty(), "PureRAW isn't started");
}

#[test]
fn photos_left_out_don_t_count_against_the_limit() {
    let mut h = FakeHost::new("send-full");
    let mut selection: Vec<String> = (0..MAX_PHOTOS).map(|i| h.raw(&format!("r{i}.CR3"))).collect();
    selection.extend((0..20).map(|i| h.file(&format!("j{i}.jpg"), "image")));
    let reply = send(&mut h, &selection).unwrap();
    assert_eq!(reply["sent"], MAX_PHOTOS, "{reply}");
    assert_eq!(h.started.len(), 1);
}

#[test]
fn a_virtual_copy_sends_its_master_s_file_once_and_the_dng_goes_on_the_master() {
    for copy_first in [true, false] {
        let mut h = FakeHost::new(&format!("send-copy-{copy_first}"));
        let master = h.raw("a.CR3");
        let copy = h.virtual_copy(&master);
        let only_copy = send(&mut h, &ids(&[&copy])).unwrap();
        assert_eq!(only_copy["sent"], 1);
        let record = h.last_run().files["run.json"].as_str().unwrap().to_string();
        assert!(record.contains(&format!("\"photo\":\"{master}\"")), "a copy alone is sent for its master: {record}");
        let selection = if copy_first { ids(&[&copy, &master]) } else { ids(&[&master, &copy]) };
        h.busy = false;
        send(&mut h, &selection).unwrap();
        let run = h.last_run().clone();
        assert_eq!(run.files["batch"].as_str().unwrap(), format!("{}\n", h.path_of(&master)), "the shared file once");
        let record = run.files["run.json"].as_str().unwrap();
        assert!(record.contains(&format!("\"photo\":\"{master}\"")) && !record.contains(&format!("\"photo\":\"{copy}\"")), "{record}");
    }
}

#[test]
fn a_virtual_copy_whose_master_left_the_library_gets_the_dng_itself() {
    for (name, set) in [("deleted", (|p: &mut support::LibPhoto| p.deleted = true) as fn(&mut support::LibPhoto)), ("local", |p| p.local = true)] {
        let mut h = FakeHost::new(&format!("send-copy-master-{name}"));
        let master = h.raw("a.CR3");
        let copy = h.virtual_copy(&master);
        set(h.photos.iter_mut().find(|p| p.id == master).unwrap());
        let reply = send(&mut h, &ids(&[&copy])).unwrap();
        assert_eq!(reply["sent"], 1, "{name}: {reply}");
        let record = h.last_run().files["run.json"].as_str().unwrap();
        assert!(record.contains(&format!("\"photo\":\"{copy}\"")), "{name}: {record}");
    }
}
