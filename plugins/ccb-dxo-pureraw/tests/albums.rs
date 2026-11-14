//! Where the DNGs go: the album the photos were sent from and the stack, as the settings were at send
//! time, round trip against the stand-in for PureRAW (`tools/fake-pureraw`).

mod support;

use ccb_dxo_pureraw::end;
use fake_pureraw::{Mode, output_of};
use lcc_plugin_sdk::serde_json::{Value, json};
use support::{FakeHost, send};

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

/// Sends `photos` from the session's source, lets the stand-in process them, and ends the run.
fn round_trip(h: &mut FakeHost, photos: &[&str]) -> Value {
    let sent = send(h, &ids(photos), &Value::Null).unwrap();
    assert_eq!(h.started.len(), 1, "{sent}");
    process(h)
}

/// The stand-in processes the last run's batch, then the run ends.
fn process(h: &mut FakeHost) -> Value {
    let run = h.last_run().clone();
    fake_pureraw::run(&run.args, &run.tmp, Mode::Process).unwrap();
    let req = h.end_request("exited");
    end(h, &req).unwrap()
}

fn summary(h: &FakeHost) -> &str {
    h.log.last().unwrap()
}

/// The library's photo for the DNG made from photo `id`.
fn dng_of(h: &FakeHost, id: &str) -> String {
    let out = output_of(&h.path_of(id)).unwrap();
    h.by_path(&out).unwrap_or_else(|| panic!("{out} isn't in the library")).id.clone()
}

/// Two raw photos in an album "Trip", shown in the session.
fn in_trip(test: &str) -> (FakeHost, String, String, String) {
    let mut h = FakeHost::new(test);
    let a = h.raw("a/1.CR3");
    let b = h.raw("b/2.CR3");
    let trip = h.album("Trip", &[&a, &b]);
    h.show_album(&trip);
    (h, a, b, trip)
}

#[test]
fn sent_from_an_album_each_dng_lands_in_it_unstacked() {
    let (mut h, a, b, trip) = in_trip("al-album");
    let sent = send(&mut h, &ids(&[&a, &b]), &Value::Null).unwrap();
    let text = sent["message"].as_str().unwrap();
    assert!(text.ends_with("the DNGs it makes are added to the album \"Trip\" when it's done."), "{text}");
    let reply = process(&mut h);
    assert_eq!(reply["added"], 2, "{reply}");
    let (da, db) = (dng_of(&h, &a), dng_of(&h, &b));
    assert_eq!(h.album_photos(&trip), [a.clone(), b.clone(), da, db], "next to their sources");
    assert!(h.stacked_on(&a).is_empty() && h.stacked_on(&b).is_empty(), "not stacked");
    assert!(h.adds.iter().all(|p| p["album"] == trip.as_str() && p["stack"] == false), "{:?}", h.adds);
    assert_eq!(summary(&h), "DxO PureRAW 3: added 2 DNGs to the album \"Trip\".");
}

#[test]
fn the_quick_collection_counts_as_an_album() {
    let mut h = FakeHost::new("al-quick");
    let a = h.raw("a.CR3");
    let quick = h.album("Quick Collection", &[&a]);
    h.source = json!({"kind": "album", "id": quick, "name": "Quick Collection", "quick": true});
    round_trip(&mut h, &[&a]);
    assert_eq!(h.album_photos(&quick), [a.clone(), dng_of(&h, &a)]);
    assert_eq!(summary(&h), "DxO PureRAW 3: added 1 DNG to the album \"Quick Collection\".");
}

