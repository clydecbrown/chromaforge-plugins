//! Which versions of PureRAW the plug-in knows. Each major version installs as an app of its own, with
//! the bundle id `com.dxo-labs.PureRawv<N>.standalone`, and names its results files with `v<N>`. The
//! manifest declares [`OLDEST`] to [`NEWEST`]; [`TESTED`] are the ones probed for a full, a cancelled
//! and a closed run. The rest run with PureRAW 3's handoff, as experimental.

use lcc_plugin_sdk::serde_json::Value;

/// The newest version the manifest declares (a later one needs a plug-in update to be granted).
pub const NEWEST: u32 = 9;
/// The oldest version the manifest declares.
pub const OLDEST: u32 = 2;
/// The versions tested, newest first.
pub const TESTED: [u32; 2] = [3, 2];
/// The most installs [`installed`] reads from the host's list: one per declared version, and as many
/// to spare.
const MAX_INSTALLS: usize = 16;

/// The bundle id of PureRAW `version`.
pub fn bundle_id(version: u32) -> String {
    format!("com.dxo-labs.PureRawv{version}.standalone")
}

/// The version a bundle id names, if it's one the manifest declares.
pub fn version_of(bundle_id: &str) -> Option<u32> {
    let n = bundle_id.strip_prefix("com.dxo-labs.PureRawv")?.strip_suffix(".standalone")?;
    parse(n)
}

/// A declared version written as a plain number ("3", not "03" or "+3").
pub fn parse(n: &str) -> Option<u32> {
    (OLDEST..=NEWEST).find(|v| v.to_string() == n)
}

pub fn is_tested(version: u32) -> bool {
    TESTED.contains(&version)
}

/// The version's name: "DxO PureRAW 3".
pub fn name(version: u32) -> String {
    format!("{} {version}", crate::PROGRAM_NAME)
}

/// The version as the dialog offers it: its name, marked when it's experimental.
pub fn label(version: u32) -> String {
    if is_tested(version) { name(version) } else { format!("{} (experimental)", name(version)) }
}

/// The version a [`label`] stands for.
pub fn from_label(text: &str) -> Option<u32> {
    (OLDEST..=NEWEST).find(|v| label(*v) == text)
}

/// The version a setting names: its name, marked experimental or not, so a version saved while
/// experimental is still found once it's tested.
pub fn from_setting(text: &str) -> Option<u32> {
    let unmarked = text.strip_suffix(" (experimental)").unwrap_or(text);
    (OLDEST..=NEWEST).find(|v| name(*v) == unmarked)
}

/// The tested versions, for a message: "DxO PureRAW 3 or 2".
pub fn tested_names() -> String {
    let rest: Vec<String> = TESTED.iter().skip(1).map(u32::to_string).collect();
    match TESTED.first() {
        Some(first) if rest.is_empty() => name(*first),
        Some(first) => format!("{} or {}", name(*first), rest.join(" or ")),
        None => crate::PROGRAM_NAME.into(),
    }
}

/// The run's folder holds the version that ran, so `program.end` reads the right files.
pub const VERSION_FILE: &str = "version";

/// PureRAW `version`'s results list, which it writes into its `$TMPDIR`: the run's folder.
pub fn results_file(version: u32) -> String {
    format!("dxo-imported-files-PureRawv{version}")
}

/// What `program.list` says of the program: whether it's granted, and the declared versions installed,
/// in the order they're offered: tested ones newest first, then experimental ones newest first.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Installed {
    pub granted: bool,
    pub versions: Vec<u32>,
}

/// Reads the program's entry in `program.list`'s reply. `None` when the reply doesn't list it.
pub fn installed(list: &Value) -> Option<Installed> {
    let entry = list.as_array()?.iter().find(|p| p.get("program").and_then(Value::as_str) == Some(crate::PROGRAM))?;
    let granted = entry.get("granted").and_then(Value::as_bool) == Some(true);
    let mut versions: Vec<u32> = entry
        .get("installed")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .take(MAX_INSTALLS)
        .filter_map(|i| i.get("bundleId").and_then(Value::as_str).and_then(version_of))
        .collect();
    versions.sort_by_key(|v| (!is_tested(*v), std::cmp::Reverse(*v)));
    versions.dedup();
    Some(Installed { granted, versions })
}

#[cfg(test)]
mod tests {
    use lcc_plugin_sdk::serde_json::json;

    use super::*;

    #[test]
    fn bundle_ids_and_versions_go_both_ways_within_the_declared_range() {
        for v in OLDEST..=NEWEST {
            assert_eq!(version_of(&bundle_id(v)), Some(v));
        }
        for id in [
            "com.dxo-labs.PureRawv1.standalone",
            "com.dxo-labs.PureRawv10.standalone",
            "com.dxo-labs.PureRawv03.standalone",
            "com.dxo-labs.PureRawv.standalone",
            "com.dxo-labs.PureRawv+3.standalone",
            "com.dxo-labs.PureRawv3",
            "com.dxo-labs.PureRawv99999999999999999999.standalone",
            "",
        ] {
            assert_eq!(version_of(id), None, "{id}");
        }
    }

    #[test]
    fn labels_mark_experimental_versions_and_read_back() {
        assert_eq!(label(3), "DxO PureRAW 3");
        assert_eq!(label(2), "DxO PureRAW 2");
        assert_eq!(label(5), "DxO PureRAW 5 (experimental)");
        for v in OLDEST..=NEWEST {
            assert_eq!(from_label(&label(v)), Some(v));
        }
        assert_eq!(from_label("DxO PureRAW 5"), None, "an experimental version is chosen by its marked label");
        assert_eq!(from_label("DxO PureRAW 10"), None);
        assert_eq!(from_setting("DxO PureRAW 5"), Some(5), "a setting may name it either way");
        assert_eq!(from_setting("DxO PureRAW 3 (experimental)"), Some(3), "saved before it was tested");
        assert_eq!(from_setting("DxO PureRAW 10"), None);
        assert_eq!(from_setting("Automatic"), None);
        assert_eq!(tested_names(), "DxO PureRAW 3 or 2");
    }

    #[test]
    fn installed_versions_are_offered_tested_first_then_newest_first() {
        let ids: Vec<Value> = [2, 5, 3, 9, 4, 3].iter().map(|v| json!({"bundleId": bundle_id(*v)})).collect();
        let list = json!([{"program": "other"}, {"program": "pureraw", "granted": true, "installed": ids}]);
        assert_eq!(installed(&list), Some(Installed { granted: true, versions: vec![3, 2, 9, 5, 4] }));
    }

    #[test]
    fn a_strange_list_gives_what_it_can_and_never_panics() {
        assert_eq!(installed(&json!({"not": "a list"})), None);
        assert_eq!(installed(&json!([{"program": "other"}])), None);
        let odd = json!([{"program": "pureraw", "granted": "yes", "installed": [
            {"bundleId": 3}, {"path": "/x"}, "junk", {"bundleId": "com.dxo-labs.PureRawv1.standalone"}, {"bundleId": bundle_id(2)}]}]);
        assert_eq!(installed(&odd), Some(Installed { granted: false, versions: vec![2] }));
        let flood: Vec<Value> = (0..1000).map(|_| json!({"bundleId": "x"})).chain([json!({"bundleId": bundle_id(3)})]).collect();
        let flood = json!([{"program": "pureraw", "granted": true, "installed": flood}]);
        assert_eq!(installed(&flood).map(|i| i.versions), Some(vec![]), "past the cap, nothing more is read");
    }
}
