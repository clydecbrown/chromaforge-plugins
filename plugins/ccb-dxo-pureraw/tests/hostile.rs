//! What's in the run's folder is input: none of it may trap the plug-in (AC-7). Each case is an error
//! or a skipped line in the log, and the hook still returns.

mod support;

use std::fs;

use ccb_dxo_pureraw::versions::{VERSION_FILE, results_file};
use ccb_dxo_pureraw::{RUN_RECORD, end};
use lcc_plugin_sdk::serde_json::Value;
use support::{FakeHost, MAX_IO_BYTES, send};

const HEADER: &str = "DxO PureRAW 3\nEND_OF_COLLECTION_SET_SECTION\nxEND_OF_COLLECTION_NAME_SECTION\n+ selectCollection\n";

/// Sends `a.dng` and `b.dng`, then ends the run with `results` as PureRAW's list (`{a}`, `{b}`: their
/// paths) and, if given, `record` as the run record.
fn ended_with(test: &str, results: Option<&str>, record: Option<&[u8]>) -> (FakeHost, Value) {
    let mut h = FakeHost::new(test);
    let a = h.raw("a.dng");
    let b = h.raw("b.dng");
    send(&mut h, &[a.clone(), b.clone()], &Value::Null).unwrap();
    let tmp = h.last_run().tmp.clone();
    let (pa, pb) = (h.path_of(&a), h.path_of(&b));
    fs::create_dir_all(h.root.join("DxO")).unwrap();
    fs::write(h.root.join("DxO/a.dng"), b"dng a").unwrap();
    if let Some(r) = results {
        fs::write(tmp.join(results_file(3)), r.replace("{a}", &pa).replace("{b}", &pb).replace("{dir}", &h.root.display().to_string())).unwrap();
    }
    if let Some(bytes) = record {
        fs::write(tmp.join(RUN_RECORD), bytes).unwrap();
    }
    let req = h.end_request("exited");
    let reply = end(&mut h, &req).expect("the hook returns");
    (h, reply)
}

#[test]
fn a_malformed_list_skips_the_lines_it_can_t_read_and_adds_the_rest() {
    let list = format!("{HEADER}- {{a}}\nnonsense\n0 {{dir}}/DxO/a.dng\n7 {{dir}}/DxO/b.dng\nx y\n");
    let (h, reply) = ended_with("hostile-malformed", Some(&list), None);
    assert_eq!(reply["added"], 1);
    assert!(h.logged("Skipped in DxO PureRAW's results: line 6 isn't a result line"), "{:?}", h.log);
    assert!(h.logged("points at photo 7, which the list doesn't have"), "{:?}", h.log);
}

#[test]
fn an_original_that_wasn_t_sent_is_skipped() {
    let list = format!("{HEADER}- /elsewhere/x.dng\n0 {{dir}}/DxO/a.dng\n");
    let (h, reply) = ended_with("hostile-unsent", Some(&list), None);
    assert_eq!(reply["added"], 0);
    assert!(h.adds.is_empty(), "never added on a photo it wasn't made from");
    assert!(h.logged("which this send didn't include"), "{:?}", h.log);
}

#[test]
fn an_output_that_doesn_t_exist_or_is_refused_is_logged_and_the_rest_still_come_in() {
    let list = format!("{HEADER}- {{a}}\n- {{b}}\n0 {{dir}}/DxO/a.dng\n1 {{dir}}/DxO/missing.dng\n");
    let (h, reply) = ended_with("hostile-refused", Some(&list), None);
    assert_eq!(reply["added"], 1);
    assert!(h.logged("missing.dng was not added:"), "{:?}", h.log);
    assert!(h.log.last().unwrap().contains("not added (see the lines above): missing.dng"), "{:?}", h.log);
}

#[test]
fn an_output_that_is_the_original_itself_is_refused_not_stacked_on_itself() {
    let list = format!("{HEADER}- {{a}}\n0 {{a}}\n");
    let (h, reply) = ended_with("hostile-self", Some(&list), None);
    assert_eq!(reply["added"], 0);
    assert!(h.logged("own original"), "{:?}", h.log);
}

