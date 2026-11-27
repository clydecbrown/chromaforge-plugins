//! Which versions of PureRAW the plug-in knows. Each major version installs as an app of its own, with
//! the bundle id `com.dxo-labs.PureRawv<N>.standalone` (PureRAW 6 spells it `PureRAWv6`: a Spotlight
//! `==` lookup of the other spelling finds nothing on the Mac it was checked on, so the manifest says it
//! that way), and names its results files
//! `…PureRawv<N>` the same way. [`DECLARED`] are the versions the manifest declares, and every one is
//! offered as tested: a version is declared only once its handoff has been probed and the stand-in
//! matches it. Nothing is offered as experimental.

use lcc_plugin_sdk::serde_json::Value;

/// The versions the manifest declares, newest first, with how DxO spells each one's bundle id and
/// file names. A version isn't offered unless it's here: PureRAW 4, 5, 7, 8 and 9 haven't been probed,
/// so their spelling is unknown.
pub const DECLARED: [(u32, &str); 3] = [(6, "PureRAWv"), (3, "PureRawv"), (2, "PureRawv")];
/// The most installs [`installed`] reads from the host's list: one per declared version, and as many
/// to spare.
const MAX_INSTALLS: usize = 16;

/// The declared versions, newest first.
pub fn declared() -> impl Iterator<Item = u32> {
    DECLARED.iter().map(|(v, _)| *v)
}

/// How DxO spells PureRAW `version`'s bundle id and file names, up to the number.
fn spelling(version: u32) -> &'static str {
    DECLARED.iter().find(|(v, _)| *v == version).map_or("PureRawv", |(_, s)| s)
}

/// The bundle id of PureRAW `version`.
pub fn bundle_id(version: u32) -> String {
    format!("com.dxo-labs.{}{version}.standalone", spelling(version))
}

/// The version a bundle id names, if it's one the manifest declares. The case of "Raw" doesn't matter.
pub fn version_of(bundle_id: &str) -> Option<u32> {
    let rest = bundle_id.strip_prefix("com.dxo-labs.")?.strip_suffix(".standalone")?;
    if !rest.get(..8)?.eq_ignore_ascii_case("PureRawv") {
        return None;
    }
    parse(rest.get(8..)?)
}

/// A declared version written as a plain number ("3", not "03" or "+3").
pub fn parse(n: &str) -> Option<u32> {
    declared().find(|v| v.to_string() == n)
}

/// The version's name: "DxO PureRAW 3".
pub fn name(version: u32) -> String {
    format!("{} {version}", crate::PROGRAM_NAME)
}

/// The version as the settings page and the summaries name it.
pub fn label(version: u32) -> String {
    name(version)
}

/// The version a setting names.
pub fn from_setting(text: &str) -> Option<u32> {
    declared().find(|v| name(*v) == text)
}

/// The declared versions, for a message: "DxO PureRAW 6, DxO PureRAW 3 or DxO PureRAW 2".
pub fn declared_names() -> String {
    let names: Vec<String> = declared().map(name).collect();
    match names.split_last() {
        Some((last, [])) => last.clone(),
        Some((last, rest)) => format!("{} or {last}", rest.join(", ")),
        None => crate::PROGRAM_NAME.into(),
    }
}

/// The run's folder holds the version that ran, so `program.end` reads the right files.
pub const VERSION_FILE: &str = "version";

/// PureRAW `version`'s results list, which it writes into its `$TMPDIR`: the run's folder.
pub fn results_file(version: u32) -> String {
    format!("dxo-imported-files-{}{version}", spelling(version))
}

/// What `program.list` says of the program: whether it's granted, and the declared versions installed,
/// newest first.
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
    versions.sort_by_key(|v| std::cmp::Reverse(*v));
    versions.dedup();
    Some(Installed { granted, versions })
}

#[cfg(test)]
mod tests {
    use lcc_plugin_sdk::serde_json::json;

