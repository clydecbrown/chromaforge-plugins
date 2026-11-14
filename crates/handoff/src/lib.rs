//! Shared logic for plug-ins that send files to another program and take the results back.
//!
//! Never released alone: each plug-in that uses it ships it inside its own `.wasm`. It is versioned
//! with the repository (`0.0.0`, `publish = false`); a change here goes out in the next release of
//! each plug-in that wants it.
//!
//! - [`Batch`]: the files to send, one absolute path per line, each path once, with the photo it
//!   came from. Paths a line-based list can't carry are refused, not mangled.
//! - [`RunRecord`]: what was sent, from where and with which settings, kept in the run's folder
//!   (`run.json`) so the plug-in's `program.end` call, a fresh instance, can tell which photo each
//!   result belongs to and where to put it, whatever the user did in between.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]

use std::collections::{BTreeMap, BTreeSet};

use serde_json::{Map, Value, json};

/// The format of [`RunRecord`]'s JSON. A record in any other format is refused.
pub const RUN_RECORD_FORMAT: u64 = 1;

/// One file sent, and the photo it came from.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Sent {
    /// The photo's id in the library, as the host gives it.
    pub photo: String,
    /// The file's absolute path, exactly as written to the batch.
    pub path: String,
}

/// Why a path can't go in a batch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refused {
    /// Empty, or not starting with `/`: the program would read it relative to a folder of its own.
    NotAbsolute,
    /// It holds a line feed or carriage return, which would split it into two lines.
    LineBreak,
    /// It holds a NUL, which no file path can.
    Nul,
}

impl Refused {
    /// What to tell the user, after the file's name.
    pub fn reason(self) -> &'static str {
        match self {
            Refused::NotAbsolute => "its path isn't absolute",
            Refused::LineBreak => "its path has a line break in it",
            Refused::Nul => "its path has a NUL character in it",
        }
    }
}

/// Why a path can't be sent, or `None` if it can.
pub fn refused(path: &str) -> Option<Refused> {
    if path.contains(['\n', '\r']) {
        Some(Refused::LineBreak)
    } else if path.contains('\0') {
        Some(Refused::Nul)
    } else if !path.starts_with('/') {
        Some(Refused::NotAbsolute)
    } else {
        None
    }
}

/// The files to send, in the order added, each path once.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Batch {
    sent: Vec<Sent>,
    paths: BTreeSet<String>,
}

impl Batch {
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds `path`, made from `photo`. A path already in the batch is kept once, with the first
    /// photo that sent it (`Ok(false)`); a path that can't be written as one line is refused.
    pub fn add(&mut self, photo: &str, path: &str) -> Result<bool, Refused> {
        if let Some(why) = refused(path) {
            return Err(why);
        }
        if !self.paths.insert(path.to_string()) {
            return Ok(false);
        }
        self.sent.push(Sent { photo: photo.to_string(), path: path.to_string() });
        Ok(true)
    }

    pub fn is_empty(&self) -> bool {
        self.sent.is_empty()
    }

    pub fn len(&self) -> usize {
        self.sent.len()
    }

    pub fn sent(&self) -> &[Sent] {
        &self.sent
    }

    /// The batch file: one path per line, each ending in `\n`.
    pub fn text(&self) -> String {
        let mut out = String::with_capacity(self.sent.iter().map(|s| s.path.len().saturating_add(1)).sum());
        for s in &self.sent {
            out.push_str(&s.path);
            out.push('\n');
        }
        out
    }

    /// The record to keep next to the batch: what was sent, without a source or settings.
    pub fn record(&self) -> RunRecord {
        RunRecord { sent: self.sent.clone(), ..RunRecord::default() }
    }
}

