//! ccb: Edit in DxO PureRAW. Sends the selected raw photos to DxO PureRAW the way its Lightroom Classic
//! plug-in does, and when PureRAW exits, adds each DNG it made to the library: in the album the photos
//! were sent from, stacked on the photo it came from, or both, as the settings say ([`filing`]).
//!
//! - [`prepare`]: before the menu command runs, finds which versions of PureRAW are installed
//!   ([`versions`]): none is a message; the version setting, or the only one installed, runs at once;
//!   more are a choice. [`settings_prepare`] offers the installed ones on the settings page.
//! - [`send`]: the menu command. Writes the batch (one absolute path per line), `run.json` (which
//!   photo each path came from, where the command ran from and the settings) and the version into the
//!   run's folder and starts that PureRAW on it.
//! - [`end`]: the `program.end` hook. Reads PureRAW's results list from the run's folder
//!   ([`results`]) and adds each output with `library.add`, as recorded at send time.
//!
//! Every call into the host goes through [`Host`], so the logic runs natively in tests.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

mod end;
pub mod filing;
pub mod results;
mod send;
pub mod versions;

use lcc_plugin_sdk::serde_json::Value;
use lcc_plugin_sdk::{Request, export_plugin};

pub use end::end;
pub use send::{MAX_PHOTOS, MAX_SELECTED, prepare, send, settings_prepare};

pub const MANIFEST: &str = concat!(
    r#"{"id": "io.github.clydecbrown.dxo-pureraw", "name": "ccb: Edit in DxO PureRAW", "version": ""#,
    env!("CARGO_PKG_VERSION"),
    r#"", "description": "Sends raw photos to DxO PureRAW and adds the DNGs it makes to the album you sent them from.",
  "author": "Clyde C Brown",
  "requires": ["programs", "libraryAdd", "settings"],
  "permissions": {
    "catalog": true,
    "libraryAdd": true,
    "programs": [{"id": "pureraw", "name": "DxO PureRAW", "macBundleIds": [
      "com.dxo-labs.PureRawv9.standalone", "com.dxo-labs.PureRawv8.standalone", "com.dxo-labs.PureRawv7.standalone",
      "com.dxo-labs.PureRawv6.standalone", "com.dxo-labs.PureRawv5.standalone", "com.dxo-labs.PureRawv4.standalone",
      "com.dxo-labs.PureRawv3.standalone", "com.dxo-labs.PureRawv2.standalone"]}]
  },
  "commands": [{"id": "send", "label": "Send Selected Photos", "prepare": true,
    "dialog": [{"key": "version", "label": "Version", "type": "choice", "options": []}]}],
  "settings": [
    {"key": "addToAlbum", "label": "Add to the album I sent from", "type": "bool", "default": true},
    {"key": "stack", "label": "Stack on the original", "type": "bool", "default": false},
    {"key": "version", "label": "PureRAW version", "type": "choice", "options": [], "prepare": true}]
}"#
);

/// The plug-in's name, as the manifest gives it (a test keeps the two the same).
pub const PLUGIN_NAME: &str = "ccb: Edit in DxO PureRAW";
/// The program's id in the manifest.
pub const PROGRAM: &str = "pureraw";
/// What the user knows it as.
pub const PROGRAM_NAME: &str = "DxO PureRAW";
/// The batch file, in the run's folder. PureRAW reads and deletes it.
pub const BATCH: &str = "batch";
/// What was sent, in the run's folder (`handoff::RunRecord`).
pub const RUN_RECORD: &str = "run.json";
/// The dialog field that holds the version to run ([`versions::label`]).
pub const VERSION_ARG: &str = "version";
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
            "send" => send(&mut Sdk, &req.selection(), req.arg(VERSION_ARG), req.source()),
            other => Err(format!("unknown command {other}")),
        },
        "command.prepare" => match req.command() {
            "send" => prepare(&mut Sdk, &req.selection()),
            other => Err(format!("unknown command {other}")),
        },
        "settings.prepare" => settings_prepare(&mut Sdk),
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
        assert_eq!(m["name"], PLUGIN_NAME);
        assert_eq!(m["version"], env!("CARGO_PKG_VERSION"));
        assert_eq!(m["requires"], serde_json::json!(["programs", "libraryAdd", "settings"]));
        let perms = m["permissions"].as_object().unwrap();
        assert_eq!(perms.keys().collect::<Vec<_>>(), ["catalog", "libraryAdd", "programs"], "nothing else is asked for");
        assert_eq!(perms["catalog"], true);
        assert_eq!(perms["libraryAdd"], true);
        let ids: Vec<String> = (versions::OLDEST..=versions::NEWEST).rev().map(versions::bundle_id).collect();
        assert_eq!(ids.len(), 8, "the host takes at most 8 bundle ids per program");
        assert_eq!(perms["programs"], serde_json::json!([{"id": PROGRAM, "name": PROGRAM_NAME, "macBundleIds": ids}]), "9 down to 2, newest first");
        assert_eq!(
            m["commands"],
            serde_json::json!([{"id": "send", "label": "Send Selected Photos", "prepare": true,
                "dialog": [{"key": VERSION_ARG, "label": "Version", "type": "choice", "options": []}]}])
        );
    }

    #[test]
    fn the_manifest_declares_the_three_settings_with_their_defaults() {
        let m: Value = serde_json::from_str(MANIFEST).unwrap();
        assert_eq!(
            m["settings"],
            serde_json::json!([
                {"key": filing::ADD_TO_ALBUM, "label": "Add to the album I sent from", "type": "bool", "default": true},
                {"key": filing::STACK, "label": "Stack on the original", "type": "bool", "default": false},
                // no options declared: the host then keeps any version saved, even one uninstalled since
                {"key": filing::VERSION_SETTING, "label": "PureRAW version", "type": "choice", "options": [], "prepare": true}])
        );
        let defaults = filing::Settings::default();
        assert_eq!((defaults.add_to_album, defaults.stack), (true, false), "the same defaults as the manifest");
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