    use super::*;

    #[test]
    fn bundle_ids_and_versions_go_both_ways_within_the_declared_range() {
        for v in declared() {
            assert_eq!(version_of(&bundle_id(v)), Some(v));
        }
        for id in [
            "com.dxo-labs.PureRawv1.standalone",
            "com.dxo-labs.PureRawv4.standalone",
            "com.dxo-labs.PureRawv10.standalone",
            "com.dxo-labs.PureRawv03.standalone",
            "com.dxo-labs.PureRawv.standalone",
            "com.dxo-labs.PureRawv+3.standalone",
            "com.dxo-labs.PureRawv3",
            "com.dxo-labs.PureRawv99999999999999999999.standalone",
            "com.dxo-labs.PureRAWv1.standalone",
            "com.dxo-labs.PureRAWv4.standalone",
            "com.dxo-labs.PureRAWv10.standalone",
            "com.dxo-labs.PureRAWv03.standalone",
            "com.dxo-labs.PureRAWv.standalone",
            "com.dxo-labs.PureRAWv+6.standalone",
            "com.dxo-labs.PureRAWv6",
            "com.dxo-labs.PureRAWv99999999999999999999.standalone",
            "",
        ] {
            assert_eq!(version_of(id), None, "{id}");
        }
    }

    #[test]
    fn pureraw_6_is_spelled_with_a_capital_raw_and_read_either_way() {
        assert_eq!(bundle_id(6), "com.dxo-labs.PureRAWv6.standalone", "the spelling DxO uses, which the host's lookup needs");
        assert_eq!(bundle_id(3), "com.dxo-labs.PureRawv3.standalone");
        assert_eq!(version_of("com.dxo-labs.PureRAWv6.standalone"), Some(6));
        assert_eq!(version_of("com.dxo-labs.PureRawv6.standalone"), Some(6), "a host that reports the other spelling");
        assert_eq!(version_of("com.dxo-labs.PURERAWV6.standalone"), Some(6), "the case of Raw doesn't matter");
        assert_eq!(results_file(6), "dxo-imported-files-PureRAWv6");
        assert_eq!(results_file(3), "dxo-imported-files-PureRawv3");
    }

    #[test]
    fn labels_name_each_declared_version_and_read_back() {
        assert_eq!(label(3), "DxO PureRAW 3");
        assert_eq!(label(2), "DxO PureRAW 2");
        assert_eq!(label(6), "DxO PureRAW 6");
        for v in declared() {
            assert_eq!(from_setting(&name(v)), Some(v));
        }
        assert_eq!(from_setting("DxO PureRAW 5"), None, "a setting naming an undeclared version finds nothing");
        assert_eq!(from_setting("DxO PureRAW 6 (experimental)"), None, "no version is offered as experimental");
        assert_eq!(from_setting("Automatic"), None);
        assert_eq!(declared_names(), "DxO PureRAW 6, DxO PureRAW 3 or DxO PureRAW 2");
    }

    #[test]
    fn installed_versions_are_offered_newest_first() {
        let ids: Vec<Value> = [2, 6, 3, 9, 4, 3].iter().map(|v| json!({"bundleId": bundle_id(*v)})).collect();
        let list = json!([{"program": "other"}, {"program": "pureraw", "granted": true, "installed": ids}]);
        assert_eq!(installed(&list), Some(Installed { granted: true, versions: vec![6, 3, 2] }));
    }

    #[test]
    fn an_installed_pureraw_6_is_found_whichever_spelling_the_host_reports() {
        for spelling in ["com.dxo-labs.PureRAWv6.standalone", "com.dxo-labs.PureRawv6.standalone"] {
            let list = json!([{"program": "pureraw", "granted": true, "installed": [{"bundleId": spelling}]}]);
            assert_eq!(installed(&list).map(|i| i.versions), Some(vec![6]), "{spelling}");
        }
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
