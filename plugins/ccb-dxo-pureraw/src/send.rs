//! Send Selected Photos. Its prepare step ([`prepare`]) finds the versions of PureRAW installed and,
//! with the version setting, answers with a message, the one to run, or a choice. The command
//! ([`send`]) checks the chosen one is still there and allowed, picks the photos it can take, writes
//! the batch, the run record (with where the command ran from and the settings) and the version, and
//! starts that PureRAW. It returns at once; [`crate::end`] takes the results. [`settings_prepare`]
//! offers the installed versions on the settings page.

use handoff::{Batch, file_name};
use lcc_plugin_sdk::serde_json::{Value, json};

use crate::filing::{AUTOMATIC, Album, Filing, Settings, VERSION_SETTING};
use crate::versions::{self, Installed, VERSION_FILE, bundle_id, from_label, from_setting, is_tested, label, tested_names};
use crate::{BATCH, Host, LR_VERSION, NEWER_CHROMAFORGE, PLUGIN_NAME, PROGRAM, PROGRAM_NAME, RUN_RECORD, VERSION_ARG, unknown_method};

/// The most photos one send takes, a rule of thumb. Every DNG is added in the one `program.end` call,
/// whose 120 s budget counts the time the app takes over each add: one live run on one loaded machine
/// took 0.27 to 0.7 s per DNG (about 100 MB each), so 250 took about 90 s there. A slower machine, or
/// PureRAW listing more outputs than photos, can still run out of time: the DNGs added so far stay.
pub const MAX_PHOTOS: usize = 250;
/// The most photos a send reads before choosing what to send: each is a host call or two, and a call
/// has a budget. Above [`MAX_PHOTOS`] so that skipped photos don't count against the limit.
pub const MAX_SELECTED: usize = 2000;

/// How many skipped photos a reply names before "and N more".
const MAX_NAMED: usize = 10;

fn message(text: impl Into<String>) -> Value {
    json!({"message": text.into()})
}

fn select_first() -> Value {
    message(format!("Select the raw photos to send to {PROGRAM_NAME} first."))
}

/// The prepare step: how Send Selected Photos runs on `selection`. No version installed and allowed
/// is a message; the version setting, or the only version installed, runs at once; when Automatic
/// leaves several (no tested one, more than one experimental) they're offered in the dialog. It only
/// reads, and gives the same answer for the same state: the host asks it twice.
pub fn prepare(host: &mut impl Host, selection: &[String]) -> Result<Value, String> {
    if selection.is_empty() {
        return Ok(select_first());
    }
    let settings = match Settings::read(host) {
        Ok(s) => s,
        Err(why) => return Ok(message(why)),
    };
    let installed = match installed(host, &settings)? {
        Ok(i) => i,
        Err(why) => return Ok(message(why)),
    };
    Ok(match decide(&installed, &settings) {
        Decision::Run(v) => json!({"run": {VERSION_ARG: label(v)}}),
        Decision::Ask(all) => {
            let labels: Vec<String> = all.iter().map(|v| label(*v)).collect();
            json!({"dialog": {VERSION_ARG: {"options": labels, "default": labels.first()}}})
        }
        Decision::Refuse(why) => message(why),
    })
}

/// The settings page's prepare step: the version setting offers Automatic and each version installed
/// and allowed. Only Automatic when the host can't say.
pub fn settings_prepare(host: &mut impl Host) -> Result<Value, String> {
    let mut options = vec![AUTOMATIC.to_string()];
    if let Some(i) = host.call("program.list", json!({})).ok().as_ref().and_then(versions::installed).filter(|i| i.granted) {
        options.extend(i.versions.iter().map(|v| label(*v)));
    }
    Ok(json!({"fields": {VERSION_SETTING: {"options": options, "default": AUTOMATIC}}}))
}

/// Which version runs.
#[derive(Debug, PartialEq, Eq)]
enum Decision {
    Run(u32),
    /// Automatic leaves these, in the order offered.
    Ask(Vec<u32>),
    /// Why none can, for the user.
    Refuse(String),
}

