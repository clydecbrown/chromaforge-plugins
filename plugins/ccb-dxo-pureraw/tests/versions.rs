//! Which PureRAW runs: the prepare step's answer for what's installed and allowed, and the command
//! starting the version chosen (AC-2 to AC-5, AC-7).

mod support;

use ccb_dxo_pureraw::versions::{bundle_id, label};
use ccb_dxo_pureraw::{prepare, send};
use lcc_plugin_sdk::serde_json::{Value, json};
use support::FakeHost;

/// A host with `a.CR3` in the library and `installed` versions of PureRAW; the photo's id.
fn host(test: &str, installed: &[u32]) -> (FakeHost, String) {
    let mut h = FakeHost::new(test);
    h.installed = installed.to_vec();
    let a = h.raw("a.CR3");
    (h, a)
}

/// The prepare step's answer, asked as the host asks it: only reads allowed.
fn prepared(h: &mut FakeHost, photo: &str) -> Value {
    h.preparing = true;
    let answer = prepare(h, &[photo.to_string()]).expect("the prepare step answers");
    h.preparing = false;
    answer
}

#[test]
fn with_no_version_installed_the_prepare_step_names_the_tested_ones_and_nothing_starts() {
    let (mut h, a) = host("ver-none", &[]);
    let answer = prepared(&mut h, &a);
    assert_eq!(answer.as_object().map(|o| o.len()), Some(1), "exactly one of message, run, dialog: {answer}");
    let text = answer["message"].as_str().unwrap();
    assert!(text.contains("DxO PureRAW isn't installed") && text.contains("tested with DxO PureRAW 3 or 2") && text.contains("2 to 9"), "{text}");
    assert_eq!(h.calls, 1, "it only lists the programs");
    assert!(h.started.is_empty());
}

#[test]
fn installed_but_not_allowed_points_to_the_plugin_manager() {
    let (mut h, a) = host("ver-ungranted", &[3, 2]);
    h.granted = false;
    let text = prepared(&mut h, &a)["message"].as_str().unwrap().to_string();
    assert!(text.contains("Plug-in Manager") && text.contains("tick DxO PureRAW"), "{text}");
}

#[test]
fn an_older_chromaforge_or_no_selection_is_a_message() {
    let (mut h, a) = host("ver-old", &[3]);
    h.old = true;
    assert!(prepared(&mut h, &a)["message"].as_str().unwrap().contains("needs a newer Chromaforge"));
    h.old = false;
    let answer = prepare(&mut h, &[]).unwrap();
    assert!(answer["message"].as_str().unwrap().contains("Select the raw photos"), "{answer}");
}

#[test]
fn with_one_version_installed_it_runs_at_once_and_that_version_starts() {
    for (version, name) in [(2, "DxO PureRAW 2"), (3, "DxO PureRAW 3"), (6, "DxO PureRAW 6 (experimental)")] {
        let (mut h, a) = host(&format!("ver-one-{version}"), &[version]);
        let answer = prepared(&mut h, &a);
        assert_eq!(answer, json!({"run": {"version": name}}), "no dialog");
        let reply = send(&mut h, std::slice::from_ref(&a), &answer["run"]["version"]).unwrap();
        assert_eq!(h.started.len(), 1, "{reply}");
        assert_eq!(h.last_run().version, version);
        assert_eq!(reply["version"], version);
    }
}

