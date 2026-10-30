//! CCB: Edit in DxO PureRAW. Sends the selected raw photos to DxO PureRAW 3 the way its Lightroom
//! Classic plug-in does, and when PureRAW exits, adds each DNG it made to the library, stacked on the
//! photo it came from.
//!
//! - [`send`]: the menu command. Writes the batch (one absolute path per line) and `run.json` (which
//!   photo each path came from) into the run's folder and starts PureRAW on it.
//! - [`end`]: the `program.end` hook. Reads PureRAW's results list from the run's folder
//!   ([`results`]) and adds each output with `library.add`.
//!
//! Every call into the host goes through [`Host`], so the logic runs natively in tests.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod end;
pub mod results;
mod send;

use lcc_plugin_sdk::serde_json::Value;
use lcc_plugin_sdk::{Request, export_plugin};

pub use end::end;
pub use send::{MAX_PHOTOS, send};

pub const MANIFEST: &str = concat!(
    r#"{"id": "io.github.clydecbrown.dxo-pureraw", "name": "CCB: Edit in DxO PureRAW", "version": ""#,
    env!("CARGO_PKG_VERSION"),
    r#"", "description": "Sends raw photos to DxO PureRAW 3 and stacks the DNGs it makes on their originals.",
  "author": "Clyde C Brown",
  "requires": ["programs", "libraryAdd"],
  "permissions": {
    "catalog": true,
    "libraryAdd": true,
    "programs": [{"id": "pureraw", "name": "DxO PureRAW", "macBundleIds": ["com.dxo-labs.PureRawv3.standalone"]}]
  },
  "commands": [{"id": "send", "label": "Send Selected Photos"}]
}"#
);

/// The program's id in the manifest.
pub const PROGRAM: &str = "pureraw";
/// What the user knows it as.
pub const PROGRAM_NAME: &str = "DxO PureRAW";
/// The batch file, in the run's folder. PureRAW reads and deletes it.
pub const BATCH: &str = "batch";
/// What was sent, in the run's folder (`handoff::RunRecord`).
pub const RUN_RECORD: &str = "run.json";
/// PureRAW 3's results list, which it writes into its `$TMPDIR`: the run's folder.
pub const RESULTS: &str = "dxo-imported-files-PureRawv3";
/// The Lightroom version passed to PureRAW. No Lightroom has it: with an installed version, PureRAW
/// sometimes starts Lightroom Classic when it's done, and that Lightroom stays suspended.
pub const LR_VERSION: &str = "99.0";

/// The host's calls, so the logic can run against a stand-in.
pub trait Host {
    /// `{method, params}` → the host's reply, or its error message.
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String>;
    /// A line in the plug-in's log (Plug-in Manager ▸ Log).
    fn log(&mut self, line: &str);
}

/// The real host, through the SDK.
pub struct Sdk;

impl Host for Sdk {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        lcc_plugin_sdk::host::call(method, params)
    }
    fn log(&mut self, line: &str) {
        lcc_plugin_sdk::host::log(line);
    }
}

/// A Chromaforge from before the features this plug-in requires answers a host call it doesn't know
/// this way (versions before `requires` install the plug-in anyway).
fn unknown_method(e: &str) -> bool {
    e.contains("unknown host method")
}

const NEWER_CHROMAFORGE: &str = "This plug-in needs a newer Chromaforge, one with plug-in programs and library.add (v26.44.2 or later).";

fn handle(req: &Request) -> Result<Value, String> {
    match req.hook() {
        "command" => match req.command() {
            "send" => send(&mut Sdk, &req.selection()),
            other => Err(format!("unknown command {other}")),
        },
        "program.end" => end(&mut Sdk, req),
        other => Err(format!("unsupported hook {other}")),
    }
}

export_plugin!(MANIFEST, handle);

#[cfg(test)]
mod tests {
    use lcc_plugin_sdk::serde_json;

    use super::*;

    #[test]
    fn the_manifest_names_the_plugin_its_permissions_and_what_it_requires() {
        let m: Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(m["id"], "io.github.clydecbrown.dxo-pureraw");
        assert_eq!(m["name"], "CCB: Edit in DxO PureRAW");
        assert_eq!(m["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(m["requires"], serde_json::json!(["programs", "libraryAdd"]));
        let perms = m["permissions"].as_object().unwrap();
        assert_eq!(perms.keys().collect::<Vec<_>>(), ["catalog", "libraryAdd", "programs"], "nothing else is asked for");
        assert_eq!(perms["catalog"], true);
        assert_eq!(perms["libraryAdd"], true);
        assert_eq!(
            perms["programs"],
            serde_json::json!([{"id": PROGRAM, "name": PROGRAM_NAME, "macBundleIds": ["com.dxo-labs.PureRawv3.standalone"]}])
        );
        assert_eq!(m["commands"], serde_json::json!([{"id": "send", "label": "Send Selected Photos"}]));
    }

    #[test]
    fn the_version_is_semver() {
        let v = env!("CARGO_PKG_VERSION");
        let core = v.split(['-', '+']).next().unwrap();
        let parts: Vec<_> = core.split('.').collect();
        assert_eq!(parts.len(), 3, "{v}");
        assert!(parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit())), "{v}");
    }
}