#[test]
fn a_list_over_the_host_s_size_limit_adds_nothing_and_says_why() {
    let big = format!("{HEADER}{}", "x".repeat(MAX_IO_BYTES + 1));
    let (h, reply) = ended_with("hostile-big", Some(&big), None);
    assert_eq!(reply["added"], 0);
    assert!(h.logged("list of results couldn't be read") && h.logged("larger than"), "{:?}", h.log);
}

#[test]
fn a_list_that_isn_t_text_adds_nothing() {
    let mut h = FakeHost::new("hostile-binary");
    let a = h.raw("a.dng");
    send(&mut h, &[a], &Value::Null).unwrap();
    fs::write(h.last_run().tmp.join(results_file(3)), [0xff, 0xfe, 0x00]).unwrap();
    let req = h.end_request("exited");
    end(&mut h, &req).unwrap();
    assert!(h.adds.is_empty());
    assert!(h.logged("not UTF-8"), "{:?}", h.log);
}

#[test]
fn a_missing_or_damaged_run_record_adds_nothing() {
    let list = format!("{HEADER}- {{a}}\n0 {{dir}}/DxO/a.dng\n");
    for (name, record) in [
        ("garbage", &b"{not json"[..]),
        ("wrong-format", br#"{"format": 9, "sent": []}"#),
        ("not-text", &[0xff, 0x00][..]),
        ("no-list", br#"{"format": 1}"#),
    ] {
        let (h, reply) = ended_with(&format!("hostile-record-{name}"), Some(&list), Some(record));
        assert_eq!(reply["added"], 0, "{name}");
        assert!(h.adds.is_empty(), "{name}");
        assert!(h.logged("Nothing was added:") && h.logged("can't be matched to their photos"), "{name}: {:?}", h.log);
    }
    // gone altogether
    let mut h = FakeHost::new("hostile-record-gone");
    let a = h.raw("a.dng");
    send(&mut h, &[a], &Value::Null).unwrap();
    fs::remove_file(h.last_run().tmp.join(RUN_RECORD)).unwrap();
    let req = h.end_request("exited");
    end(&mut h, &req).unwrap();
    assert!(h.adds.is_empty() && h.logged("the run record couldn't be read"), "{:?}", h.log);
}

#[test]
fn a_list_naming_far_more_outputs_than_were_sent_stays_within_budget() {
    let mut list = format!("{HEADER}- {{a}}\n");
    for i in 0..20_000 {
        list.push_str(&format!("0 {{dir}}/DxO/{i}.dng\n"));
    }
    let (h, _) = ended_with("hostile-flood", Some(&list), None);
    assert!(h.adds.len() <= 4, "{} adds", h.adds.len());
    assert!(h.calls < 200, "{} host calls", h.calls);
    assert!(h.log.len() <= 60, "{} log lines", h.log.len());
    assert!(h.logged("they list more files than were sent"), "{:?}", h.log);
    assert!(h.logged("more lines, past what one send can bring in"), "{:?}", h.log);
}

#[test]
fn a_flood_of_results_from_originals_that_weren_t_sent_stays_within_budget() {
    let mut list = format!("{HEADER}- /elsewhere/x.dng\n");
    for i in 0..200_000 {
        list.push_str(&format!("0 /o/{i}\n"));
    }
    for _ in 0..200_000 {
        list.push_str("junk\n");
    }
    let (h, _) = ended_with("hostile-flood-unsent", Some(&list), None);
    assert!(h.adds.is_empty());
    assert!(h.log.len() <= 60, "{} log lines", h.log.len());
    assert!(h.logged("more lines like these"), "{:?}", h.log);
}

#[test]
fn one_huge_original_with_many_results_stays_within_budget() {
    // one long original and a list full of results for it: each result once kept its own copy of the path
    let long = format!("/{}", "d".repeat(4000));
    let mut list = format!("{HEADER}- {long}\n");
    while list.len() < MAX_IO_BYTES - 20 {
        list.push_str(&format!("0 /o/{}\n", list.len()));
    }
    let (h, _) = ended_with("hostile-huge-original", Some(&list), None);
    assert!(h.adds.is_empty());
    assert!(h.log.len() <= 60, "{} log lines", h.log.len());
}

#[test]
fn a_revoked_library_add_stops_with_what_to_allow() {
    let mut h = FakeHost::new("hostile-revoked");
    let a = h.raw("a.dng");
    let b = h.raw("b.dng");
    send(&mut h, &[a, b], &Value::Null).unwrap();
    let run = h.last_run().clone();
    fake_pureraw::run(&run.args, &run.tmp, fake_pureraw::Mode::Process).unwrap();
    h.library_add = false;
    let req = h.end_request("exited");
    end(&mut h, &req).unwrap();
    assert!(h.adds.is_empty());
    assert!(!h.logged("was not added"), "it stops at the first refusal, not one line per DNG: {:?}", h.log);
    let last = h.log.last().unwrap();
    assert!(last.contains("The DNGs from DxO PureRAW weren't added: allow \"Add files made from photos to the library\""), "{last}");
}

#[test]
fn every_log_line_fits_the_host_s_limit() {
    let mut h = FakeHost::new("hostile-long-names");
    let ids: Vec<String> = (0..30).map(|i| h.raw(&format!("{i}-{}.dng", "n".repeat(200)))).collect();
    send(&mut h, &ids, &Value::Null).unwrap();
    let tmp = h.last_run().tmp.clone();
    // results that all point outside the originals' folders, and long names everywhere
    let mut list = HEADER.to_string();
    for id in &ids {
        list.push_str(&format!("- {}\n", h.path_of(id)));
    }
    for (i, id) in ids.iter().enumerate() {
        let name = h.path_of(id).rsplit('/').next().unwrap().to_string();
        let out = h.root.parent().unwrap().join(format!("elsewhere-{}", std::process::id())).join(name);
        fs::create_dir_all(out.parent().unwrap()).unwrap();
        fs::write(&out, b"dng").unwrap();
        list.push_str(&format!("{i} {}\n", out.display()));
    }
    fs::write(tmp.join(results_file(3)), list).unwrap();
    let req = h.end_request("exited");
    end(&mut h, &req).unwrap();
    let _ = fs::remove_dir_all(h.root.parent().unwrap().join(format!("elsewhere-{}", std::process::id())));
    assert!(h.logged("outside their originals' folders (see above)"), "{:?}", h.log);
    assert!(h.log.iter().all(|l| l.len() <= 2048), "{:?}", h.log.iter().map(String::len).collect::<Vec<_>>());
}

#[test]
fn an_end_without_a_folder_or_with_an_odd_status_adds_nothing() {
    let mut h = FakeHost::new("hostile-odd");
    for req in [
        r#"{"hook": "program.end", "status": "exited", "tmpDir": ""}"#,
        r#"{"hook": "program.end", "status": "failed", "tmpDir": "/x"}"#,
        r#"{"hook": "program.end", "status": 7}"#,
        r#"{"hook": "program.end"}"#,
    ] {
        let req = lcc_plugin_sdk::Request(lcc_plugin_sdk::serde_json::from_str(req).unwrap());
        end(&mut h, &req).unwrap();
    }
    assert!(h.adds.is_empty());
    assert_eq!(h.log.len(), 4, "{:?}", h.log);
}

#[test]
fn a_damaged_version_file_adds_nothing_and_says_why() {
    for (name, text) in [("junk", "x".repeat(1000)), ("out-of-range", "10".into()), ("zero-padded", "03".into()), ("empty", String::new())] {
        let mut h = FakeHost::new(&format!("hostile-version-{name}"));
        let a = h.raw("a.dng");
        send(&mut h, &[a], &Value::Null).unwrap();
        let run = h.last_run().clone();
        fake_pureraw::run(&run.args, &run.tmp, fake_pureraw::Mode::Process).unwrap();
        fs::write(run.tmp.join(VERSION_FILE), &text).unwrap();
        let req = h.end_request("exited");
        let reply = end(&mut h, &req).expect("the hook returns");
        assert_eq!(reply["added"], 0, "{name}");
        assert!(h.logged("the run's version file is damaged"), "{name}: {:?}", h.log);
        assert!(h.log.iter().all(|l| l.len() < 300), "{name}: a long file isn't copied into the log");
    }
}