/// The version `settings` choose from `installed` (at least one, allowed): the one named, or with
/// Automatic the newest tested one, the only one, or a choice.
fn decide(installed: &Installed, settings: &Settings) -> Decision {
    if !settings.is_automatic() {
        return match from_setting(&settings.version) {
            Some(v) if installed.versions.contains(&v) => Decision::Run(v),
            Some(v) => Decision::Refuse(format!(
                "{}, chosen in this plug-in's settings, isn't installed any more, so nothing was sent. {}",
                versions::name(v),
                choose_in_settings()
            )),
            None => Decision::Refuse(format!(
                "This plug-in's settings choose a version of {PROGRAM_NAME} it doesn't know ({:?}), so nothing was sent. {}",
                crate::filing::shown(&settings.version),
                choose_in_settings()
            )),
        };
    }
    // tested versions come first, newest first
    match installed.versions.as_slice() {
        [first, ..] if is_tested(*first) => Decision::Run(*first),
        [only] => Decision::Run(*only),
        all => Decision::Ask(all.to_vec()),
    }
}

fn choose_in_settings() -> &'static str {
    "Choose another one, or Automatic, in its settings: File ▸ Plug-ins ▸ Plug-in Manager…, then Settings… on this plug-in's row."
}

/// The command: sends `selection` (photo ids) to the PureRAW `choice` names (a [`label`]; none: the
/// one the prepare step would choose), from `source` (the host's `source`: where it ran from).
/// Refusals are messages for the user, not errors; an error is something the host or the plug-in got
/// wrong.
pub fn send(host: &mut impl Host, selection: &[String], choice: &Value, source: &Value) -> Result<Value, String> {
    if selection.is_empty() {
        return Ok(select_first());
    }
    // too many to read through: refused as if all were raw
    if selection.len() > MAX_SELECTED {
        return Ok(message(too_many(selection.len())));
    }
    let settings = match Settings::read(host) {
        Ok(s) => s,
        Err(why) => return Ok(message(why)),
    };
    let version = match chosen(host, choice, &settings)? {
        Ok(v) => v,
        Err(why) => return Ok(message(why)),
    };
    let filing = Filing::new(&settings, source);
    if let Some(why) = cannot_add(host) {
        return Ok(message(why));
    }

    let mut batch = Batch::new();
    let mut skipped: Vec<String> = Vec::new();
    for photo in selection {
        let inspected = match host.call("photo.inspect", json!({"id": photo})) {
            Ok(v) => v,
            Err(e) if e.contains("permission `catalog` not granted") => {
                return Ok(message(format!(
                    "Allow \"Read the catalog\" for this plug-in in File ▸ Plug-ins ▸ Plug-in Manager…: it reads where each photo's file is, to send it to {PROGRAM_NAME}."
                )));
            }
            Err(e) if unknown_method(&e) => return Ok(message(NEWER_CHROMAFORGE)),
            Err(e) => {
                skipped.push(format!("photo {photo} (it couldn't be read: {e})"));
                continue;
            }
        };
        let name = inspected.get("fileName").or_else(|| inspected.get("file_name")).and_then(Value::as_str).unwrap_or(photo);
        let source = inspected.get("source");
        let path = match (source.and_then(|s| s.get("type")).and_then(Value::as_str), source.and_then(|s| s.get("path")).and_then(Value::as_str)) {
            (Some("file"), Some(path)) => path,
            _ => {
                skipped.push(format!("{name} (it has no file on disk)"));
                continue;
            }
        };
        if inspected.get("deleted").and_then(Value::as_bool) == Some(true) {
            skipped.push(format!("{name} (it's in Recently Deleted)"));
            continue;
        }
        // seen while browsing a folder, not in the library: its DNG couldn't be stacked on it
        if inspected.get("local").and_then(Value::as_bool) == Some(true) {
            skipped.push(format!("{name} (it isn't in the library, only in a folder you're browsing)"));
            continue;
        }
        if inspected.get("kind").and_then(Value::as_str) != Some("raw") {
            skipped.push(format!("{name} (it isn't a raw file)"));
            continue;
        }
        // a DNG for an album is added from the photo in it; otherwise a virtual copy's goes on its master
        let target = if filing.album_id().is_some() { photo.to_string() } else { stack_target(host, photo, &inspected) };
        if let Err(why) = batch.add(&target, path) {
            skipped.push(format!("{} ({})", file_name(path), why.reason()));
        }
    }

    if batch.len() > MAX_PHOTOS {
        return Ok(message(too_many(batch.len())));
    }
    let left_out = named(&skipped);
    if batch.is_empty() {
        return Ok(message(format!("Nothing was sent to {PROGRAM_NAME}: {left_out}.")));
    }
    let args = ["--as-lightroom-plugin", "--lr-version", LR_VERSION, "--batch-file", &format!("{{tmpDir}}/{BATCH}")];
    let mut record = batch.record();
    record.source = source.clone();
    record.settings = Some(settings.to_map());
    let files = json!({BATCH: batch.text(), RUN_RECORD: record.to_json(), VERSION_FILE: version.to_string()});
    let started = match host.call("program.start", json!({"program": PROGRAM, "args": args, "files": files, "bundleId": bundle_id(version)})) {
        Ok(v) => v,
        Err(e) => return Ok(message(start_failed(&e, version))),
    };
    let run = started.get("run").and_then(Value::as_str).unwrap_or_default();
    let count = batch.len();
    let photos = if count == 1 { "1 photo".to_string() } else { format!("{count} photos") };
    let made = where_they_go(&filing, count == 1);
    let name = versions::name(version);
    let mut text = if is_tested(version) {
        format!("Sent {photos} to {name}.")
    } else {
        format!("Sent {photos} to {name}, which this plug-in hasn't been tested with (experimental).")
    };
    text.push_str(&format!(" Choose its settings in its window; {made} when it's done."));
    if let Album::Not(why) = &filing.album {
        text.push_str(&format!(" {} won't go into an album: {why}.", if count == 1 { "It" } else { "They" }));
    }
    if !skipped.is_empty() {
        text.push_str(&format!(" Left out: {left_out}."));
    }
    host.log(&format!("{run}: sent {photos} to {}", label(version)));
    Ok(json!({"message": text, "run": run, "sent": count, "skipped": skipped, "version": version, "experimental": !is_tested(version)}))
}