#[test]
fn with_several_installed_the_dialog_offers_only_those_tested_first_and_the_chosen_one_starts() {
    let (mut h, a) = host("ver-many", &[5, 2, 3]);
    let answer = prepared(&mut h, &a);
    let offered = json!(["DxO PureRAW 3", "DxO PureRAW 2", "DxO PureRAW 5 (experimental)"]);
    assert_eq!(answer, json!({"dialog": {"version": {"options": offered, "default": "DxO PureRAW 3"}}}));
    assert_eq!(prepared(&mut h, &a), answer, "the same answer when the host asks again");
    assert!(h.started.is_empty(), "the prepare step starts nothing");

    let reply = send(&mut h, std::slice::from_ref(&a), &json!("DxO PureRAW 2")).unwrap();
    assert_eq!(h.last_run().version, 2, "{reply}");
    let reply = send(&mut h, std::slice::from_ref(&a), &json!("DxO PureRAW 5 (experimental)")).unwrap();
    assert_eq!(h.last_run().version, 5, "{reply}");
    assert_eq!(h.started.len(), 2, "one command, no second one");
}

#[test]
fn a_version_gone_or_no_longer_allowed_by_the_time_it_runs_starts_nothing() {
    let (mut h, a) = host("ver-gone", &[3, 2]);
    let choice = json!(label(2));
    h.installed = vec![3];
    let reply = send(&mut h, std::slice::from_ref(&a), &choice).unwrap();
    assert_eq!(
        reply["message"],
        "DxO PureRAW 2 isn't installed any more, so nothing was sent. Send the photos again to choose from the versions installed now."
    );
    assert_eq!(h.calls, 1, "refused from the list, before any photo is read");
    h.installed = vec![3, 2];
    h.granted = false;
    let reply = send(&mut h, std::slice::from_ref(&a), &choice).unwrap();
    assert!(reply["message"].as_str().unwrap().contains("Plug-in Manager"), "{reply}");
    h.granted = true;
    let reply = send(&mut h, std::slice::from_ref(&a), &json!("")).unwrap();
    assert_eq!(h.last_run().version, 3, "an empty value is the prepare step's first choice: {reply}");
    h.started.clear();
    for odd in [json!("DxO PureRAW 10"), json!("DxO PureRAW 5"), json!(3), json!(["DxO PureRAW 3"])] {
        let reply = send(&mut h, std::slice::from_ref(&a), &odd).unwrap();
        assert!(reply["message"].as_str().unwrap().contains("doesn't know the version"), "{odd}: {reply}");
    }
    assert!(h.started.is_empty());
}

#[test]
fn a_version_removed_between_the_check_and_the_start_says_so() {
    let (mut h, a) = host("ver-race", &[3, 2]);
    h.gone_at_start = true;
    let reply = send(&mut h, &[a], &json!(label(2))).unwrap();
    assert_eq!(
        reply["message"],
        "DxO PureRAW 2 isn't installed any more, so nothing was sent. Send the photos again to choose from the versions installed now."
    );
    assert!(h.started.is_empty());
}

#[test]
fn pureraw_2_starts_with_the_same_arguments_as_pureraw_3_and_its_own_bundle_id() {
    let (mut h, a) = host("ver-args", &[3, 2]);
    let mut runs = Vec::new();
    for version in [3, 2] {
        send(&mut h, std::slice::from_ref(&a), &json!(label(version))).unwrap();
        let run = h.last_run().clone();
        assert_eq!(h.started_with.last().map(String::as_str), Some(bundle_id(version).as_str()));
        assert_eq!(run.files["version"], version.to_string(), "the run's folder says which version ran");
        runs.push(run.args.iter().map(|x| x.replace(&run.tmp.display().to_string(), "{tmpDir}")).collect::<Vec<_>>());
    }
    assert_eq!(runs[0], runs[1]);
    assert_eq!(runs[0], ["--as-lightroom-plugin", "--lr-version", "99.0", "--batch-file", "{tmpDir}/batch"]);
}

#[test]
fn an_experimental_version_says_so_when_it_s_sent() {
    let (mut h, a) = host("ver-exp-send", &[7]);
    let reply = send(&mut h, &[a], &Value::Null).unwrap();
    assert_eq!(reply["experimental"], true);
    let text = reply["message"].as_str().unwrap();
    assert!(text.starts_with("Sent 1 photo to DxO PureRAW 7, which this plug-in hasn't been tested with (experimental)."), "{text}");
}