/// What a run sent: kept in its folder as JSON and read back when the program ends.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RunRecord {
    pub sent: Vec<Sent>,
    /// The Library source the command ran from (`source` in the host's request), as the host gave it:
    /// read back from a file, so as untrusted as any input. `Null` when not recorded.
    pub source: Value,
    /// The plug-in's settings when the files were sent. `None` in a record written before plug-ins
    /// kept them: the plug-in then does what it did then.
    pub settings: Option<Map<String, Value>>,
}

impl RunRecord {
    pub fn to_json(&self) -> String {
        let sent: Vec<Value> = self.sent.iter().map(|s| json!({"photo": s.photo, "path": s.path})).collect();
        let mut v = json!({"format": RUN_RECORD_FORMAT, "sent": sent});
        if !self.source.is_null() {
            v["source"] = self.source.clone();
        }
        if let Some(settings) = &self.settings {
            v["settings"] = Value::Object(settings.clone());
        }
        v.to_string()
    }

    /// Reads a record written by [`RunRecord::to_json`] of at most `max` files. Anything else, or a
    /// record with a path listed twice, is an error: the plug-in can't then tell which photo a result
    /// belongs to. `source` and `settings` may be missing (a record from before them); settings that
    /// aren't an object are an error, as the plug-in can't then tell what the user chose.
    pub fn from_json(text: &str, max: usize) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("the run record is damaged ({e})"))?;
        if v.get("format").and_then(Value::as_u64) != Some(RUN_RECORD_FORMAT) {
            return Err(format!("the run record is in an unknown format ({})", v.get("format").unwrap_or(&Value::Null)));
        }
        let list = v.get("sent").and_then(Value::as_array).ok_or("the run record is damaged (no list of sent files)")?;
        if list.len() > max {
            return Err(format!("the run record is damaged (it lists {} files, more than the {max} a send takes)", list.len()));
        }
        let mut paths = BTreeSet::new();
        let mut sent = Vec::with_capacity(list.len());
        for item in list {
            let photo = item.get("photo").and_then(Value::as_str);
            let path = item.get("path").and_then(Value::as_str);
            let (Some(photo), Some(path)) = (photo, path) else {
                return Err("the run record is damaged (an entry without photo or path)".into());
            };
            if !paths.insert(path) {
                return Err("the run record is damaged (a path listed twice)".into());
            }
            sent.push(Sent { photo: photo.to_string(), path: path.to_string() });
        }
        let source = v.get("source").cloned().unwrap_or(Value::Null);
        let settings = match v.get("settings") {
            None => None,
            Some(Value::Object(m)) => Some(m.clone()),
            Some(_) => return Err("the run record is damaged (its settings aren't a list of values)".into()),
        };
        Ok(Self { sent, source, settings })
    }

    /// The photo that sent `path`. Each call scans the record: for many lookups, use [`RunRecord::index`].
    pub fn photo_of(&self, path: &str) -> Option<&str> {
        self.sent.iter().find(|s| s.path == path).map(|s| s.photo.as_str())
    }

    /// Path → the photo that sent it.
    pub fn index(&self) -> BTreeMap<&str, &str> {
        self.sent.iter().map(|s| (s.path.as_str(), s.photo.as_str())).collect()
    }
}