fn not_installed() -> String {
    format!(
        "{PROGRAM_NAME} isn't installed on this computer, or not in a version this plug-in knows ({} to {}; a later one needs an update \
         of the plug-in). It's tested with {}: install one, then send the photos again. (This plug-in works on macOS only for now.)",
        versions::OLDEST,
        versions::NEWEST,
        tested_names()
    )
}

/// "the DNGs it makes are …", as `filing` puts them.
fn where_they_go(filing: &Filing, one: bool) -> String {
    let (dngs, are, originals) = if one { ("the DNG it makes", "is", "its original") } else { ("the DNGs it makes", "are", "their originals") };
    match (&filing.album, filing.stack) {
        (Album::Into { name, .. }, true) => format!("{dngs} {are} added to the album \"{name}\", stacked on {originals},"),
        (Album::Into { name, .. }, false) => format!("{dngs} {are} added to the album \"{name}\""),
        (_, true) => format!("{dngs} {are} stacked on {originals}"),
        (_, false) => format!("{dngs} {are} added to the library"),
    }
}

/// The refusal while the program isn't allowed; it names the version the settings choose, if any.
fn not_granted(settings: Option<&Settings>) -> String {
    let wanted =
        settings.filter(|s| !s.is_automatic()).map(|s| format!(" (the settings choose {})", crate::filing::shown(&s.version))).unwrap_or_default();
    format!(
        "Allow {PROGRAM_NAME} for this plug-in first{wanted}: File ▸ Plug-ins ▸ Plug-in Manager…, then tick {PROGRAM_NAME} on this plug-in's row."
    )
}

/// The versions installed and allowed, at least one; or why there's none, for the user. An `Err` is
/// the host's.
fn installed(host: &mut impl Host, settings: &Settings) -> Result<Result<Installed, String>, String> {
    let list = match host.call("program.list", json!({})) {
        Ok(v) => v,
        Err(e) if unknown_method(&e) => return Ok(Err(NEWER_CHROMAFORGE.into())),
        Err(e) => return Err(e),
    };
    let Some(installed) = versions::installed(&list) else {
        return Err(format!("the host doesn't list the program {PROGRAM:?} this plug-in declares"));
    };
    if installed.versions.is_empty() {
        return Ok(Err(not_installed()));
    }
    if !installed.granted {
        return Ok(Err(not_granted(Some(settings))));
    }
    Ok(Ok(installed))
}