#[test]
fn sent_from_a_folder_all_photographs_or_a_smart_album_the_dngs_get_no_album_and_the_summary_says_why() {
    let cases = [
        ("folder", None, "you sent the photos from a folder, not an album"),
        ("all", Some(json!({"kind": "allPhotos"})), "you sent the photos from All Photographs, not an album"),
        (
            "smart",
            Some(json!({"kind": "smartAlbum", "id": "7", "name": "Five stars"})),
            "you sent the photos from the smart album \"Five stars\", which picks its photos by its rules",
        ),
        ("view", Some(json!({"kind": "other", "name": "picks"})), "you sent the photos from a view that isn't an album"),
    ];
    for (name, source, why) in cases {
        let mut h = FakeHost::new(&format!("al-not-{name}"));
        let a = h.raw("a.CR3");
        // any album the photo is in elsewhere isn't used: only the view it was sent from counts
        let other = h.album("Elsewhere", &[&a]);
        if let Some(s) = source {
            h.source = s;
        }
        let sent = send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
        assert!(sent["message"].as_str().unwrap().ends_with(&format!("It won't go into an album: {why}.")), "{name}: {sent}");
        process(&mut h);
        assert_eq!(h.adds.len(), 1, "{name}");
        assert!(h.adds[0].get("album").is_none(), "{name}: {:?}", h.adds);
        assert_eq!(h.album_photos(&other), std::slice::from_ref(&a), "{name}");
        assert!(h.stacked_on(&a).is_empty(), "{name}");
        dng_of(&h, &a);
        assert_eq!(summary(&h), format!("DxO PureRAW 3: added 1 DNG; not put in an album: {why}."), "{name}");
    }
}

#[test]
fn the_album_and_settings_recorded_at_send_are_used_whatever_changed_in_between() {
    let (mut h, a, b, trip) = in_trip("al-recorded");
    send(&mut h, &ids(&[&a, &b]), &Value::Null).unwrap();
    // meanwhile: another view, and the opposite settings
    h.source = json!({"kind": "allPhotos"});
    h.set("addToAlbum", json!(false));
    h.set("stack", json!(true));
    process(&mut h);
    assert_eq!(h.album_photos(&trip).len(), 4, "into the album it was sent from");
    assert!(h.stacked_on(&a).is_empty() && h.stacked_on(&b).is_empty(), "stack was off when it was sent");
    assert_eq!(summary(&h), "DxO PureRAW 3: added 2 DNGs to the album \"Trip\".");
    // the next send takes the new settings
    h.busy = false;
    h.started.clear();
    h.set("version", json!("Automatic"));
    round_trip(&mut h, &[&a]);
    assert_eq!(h.stacked_on(&a).len(), 1);
}

#[test]
fn with_stack_on_each_dng_is_stacked_also_in_the_album_and_with_both_off_only_added() {
    let (mut h, a, b, trip) = in_trip("al-both");
    h.set("stack", json!(true));
    let sent = send(&mut h, &ids(&[&a, &b]), &Value::Null).unwrap();
    assert!(sent["message"].as_str().unwrap().contains("added to the album \"Trip\", stacked on their originals, when it's done"), "{sent}");
    process(&mut h);
    for id in [&a, &b] {
        let dng = dng_of(&h, id);
        assert_eq!(h.stacked_on(id).iter().map(|p| p.id.clone()).collect::<Vec<_>>(), std::slice::from_ref(&dng), "photo {id}");
        assert!(h.album_photos(&trip).contains(&dng));
    }
    assert_eq!(summary(&h), "DxO PureRAW 3: added 2 DNGs to the album \"Trip\", stacked on their originals.");

    let (mut h, a, b, trip) = in_trip("al-neither");
    h.set("addToAlbum", json!(false));
    let sent = send(&mut h, &ids(&[&a, &b]), &Value::Null).unwrap();
    assert!(sent["message"].as_str().unwrap().ends_with("the DNGs it makes are added to the library when it's done."), "{sent}");
    process(&mut h);
    assert_eq!(h.album_photos(&trip), [a.clone(), b.clone()], "no album");
    assert!(h.stacked_on(&a).is_empty() && h.stacked_on(&b).is_empty(), "no stack");
    assert!(h.adds.iter().all(|p| p.get("album").is_none() && p["stack"] == false), "{:?}", h.adds);
    dng_of(&h, &a);
    dng_of(&h, &b);
    assert_eq!(summary(&h), "DxO PureRAW 3: added 2 DNGs.");
}

#[test]
fn an_album_removed_mid_run_still_gets_every_dng_added_without_it_and_named() {
    let (mut h, a, b, _) = in_trip("al-gone");
    send(&mut h, &ids(&[&a, &b]), &Value::Null).unwrap();
    h.albums.clear();
    let reply = process(&mut h);
    assert_eq!(reply["added"], 2, "{reply}");
    dng_of(&h, &a);
    dng_of(&h, &b);
    assert_eq!(h.adds.len(), 3, "once the album is gone, the rest aren't tried with it: {:?}", h.adds);
    let s = summary(&h);
    assert!(s.starts_with("DxO PureRAW 3: added 2 DNGs; added without the album"), "none went into it: {s}");
    assert!(
        s.ends_with(
            "added without the album \"Trip\", which is gone or no longer holds their originals: 1-CR3_DxO_DeepPRIME.dng, 2-CR3_DxO_DeepPRIME.dng."
        ),
        "{s}"
    );
    assert!(h.logged("was added without the album: no album"), "{:?}", h.log);
}

