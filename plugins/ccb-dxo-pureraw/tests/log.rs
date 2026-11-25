//! The log is the whole record: every reply the plug-in shows the user is also in its log (T-8F57).

mod support;

use ccb_dxo_pureraw::{prepare, send as send_hook};
use lcc_plugin_sdk::serde_json::{Value, json};
use support::{FakeHost, send};

fn ids(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

fn message(v: &Value) -> &str {
    v["message"].as_str().unwrap()
}

/// The reply's words are in one logged line, which never holds the folder the photos are in.
fn logged_once(h: &FakeHost, reply: &Value, what: &str) {
    let text = message(reply);
    let lines: Vec<&String> = h.log.iter().filter(|l| l.contains(text)).collect();
    assert_eq!(lines.len(), 1, "{what}: {text:?} in the log {:?}", h.log);
    let root = h.root.to_string_lossy();
    assert!(h.log.iter().all(|l| !l.contains(root.as_ref())), "{what}: a path in the log {:?}", h.log);
}

#[test]
fn every_reply_a_send_returns_is_also_in_the_log_in_the_same_words() {
    type Case = (&'static str, fn(&mut FakeHost));
    let cases: [Case; 6] = [
        ("not-installed", |h| h.installed.clear()),
        ("not-granted", |h| h.granted = false),
        ("old-host", |h| h.old = true),
        ("no-library-add", |h| h.library_add = false),
        ("no-catalog", |h| h.catalog = false),
        ("busy", |h| h.busy = true),
    ];
    for (name, set) in cases {
        let mut h = FakeHost::new(&format!("log-{name}"));
        let a = h.raw("a.dng");
        set(&mut h);
        let reply = send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
        logged_once(&h, &reply, name);
    }
    let mut h = FakeHost::new("log-empty");
    let reply = send(&mut h, &[], &Value::Null).unwrap();
    logged_once(&h, &reply, "nothing selected");
    let many: Vec<String> = (0..=ccb_dxo_pureraw::MAX_SELECTED).map(|i| i.to_string()).collect();
    let reply = send(&mut h, &many, &Value::Null).unwrap();
    logged_once(&h, &reply, "too many to read");
    let mut h = FakeHost::new("log-over");
    let raws: Vec<String> = (0..=ccb_dxo_pureraw::MAX_PHOTOS).map(|i| h.raw(&format!("r{i}.CR3"))).collect();
    let reply = send(&mut h, &raws, &Value::Null).unwrap();
    logged_once(&h, &reply, "too many raw");
    let mut h = FakeHost::new("log-nothing");
    let jpeg = h.file("a.jpg", "image");
    let reply = send(&mut h, &ids(&[&jpeg]), &Value::Null).unwrap();
    logged_once(&h, &reply, "nothing sendable");
}

#[test]
fn the_version_the_choice_names_being_gone_is_logged() {
    let mut h = FakeHost::new("log-gone");
    let a = h.raw("a.dng");
    let reply = send_hook(&mut h, &ids(&[&a]), &json!("DxO PureRAW 9"), &Value::Null).unwrap();
    logged_once(&h, &reply, "version gone");
}

#[test]
fn a_run_that_wouldn_t_start_is_logged() {
    let mut h = FakeHost::new("log-nostart");
    let a = h.raw("a.dng");
    h.busy = true;
    let reply = send(&mut h, &ids(&[&a]), &Value::Null).unwrap();
    assert!(message(&reply).contains("still open from the last send"));
    logged_once(&h, &reply, "wouldn't start");
}

/// A host whose `program.list` fails: an error the host shows, not a refusal.
struct Broken(FakeHost);

impl ccb_dxo_pureraw::Host for Broken {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        if method == "program.list" { Err("boom".into()) } else { self.0.call(method, params) }
    }
    fn log(&mut self, line: &str) {
        self.0.log(line);
    }
}

#[test]
fn an_error_the_host_shows_is_logged_too() {
    let mut h = Broken(FakeHost::new("log-err"));
    let a = h.0.raw("a.dng");
    let e = send_hook(&mut h, &ids(&[&a]), &Value::Null, &Value::Null).unwrap_err();
    assert!(e.contains("boom"), "{e}");
    assert!(h.0.log.iter().any(|l| l.contains("boom")), "{:?}", h.0.log);
    let before = h.0.log.len();
    let e = prepare(&mut h, &ids(&[&a])).unwrap_err();
    assert!(h.0.log.iter().skip(before).any(|l| l.contains(&e)), "{e}: {:?}", h.0.log);
}

#[test]
fn a_send_that_starts_a_run_logs_one_line_with_the_run_id_the_album_reason_and_the_photos_left_out() {
    let mut h = FakeHost::new("log-sent");
    let a = h.raw("a.CR3");
    let jpeg = h.file("b.jpg", "image");
    let reply = send(&mut h, &ids(&[&a, &jpeg]), &Value::Null).unwrap();
    let run = reply["run"].as_str().unwrap();
    assert_eq!(h.log.len(), 1, "an action is in the log once: {:?}", h.log);
    assert_eq!(h.log[0], format!("{run}: {}", message(&reply)));
    assert!(h.log[0].contains("won't go into an album") && h.log[0].contains("Left out: b.jpg (it isn't a raw file)"), "{:?}", h.log);
    logged_once(&h, &reply, "sent");
}

#[test]
fn a_prepare_step_refusal_is_logged_each_time_it_runs() {
    let mut h = FakeHost::new("log-prepare");
    let a = h.raw("a.dng");
    h.installed.clear();
    let reply = prepare(&mut h, &ids(&[&a])).unwrap();
    assert_eq!(h.log, [message(&reply)], "{:?}", h.log);
    let reply = prepare(&mut h, &[]).unwrap();
    assert_eq!(h.log.last().map(String::as_str), Some(message(&reply)));
    // an answer that isn't a refusal logs nothing
    let mut h = FakeHost::new("log-prepare-run");
    let a = h.raw("a.dng");
    assert!(prepare(&mut h, &ids(&[&a])).unwrap().get("run").is_some());
    assert!(h.log.is_empty(), "{:?}", h.log);
}

#[test]
fn a_line_over_the_hosts_limit_is_cut_at_a_character_and_counts_the_photos_left_out() {
    let mut h = FakeHost::new("log-long");
    let long = "é".repeat(120);
    let files: Vec<String> = (0..15).map(|i| h.file(&format!("{long}{i}.jpg"), "image")).collect();
    let reply = send(&mut h, &files, &Value::Null).unwrap();
    assert!(message(&reply).len() > 2048, "the reply itself is longer than a log line: {}", message(&reply).len());
    let line = h.log.last().unwrap();
    assert!(line.len() <= 2048, "{} bytes", line.len());
    assert!(line.ends_with("… (15 left out in all)"), "{line}");
    assert_eq!(h.log.len(), 1);
}

#[test]
fn no_module_but_send_returns_a_message_and_send_logs_every_one() {
    let src = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src");
    for entry in std::fs::read_dir(&src).unwrap() {
        let path = entry.unwrap().path();
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let text = std::fs::read_to_string(&path).unwrap();
        let returns = text.lines().filter(|l| !l.trim_start().starts_with("//") && (l.contains("\"message\"") || l.contains("message("))).count();
        match name.as_str() {
            "send.rs" => {
                // the two entry points log what their steps return; nothing returns around them
                assert!(text.contains("let reply = prepare_step(host, selection);\n    log_result(host, &reply);"), "prepare logs its reply");
                assert!(
                    text.contains("let reply = send_step(host, selection, choice, source);\n    log_result(host, &reply);"),
                    "send logs its reply"
                );
            }
            _ => assert_eq!(returns, 0, "{name} returns a message that nothing logs: route it through send.rs's log_reply or log it"),
        }
    }
}