/// The version `choice` names, if it's still installed and allowed; or why it can't run, for the user.
/// No choice (a run without the dialog's value) is the prepare step's: the one `settings` choose, or
/// the first one offered.
fn chosen(host: &mut impl Host, choice: &Value, settings: &Settings) -> Result<Result<u32, String>, String> {
    let installed = match installed(host, settings)? {
        Ok(i) => i,
        Err(why) => return Ok(Err(why)),
    };
    // a run without the dialog's value gets the field's declared default: empty
    if choice.is_null() || choice.as_str() == Some("") {
        return Ok(match decide(&installed, settings) {
            Decision::Run(v) => Ok(v),
            Decision::Ask(all) => all.first().copied().ok_or_else(not_installed),
            Decision::Refuse(why) => Err(why),
        });
    }
    let version = match choice {
        Value::String(text) => match from_label(text) {
            Some(v) if installed.versions.contains(&v) => Some(v),
            Some(v) => return Ok(Err(gone(v))),
            None => None,
        },
        _ => None,
    };
    Ok(version.ok_or_else(|| format!("This plug-in doesn't know the version {choice}. Send the photos again and choose one from the list.")))
}

/// The refusal for a chosen `version` that isn't installed now.
fn gone(version: u32) -> String {
    format!(
        "{} isn't installed any more, so nothing was sent. Send the photos again to choose from the versions installed now.",
        versions::name(version)
    )
}

/// Why the DNGs couldn't come back, before anything is read: adding to the library not allowed.
fn cannot_add(host: &mut impl Host) -> Option<String> {
    // Without libraryAdd, PureRAW's DNGs couldn't come back: say so before the user spends time there.
    // The host checks the permission before the params, so a call without params asks only that and
    // adds nothing (Chromaforge's docs don't promise that order yet).
    match host.call("library.add", Value::Null) {
        Err(e) if e.contains("permission `libraryAdd` not granted") => Some(format!(
            "Allow \"Add files made from photos to the library\" for this plug-in first: File ▸ Plug-ins ▸ Plug-in Manager…. \
             It adds the DNGs {PROGRAM_NAME} makes to the library."
        )),
        Err(e) if unknown_method(&e) => Some(NEWER_CHROMAFORGE.into()),
        _ => None,
    }
}

/// The photo the DNG is stacked on. A virtual copy shares its master's file, so that's the master, unless
/// the master isn't in the library (in Recently Deleted, only browsed, gone): then the copy itself.
fn stack_target(host: &mut impl Host, photo: &str, inspected: &Value) -> String {
    let master = inspected.get("copy_of").and_then(|m| m.as_u64().map(|n| n.to_string()).or_else(|| m.as_str().map(str::to_string)));
    let Some(master) = master else { return photo.to_string() };
    let in_library = host
        .call("photo.inspect", json!({"id": master}))
        .is_ok_and(|m| m.get("deleted").and_then(Value::as_bool) != Some(true) && m.get("local").and_then(Value::as_bool) != Some(true));
    if in_library { master } else { photo.to_string() }
}

/// The refusal for a send of `count` raw photos (those left out don't count), over [`MAX_PHOTOS`], or of
/// `count` photos over [`MAX_SELECTED`].
fn too_many(count: usize) -> String {
    format!("{PLUGIN_NAME} can process up to {MAX_PHOTOS} DNGs at a time. {count} are selected.")
}

/// What to tell the user when `program.start` refuses.
fn start_failed(e: &str, version: u32) -> String {
    if e.contains("already running") {
        format!(
            "{PROGRAM_NAME} is still open from the last send. Process or close the photos there first, then send these. \
             (✕ on its row in the activity list stops waiting for it.)"
        )
    } else if e.contains("permission `programs` not granted") {
        not_granted(None)
    } else if e.contains("is not installed") {
        gone(version)
    } else if unknown_method(e) {
        NEWER_CHROMAFORGE.into()
    } else {
        format!("{} could not be started: {e}", versions::name(version))
    }
}

/// "a, b and c", or the first [`MAX_NAMED`] and "N more".
fn named(items: &[String]) -> String {
    let shown: Vec<&str> = items.iter().take(MAX_NAMED).map(String::as_str).collect();
    let more = items.len().saturating_sub(shown.len());
    match (shown.split_last(), more) {
        (None, _) => "none of the selected photos can be sent".into(),
        (Some((last, [])), 0) => (*last).to_string(),
        (Some((last, rest)), 0) => format!("{} and {last}", rest.join(", ")),
        (Some(_), more) => format!("{} and {more} more", shown.join(", ")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn named_lists_a_few_and_counts_the_rest() {
        let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(named(&s(&["a"])), "a");
        assert_eq!(named(&s(&["a", "b", "c"])), "a, b and c");
        let many: Vec<String> = (0..12).map(|i| i.to_string()).collect();
        assert_eq!(named(&many), "0, 1, 2, 3, 4, 5, 6, 7, 8, 9 and 2 more");
    }
}
