//! Send Selected Photos: checks PureRAW is there and allowed, picks the photos it can take, writes the
//! batch and the run record, and starts PureRAW. It returns at once; [`crate::end`] takes the results.

use handoff::{Batch, file_name};
use lcc_plugin_sdk::serde_json::{Value, json};

use crate::{BATCH, Host, LR_VERSION, NEWER_CHROMAFORGE, PROGRAM, PROGRAM_NAME, RUN_RECORD, unknown_method};

/// The most photos one send takes: each costs a host call here and a `library.add` at the end, and
/// both calls have budgets. More can go in another send.
pub const MAX_PHOTOS: usize = 500;
const NOT_INSTALLED: &str =
    "DxO PureRAW 3 isn't installed on this computer. Install it, then send the photos again. (This plug-in works on macOS only for now.)";

/// How many skipped photos a reply names before "and N more".
const MAX_NAMED: usize = 10;

fn message(text: impl Into<String>) -> Value {
    json!({"message": text.into()})
}

/// The command: sends `selection` (photo ids) to PureRAW. Refusals are messages for the user, not
/// errors; an error is something the host or the plug-in got wrong.
pub fn send(host: &mut impl Host, selection: &[String]) -> Result<Value, String> {
    if selection.is_empty() {
        return Ok(message(format!("Select the raw photos to send to {PROGRAM_NAME} first.")));
    }
    if selection.len() > MAX_PHOTOS {
        return Ok(message(format!("Send at most {MAX_PHOTOS} photos at a time ({} are selected).", selection.len())));
    }
    if let Some(why) = not_ready(host)? {
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
        let target = stack_target(host, photo, &inspected);
        if let Err(why) = batch.add(&target, path) {
            skipped.push(format!("{} ({})", file_name(path), why.reason()));
        }
    }

    let left_out = named(&skipped);
    if batch.is_empty() {
        return Ok(message(format!("Nothing was sent to {PROGRAM_NAME}: {left_out}.")));
    }
    let args = ["--as-lightroom-plugin", "--lr-version", LR_VERSION, "--batch-file", &format!("{{tmpDir}}/{BATCH}")];
    let files = json!({BATCH: batch.text(), RUN_RECORD: batch.record().to_json()});
    let started = match host.call("program.start", json!({"program": PROGRAM, "args": args, "files": files})) {
        Ok(v) => v,
        Err(e) => return Ok(message(start_failed(&e))),
    };
    let run = started.get("run").and_then(Value::as_str).unwrap_or_default();
    let count = batch.len();
    let photos = if count == 1 { "1 photo".to_string() } else { format!("{count} photos") };
    let mut text = format!(
        "Sent {photos} to {PROGRAM_NAME}. Choose its settings in its window; the DNGs it makes are stacked on their originals when it's done."
    );
    if !skipped.is_empty() {
        text.push_str(&format!(" Left out: {left_out}."));
    }
    host.log(&format!("{run}: sent {photos} to {PROGRAM_NAME}"));
    Ok(json!({"message": text, "run": run, "sent": count, "skipped": skipped}))
}

/// Why PureRAW can't be started now, before anything is read: not installed, or it or adding to the
/// library not allowed.
fn not_ready(host: &mut impl Host) -> Result<Option<String>, String> {
    let list = match host.call("program.list", json!({})) {
        Ok(v) => v,
        Err(e) if unknown_method(&e) => return Ok(Some(NEWER_CHROMAFORGE.into())),
        Err(e) => return Err(e),
    };
    let entry = list.as_array().and_then(|l| l.iter().find(|p| p.get("program").and_then(Value::as_str) == Some(PROGRAM)));
    let Some(entry) = entry else {
        return Err(format!("the host doesn't list the program {PROGRAM:?} this plug-in declares"));
    };
    if entry.get("installed").and_then(Value::as_array).is_none_or(|i| i.is_empty()) {
        return Ok(Some(NOT_INSTALLED.into()));
    }
    if entry.get("granted").and_then(Value::as_bool) != Some(true) {
        return Ok(Some(format!(
            "Allow {PROGRAM_NAME} for this plug-in first: File ▸ Plug-ins ▸ Plug-in Manager…, then tick {PROGRAM_NAME} on this plug-in's row."
        )));
    }
    // Without libraryAdd, PureRAW's DNGs couldn't come back: say so before the user spends time there.
    // The host checks the permission before the params, so a call without params asks only that and
    // adds nothing (Chromaforge's docs don't promise that order yet).
    match host.call("library.add", Value::Null) {
        Err(e) if e.contains("permission `libraryAdd` not granted") => Ok(Some(format!(
            "Allow \"Add files made from photos to the library\" for this plug-in first: File ▸ Plug-ins ▸ Plug-in Manager…. \
             It adds the DNGs {PROGRAM_NAME} makes, stacked on their originals."
        ))),
        Err(e) if unknown_method(&e) => Ok(Some(NEWER_CHROMAFORGE.into())),
        _ => Ok(None),
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

/// What to tell the user when `program.start` refuses.
fn start_failed(e: &str) -> String {
    if e.contains("already running") {
        format!(
            "{PROGRAM_NAME} is still open from the last send. Process or close the photos there first, then send these. \
             (✕ on its row in the activity list stops waiting for it.)"
        )
    } else if e.contains("permission `programs` not granted") {
        format!("Allow {PROGRAM_NAME} for this plug-in first: File ▸ Plug-ins ▸ Plug-in Manager…, then tick {PROGRAM_NAME} on this plug-in's row.")
    } else if e.contains("is not installed") {
        NOT_INSTALLED.into()
    } else if unknown_method(e) {
        NEWER_CHROMAFORGE.into()
    } else {
        format!("{PROGRAM_NAME} could not be started: {e}")
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