/// The last component of `path`, for messages (the whole path if it has none).
pub fn file_name(path: &str) -> &str {
    path.rsplit('/').find(|p| !p.is_empty()).unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_batch_has_one_absolute_path_per_line_each_ending_in_a_line_feed() {
        let mut b = Batch::new();
        assert_eq!(b.add("1", "/p/b.CR3"), Ok(true));
        assert_eq!(b.add("2", "/p/a b/é.dng"), Ok(true));
        assert_eq!(b.text(), "/p/b.CR3\n/p/a b/é.dng\n");
        assert_eq!(b.len(), 2);
    }

    #[test]
    fn a_path_is_sent_once_with_the_first_photo_that_sent_it() {
        let mut b = Batch::new();
        assert_eq!(b.add("1", "/p/a.dng"), Ok(true));
        assert_eq!(b.add("2", "/p/a.dng"), Ok(false), "a virtual copy shares its original's file");
        assert_eq!(b.text(), "/p/a.dng\n");
        assert_eq!(b.record().photo_of("/p/a.dng"), Some("1"));
    }

    #[test]
    fn paths_a_line_can_t_carry_are_refused() {
        let mut b = Batch::new();
        assert_eq!(b.add("1", "/p/a\nb.dng"), Err(Refused::LineBreak));
        assert_eq!(b.add("1", "/p/a\rb.dng"), Err(Refused::LineBreak));
        assert_eq!(b.add("1", "/p/a.dng\r"), Err(Refused::LineBreak));
        assert_eq!(b.add("1", "/p/a\0.dng"), Err(Refused::Nul));
        assert_eq!(b.add("1", "a.dng"), Err(Refused::NotAbsolute));
        assert_eq!(b.add("1", ""), Err(Refused::NotAbsolute));
        assert!(b.is_empty());
        assert_eq!(b.text(), "");
    }

    #[test]
    fn the_run_record_reads_back_what_was_written() {
        let mut b = Batch::new();
        b.add("7", "/p/\"q\" \\ é.dng").unwrap();
        b.add("9", "/p/x.CR3").unwrap();
        let back = RunRecord::from_json(&b.record().to_json(), 10).unwrap();
        assert_eq!(back, b.record());
        assert_eq!(back.photo_of("/p/x.CR3"), Some("9"));
        assert_eq!(back.photo_of("/p/y.CR3"), None);
    }

    #[test]
    fn the_source_and_settings_read_back_and_may_be_missing() {
        let mut b = Batch::new();
        b.add("7", "/p/a.CR3").unwrap();
        let mut record = b.record();
        let settings = json!({"addToAlbum": true, "stack": false});
        record.source = json!({"kind": "album", "id": "4", "name": "Trip"});
        record.settings = settings.as_object().cloned();
        let back = RunRecord::from_json(&record.to_json(), 10).unwrap();
        assert_eq!(back, record);
        assert_eq!(back.settings.map(Value::Object), Some(settings));
        let old = RunRecord::from_json(r#"{"format": 1, "sent": [{"photo": "7", "path": "/p/a.CR3"}]}"#, 10).unwrap();
        assert_eq!((old.source, old.settings), (Value::Null, None), "a record from before them");
        assert_eq!(old.sent, record.sent);
        assert!(!b.record().to_json().contains("settings"), "none recorded, none written");
    }

    #[test]
    fn a_damaged_run_record_is_an_error_not_a_guess() {
        for text in [
            "",
            "{",
            "[]",
            r#"{"sent": []}"#,
            r#"{"format": 2, "sent": []}"#,
            r#"{"format": 1}"#,
            r#"{"format": 1, "sent": [{"photo": "1"}]}"#,
            r#"{"format": 1, "sent": [{"photo": 1, "path": "/a"}]}"#,
            r#"{"format": 1, "sent": [{"photo": "1", "path": "/a"}, {"photo": "2", "path": "/a"}]}"#,
            r#"{"format": 1, "sent": [], "settings": [true]}"#,
            r#"{"format": 1, "sent": [], "settings": null}"#,
        ] {
            assert!(RunRecord::from_json(text, 10).is_err(), "{text:?} should be refused");
        }
        let two = r#"{"format": 1, "sent": [{"photo": "1", "path": "/a"}, {"photo": "2", "path": "/b"}]}"#;
        assert!(RunRecord::from_json(two, 2).is_ok());
        assert!(RunRecord::from_json(two, 1).is_err(), "more files than a send takes");
    }

    #[test]
    fn file_name_is_the_last_component() {
        assert_eq!(file_name("/p/a b/c.dng"), "c.dng");
        assert_eq!(file_name("/p/dir/"), "dir");
        assert_eq!(file_name("c.dng"), "c.dng");
        assert_eq!(file_name(""), "");
    }
}