#[test]
fn a_photo_that_left_the_album_mid_run_gets_its_dng_without_the_album_and_the_rest_go_in() {
    let (mut h, a, b, trip) = in_trip("al-left");
    send(&mut h, &ids(&[&a, &b]), &Value::Null).unwrap();
    h.albums[0].photos.retain(|p| *p != b);
    let reply = process(&mut h);
    assert_eq!(reply["added"], 2, "{reply}");
    let (da, db) = (dng_of(&h, &a), dng_of(&h, &b));
    assert_eq!(h.album_photos(&trip), [a.clone(), da], "only the one whose original is still there");
    assert!(!h.album_photos(&trip).contains(&db));
    let s = summary(&h);
    assert!(s.contains("added without the album \"Trip\", which is gone or no longer holds its original: 2-CR3_DxO_DeepPRIME.dng."), "{s}");
    assert!(h.logged("doesn't hold photo"), "{:?}", h.log);
    assert!(s.starts_with("DxO PureRAW 3: added 2 DNGs, 1 of them to the album \"Trip\"; "), "{s}");
}

#[test]
fn a_failure_that_isn_t_the_album_s_is_not_tried_again_without_it() {
    let (mut h, a, _, _) = in_trip("al-other-error");
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    let run = h.last_run().clone();
    fake_pureraw::run(&run.args, &run.tmp, Mode::Process).unwrap();
    std::fs::remove_file(output_of(&h.path_of(&a)).unwrap()).unwrap();
    let req = h.end_request("exited");
    let reply = end(&mut h, &req).unwrap();
    assert_eq!(h.adds.len(), 1, "one try: {:?}", h.adds);
    assert_eq!(reply["failed"].as_array().map(Vec::len), Some(1), "{reply}");
    assert!(!summary(&h).contains("without the album"), "{}", summary(&h));
}

#[test]
fn sending_the_same_photos_again_adds_nothing_twice_to_the_library_album_or_stack() {
    let (mut h, a, b, trip) = in_trip("al-again");
    h.set("stack", json!(true));
    round_trip(&mut h, &[&a, &b]);
    let photos = h.photos.len();
    h.started.clear();
    let reply = round_trip(&mut h, &[&a, &b]);
    assert_eq!(reply["added"], 0, "{reply}");
    assert_eq!(h.photos.len(), photos, "no second DNG in the library");
    assert_eq!(h.album_photos(&trip).len(), 4, "each DNG once in the album");
    assert_eq!(h.stacked_on(&a).len(), 1);
    assert_eq!(h.stacked_on(&b).len(), 1);
}

#[test]
fn a_virtual_copy_in_the_album_gets_the_dng_beside_it() {
    let mut h = FakeHost::new("al-copy");
    let master = h.raw("a.CR3");
    let copy = h.virtual_copy(&master);
    let trip = h.album("Trip", &[&copy]);
    h.show_album(&trip);
    round_trip(&mut h, &[&copy]);
    assert_eq!(h.adds[0]["from"], copy.as_str(), "added from the photo the album holds, not its master");
    assert_eq!(h.album_photos(&trip), [copy.clone(), dng_of(&h, &master)]);
}

#[test]
fn a_run_sent_before_the_settings_existed_stacks_its_dngs_as_it_did_then() {
    let (mut h, a, b, trip) = in_trip("al-old-run");
    send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    let run = h.last_run().clone();
    // a record written by 0.2.0: what was sent, no source or settings
    let record = format!(r#"{{"format": 1, "sent": [{{"photo": "{a}", "path": "{}"}}]}}"#, h.path_of(&a));
    std::fs::write(run.tmp.join("run.json"), record).unwrap();
    process(&mut h);
    assert_eq!(h.stacked_on(&a).len(), 1);
    assert_eq!(h.album_photos(&trip), [a.clone(), b], "no album then");
    assert_eq!(summary(&h), "DxO PureRAW 3: added 1 DNG, stacked on its original.");
}
