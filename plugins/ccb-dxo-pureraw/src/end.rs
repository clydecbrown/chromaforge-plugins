//! The `program.end` hook: PureRAW has exited (or the user stopped waiting). Reads its results list
//! from the run's folder and adds each output to the library, stacked on the photo it came from, then
//! logs one summary.

use std::collections::BTreeSet;

use handoff::{RunRecord, file_name};
use lcc_plugin_sdk::Request;
use lcc_plugin_sdk::serde_json::{Value, json};

use crate::results::parse;
use crate::{BATCH, Host, MAX_PHOTOS, NEWER_CHROMAFORGE, PROGRAM_NAME, RESULTS, RUN_RECORD, unknown_method};

/// How many lines of detail (skipped results, failed adds) are logged before one "and N more": each
/// line is a host call, and a call has a budget.
const MAX_DETAIL: usize = 50;
/// How many files a summary names before "and N more".
const MAX_NAMED: usize = 10;
/// The host cuts a log line off at 2048 bytes: lines are kept below that, cut at a character.
const MAX_LINE: usize = 2000;
/// The most results one run reads: an add per photo sent, and as many to spare.
const MAX_RESULTS: usize = MAX_PHOTOS * 2;

/// What a run brought in, for the hook's reply and its summary.
#[derive(Debug, Default)]
struct Tally {
    added: usize,
    updated: usize,
    unchanged: usize,
    not_processed: Vec<String>,
    outside: Vec<String>,
    failed: Vec<String>,
    details: usize,
}

impl Tally {
    /// Logs a line of detail, built only if it will be logged: a hostile list's lines can be long.
    fn detail(&mut self, host: &mut impl Host, line: impl FnOnce() -> String) {
        self.details = self.details.saturating_add(1);
        if self.details <= MAX_DETAIL {
            host.log(fit(&line()));
        }
    }

    fn reply(&self) -> Value {
        json!({"added": self.added, "updated": self.updated, "unchanged": self.unchanged,
            "notProcessed": self.not_processed, "outsideFolder": self.outside, "failed": self.failed})
    }
}

/// The hook. It never fails over what PureRAW or the run's folder hold: each problem is a log line.
pub fn end(host: &mut impl Host, req: &Request) -> Result<Value, String> {
    let tmp = req.tmp_dir().trim_end_matches('/').to_string();
    let mut tally = Tally::default();
    match req.status() {
        "exited" => {}
        "cancelled" => {
            host.log(&format!(
                "Stopped waiting for {PROGRAM_NAME} (✕). It keeps running, but the DNGs it makes from this send won't be added to the library: \
                 import them from their DxO folders, or send the photos again once it's closed."
            ));
            return Ok(tally.reply());
        }
        other => {
            host.log(&format!("Waiting for {PROGRAM_NAME} ended without an answer ({other}). Nothing was added."));
            return Ok(tally.reply());
        }
    }
    if tmp.is_empty() {
        host.log("Nothing was added: the host didn't say where this run's files are.");
        return Ok(tally.reply());
    }
    let record = match host.call("fs.readText", json!({"path": format!("{tmp}/{RUN_RECORD}")})) {
        Ok(text) => RunRecord::from_json(text.as_str().unwrap_or_default(), MAX_PHOTOS),
        Err(e) => Err(format!("the run record couldn't be read ({e})")),
    };
    let record = match record {
        Ok(r) => r,
        Err(e) => {
            host.log(fit(&format!("Nothing was added: {e}, so the DNGs can't be matched to their photos.")));
            return Ok(tally.reply());
        }
    };
    let results_path = format!("{tmp}/{RESULTS}");
    let text = match host.call("fs.readText", json!({"path": results_path})) {
        Ok(text) => text.as_str().unwrap_or_default().to_string(),
        Err(e) => {
            let line = if exists(host, &results_path) {
                format!("Nothing was added: {PROGRAM_NAME}'s list of results couldn't be read ({e}).")
            } else if exists(host, &format!("{tmp}/{BATCH}")) {
                // PureRAW reads and deletes its batch, even one it refuses: this one it never read
                format!(
                    "{PROGRAM_NAME} closed without taking the photos it was sent, so nothing was added. It was probably already open from \
                     another app or an earlier send (Lightroom Classic's DxO plug-in, or a send you stopped waiting for), and then it drops \
                     a new batch. Close that {PROGRAM_NAME} window, then send the photos again."
                )
            } else {
                format!("{PROGRAM_NAME} processed nothing (closed without processing, or it refused the photos), so nothing was added.")
            };
            host.log(fit(&line));
            tally.not_processed = record.sent.iter().map(|s| s.path.clone()).collect();
            return Ok(tally.reply());
        }
    };

    let results = parse(&text, MAX_RESULTS);
    for p in &results.problems {
        tally.detail(host, || format!("Skipped in {PROGRAM_NAME}'s results: {p}"));
    }
    if results.more > 0 {
        host.log(&format!("Skipped in {PROGRAM_NAME}'s results: {} more lines, past what one send can bring in.", results.more));
    }
    let photos = record.index();
    // every output of a photo sent is added, however many it has; in all, at most two per photo sent,
    // so a broken or hostile list can't run the call out of host calls
    let max_adds = record.sent.len().saturating_mul(2);
    let mut processed: BTreeSet<&str> = BTreeSet::new();
    let mut adds = 0usize;
    // a call cut off by its time limit logs no summary: this line says it got this far
    let tries = results.pairs.iter().filter(|p| photos.contains_key(p.original)).count().min(max_adds);
    host.log(&format!("Adding {}…", count(tries, "DNG", "DNGs")));
    for pair in &results.pairs {
        let Some(photo) = photos.get(pair.original).copied() else {
            tally.detail(host, || format!("Skipped {}: it was made from {}, which this send didn't include.", pair.output, pair.original));
            continue;
        };
        if adds >= max_adds {
            tally.detail(host, || "Skipped the rest of the results: they list more files than were sent.".into());
            break;
        }
        adds = adds.saturating_add(1);
        processed.insert(pair.original);
        match host.call("library.add", json!({"path": pair.output, "from": photo, "stack": true})) {
            Ok(v) => {
                if v.get("added").and_then(Value::as_bool) == Some(true) {
                    tally.added = tally.added.saturating_add(1);
                } else if v.get("reloaded").and_then(Value::as_bool) == Some(true) {
                    tally.updated = tally.updated.saturating_add(1);
                } else {
                    tally.unchanged = tally.unchanged.saturating_add(1);
                }
            }
            Err(e) if e.contains("is not in the folder") => tally.outside.push(pair.output.to_string()),
            Err(e) if unknown_method(&e) => {
                host.log(&format!("Nothing was added. {NEWER_CHROMAFORGE}"));
                return Ok(tally.reply());
            }
            Err(e) if e.contains("permission `libraryAdd` not granted") => {
                host.log(&format!(
                    "The DNGs from {PROGRAM_NAME} weren't added: allow \"Add files made from photos to the library\" for this plug-in in \
                     File ▸ Plug-ins ▸ Plug-in Manager…, then import them from their DxO folders, or send the photos again."
                ));
                return Ok(tally.reply());
            }
            Err(e) => {
                tally.detail(host, || format!("{} was not added: {e}", pair.output));
                tally.failed.push(pair.output.to_string());
            }
        }
    }
    tally.not_processed = record.sent.iter().filter(|s| !processed.contains(s.path.as_str())).map(|s| s.path.clone()).collect();
    if tally.details > MAX_DETAIL {
        host.log(&format!("… and {} more lines like these.", tally.details - MAX_DETAIL));
    }
    if !tally.outside.is_empty() {
        host.log(&format!(
            "{PROGRAM_NAME} saved some DNGs outside their originals' folders, where Chromaforge doesn't add them from. Set its output folder \
             back to the default (a \"DxO\" folder next to the originals) in its Preferences, then send those photos again."
        ));
    }
    host.log(fit(&summary(&tally)));
    Ok(tally.reply())
}

