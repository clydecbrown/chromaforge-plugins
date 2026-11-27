//! Which PureRAW runs: the prepare step's answer for what's installed and allowed and for the version
//! setting, the settings page's choices, and the command starting the version chosen.

mod support;

use ccb_dxo_pureraw::versions::{bundle_id, label};
use ccb_dxo_pureraw::{prepare, settings_prepare};
use lcc_plugin_sdk::serde_json::{Value, json};
use support::{FakeHost, send};

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
    assert_eq!(answer.as_object().map(|o| o.len()), Some(1), "exactly one of message or run: {answer}");
    let text = answer["message"].as_str().unwrap();
    assert!(text.contains("DxO PureRAW isn't installed") && text.contains("DxO PureRAW 6, DxO PureRAW 3 or DxO PureRAW 2"), "{text}");
    assert_eq!(h.calls, 2, "it only reads the settings and lists the programs");
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
    for version in [2, 3, 6] {
        let (mut h, a) = host(&format!("ver-one-{version}"), &[version]);
        let answer = prepared(&mut h, &a);
        assert_eq!(answer, json!({"run": {}}), "no dialog");
        let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
        assert_eq!(h.started.len(), 1, "{reply}");
        assert_eq!(h.last_run().version, version);
        assert_eq!(reply["version"], version);
    }
}

#[test]
fn with_several_installed_the_newest_runs_and_the_setting_chooses_another() {
    let (mut h, a) = host("ver-many", &[6, 2, 3]);
    let answer = prepared(&mut h, &a);
    assert_eq!(answer, json!({"run": {}}), "Automatic, with several installed: the newest, no dialog");
    assert_eq!(prepared(&mut h, &a), answer, "the same answer when the host asks again");
    assert!(h.started.is_empty(), "the prepare step starts nothing");

    h.set("version", json!(label(2)));
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert_eq!(h.last_run().version, 2, "{reply}");
    h.set("version", json!(label(6)));
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert_eq!(h.last_run().version, 6, "{reply}");
    assert_eq!(h.started.len(), 2, "one command, no second one");
}

#[test]
fn a_version_gone_or_no_longer_allowed_by_the_time_it_runs_starts_nothing() {
    let (mut h, a) = host("ver-gone", &[3, 2]);
    h.set("version", json!(label(2)));
    h.installed = vec![3];
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert_eq!(h.calls, 2, "refused from the settings and the list, before any photo is read");
    let expected = prepared(&mut h, &a)["message"].clone();
    assert_eq!(reply["message"], expected, "the same text as the prepare step gives");
    h.installed = vec![3, 2];
    h.granted = false;
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert!(reply["message"].as_str().unwrap().contains("Plug-in Manager"), "{reply}");
    h.granted = true;
    h.set("version", json!("Automatic"));
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert_eq!(h.last_run().version, 3, "Automatic is the newest installed: {reply}");
}

#[test]
fn a_version_removed_between_the_check_and_the_start_says_so() {
    let (mut h, a) = host("ver-race", &[3, 2]);
    h.set("version", json!(label(2)));
    h.gone_at_start = true;
    let reply = send(&mut h, &[a]).unwrap();
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
        h.set("version", json!(label(version)));
        send(&mut h, std::slice::from_ref(&a)).unwrap();
        let run = h.last_run().clone();
        assert_eq!(h.started_with.last().map(String::as_str), Some(bundle_id(version).as_str()));
        assert_eq!(run.files["version"], version.to_string(), "the run's folder says which version ran");
        runs.push(run.args.iter().map(|x| x.replace(&run.tmp.display().to_string(), "{tmpDir}")).collect::<Vec<_>>());
    }
    assert_eq!(runs[0], runs[1]);
    assert_eq!(runs[0], ["--as-lightroom-plugin", "--lr-version", "99.0", "--batch-file", "{tmpDir}/batch"]);
}

#[test]
fn pureraw_6_is_sent_like_the_others() {
    let (mut h, a) = host("ver-six-send", &[6]);
    let reply = send(&mut h, &[a]).unwrap();
    assert!(reply.get("experimental").is_none(), "{reply}");
    let text = reply["message"].as_str().unwrap();
    assert!(text.starts_with("Sent 1 photo to DxO PureRAW 6."), "{text}");
}

#[test]
fn the_settings_page_offers_automatic_and_each_version_installed_and_allowed() {
    let (mut h, _) = host("set-prepare", &[6, 2, 3]);
    h.preparing = true;
    let offered = json!(["Automatic", "DxO PureRAW 6", "DxO PureRAW 3", "DxO PureRAW 2"]);
    assert_eq!(settings_prepare(&mut h).unwrap(), json!({"fields": {"version": {"options": offered, "default": "Automatic"}}}));
    let only_automatic = json!({"fields": {"version": {"options": ["Automatic"], "default": "Automatic"}}});
    h.installed.clear();
    assert_eq!(settings_prepare(&mut h).unwrap(), only_automatic, "none installed");
    h.installed = vec![3];
    h.granted = false;
    assert_eq!(settings_prepare(&mut h).unwrap(), only_automatic, "not allowed");
    h.granted = true;
    h.old = true;
    assert_eq!(settings_prepare(&mut h).unwrap(), only_automatic, "the host can't list them");
}

#[test]
fn automatic_runs_the_newest_installed_at_once() {
    let (mut h, a) = host("set-auto", &[2, 3, 6]);
    for automatic in [None, Some(json!("Automatic")), Some(json!(""))] {
        if let Some(v) = automatic {
            h.set("version", v);
        }
        assert_eq!(prepared(&mut h, &a), json!({"run": {}}), "newest installed, no dialog");
    }
    h.installed = vec![2, 3];
    assert_eq!(prepared(&mut h, &a), json!({"run": {}}), "the newest one installed");
    h.installed = vec![2];
    assert_eq!(prepared(&mut h, &a), json!({"run": {}}), "the only one");
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert_eq!(h.last_run().version, 2, "{reply}");
}

#[test]
fn a_version_chosen_in_the_settings_runs_at_once_even_with_others_installed() {
    let (mut h, a) = host("set-named", &[2, 3, 6]);
    for (setting, version) in [("DxO PureRAW 2", 2), ("DxO PureRAW 3", 3), ("DxO PureRAW 6", 6)] {
        h.set("version", json!(setting));
        assert_eq!(prepared(&mut h, &a), json!({"run": {}}), "{setting}");
        h.busy = false;
        let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
        assert_eq!(h.last_run().version, version, "{setting}: a run without the value follows the setting too: {reply}");
    }
}

#[test]
fn a_version_chosen_in_the_settings_but_gone_or_unknown_names_it_and_starts_nothing() {
    let (mut h, a) = host("set-gone", &[3]);
    h.set("version", json!("DxO PureRAW 2"));
    let text = prepared(&mut h, &a)["message"].as_str().unwrap().to_string();
    assert!(text.starts_with("DxO PureRAW 2, chosen in this plug-in's settings, isn't installed any more, so nothing was sent."), "{text}");
    assert!(text.contains("Settings… on this plug-in's row"), "{text}");
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert_eq!(reply["message"], text, "the command refuses the same way");

    h.granted = false;
    let text = prepared(&mut h, &a)["message"].as_str().unwrap().to_string();
    assert!(text.contains("(the settings choose DxO PureRAW 2)") && text.contains("tick DxO PureRAW"), "{text}");
    h.granted = true;

    h.set("version", json!("DxO PureRAW 12"));
    let text = prepared(&mut h, &a)["message"].as_str().unwrap().to_string();
    assert!(text.contains("doesn't know (\"DxO PureRAW 12\")"), "{text}");

    h.installed.clear();
    h.set("version", json!("DxO PureRAW 2"));
    let text = prepared(&mut h, &a)["message"].as_str().unwrap().to_string();
    assert!(
        text.contains("DxO PureRAW isn't installed") && text.contains("DxO PureRAW 6, DxO PureRAW 3 or DxO PureRAW 2"),
        "none installed: 02's message: {text}"
    );
    assert!(h.started.is_empty());
}

#[test]
fn settings_the_host_can_t_give_start_nothing_rather_than_guess() {
    let (mut h, a) = host("set-broken", &[3, 2]);
    h.set("version", json!("DxO PureRAW 2"));
    h.settings_broken = true;
    let text = prepared(&mut h, &a)["message"].as_str().unwrap().to_string();
    assert!(text.starts_with("This plug-in's settings couldn't be read"), "{text}");
    let reply = send(&mut h, std::slice::from_ref(&a)).unwrap();
    assert!(reply["message"].as_str().unwrap().starts_with("This plug-in's settings couldn't be read"), "{reply}");
    assert!(h.started.is_empty());
}