/// Whether `path` (in the run's folder, lent to the call) exists; `false` when the host can't say.
fn exists(host: &mut impl Host, path: &str) -> bool {
    host.call("fs.exists", json!({"path": path})).ok().and_then(|v| v.as_bool()).unwrap_or(false)
}

/// `line`, cut at a character to fit the host's log line.
fn fit(line: &str) -> &str {
    if line.len() <= MAX_LINE {
        return line;
    }
    let mut end = MAX_LINE;
    while !line.is_char_boundary(end) {
        end = end.saturating_sub(1);
    }
    line.get(..end).unwrap_or_default()
}

fn count(n: usize, one: &str, many: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {many}") }
}

fn names(paths: &[String]) -> String {
    let shown: Vec<&str> = paths.iter().take(MAX_NAMED).map(|p| file_name(p)).collect();
    let more = paths.len().saturating_sub(shown.len());
    let more = if more > 0 { format!(" and {more} more") } else { String::new() };
    format!("{}{more}", shown.join(", "))
}

/// One line: what was added, then what wasn't (what to do about it is on lines of its own).
fn summary(t: &Tally) -> String {
    let mut parts = vec![format!("{PROGRAM_NAME}: added {}, stacked on their originals", count(t.added, "DNG", "DNGs"))];
    if t.updated > 0 {
        parts.push(format!("{} made again and read again", count(t.updated, "DNG was", "DNGs were")));
    }
    if t.unchanged > 0 {
        parts.push(format!("{} already in the library", count(t.unchanged, "DNG was", "DNGs were")));
    }
    if !t.not_processed.is_empty() {
        parts.push(format!("not processed (failed or cancelled in {PROGRAM_NAME}): {}", names(&t.not_processed)));
    }
    if !t.outside.is_empty() {
        parts.push(format!("not added because {PROGRAM_NAME} saved them outside their originals' folders (see above): {}", names(&t.outside)));
    }
    if !t.failed.is_empty() {
        parts.push(format!("not added (see the lines above): {}", names(&t.failed)));
    }
    format!("{}.", parts.join("; "))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Logs(usize);

    impl Host for Logs {
        fn call(&mut self, _: &str, _: Value) -> Result<Value, String> {
            Err("no host".into())
        }
        fn log(&mut self, _: &str) {
            self.0 += 1;
        }
    }

    #[test]
    fn detail_lines_past_the_cap_are_counted_not_built() {
        let (mut host, mut tally, mut built) = (Logs(0), Tally::default(), 0);
        for _ in 0..(MAX_DETAIL + 10) {
            tally.detail(&mut host, || {
                built += 1;
                "a line".into()
            });
        }
        assert_eq!((host.0, built, tally.details), (MAX_DETAIL, MAX_DETAIL, MAX_DETAIL + 10));
    }
}
