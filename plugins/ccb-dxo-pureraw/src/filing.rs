//! Where the DNGs go and which PureRAW runs: the plug-in's settings (Plug-in Manager ▸ Settings…) and
//! the album the command ran from. Send Selected Photos records both in `run.json`, and `program.end`
//! files the DNGs as they were then ([`Filing::of_record`]), whatever the user changed or wherever they
//! went in between.

use handoff::RunRecord;
use lcc_plugin_sdk::serde_json::{Map, Value, json};

use crate::Host;

/// The setting: add each DNG to the album the photos were sent from.
pub const ADD_TO_ALBUM: &str = "addToAlbum";
/// The setting: stack each DNG on its original.
pub const STACK: &str = "stack";
/// The setting: the PureRAW version to run, [`AUTOMATIC`] or a [`crate::versions::label`].
pub const VERSION_SETTING: &str = "version";
/// The version setting's first choice: the newest version installed.
pub const AUTOMATIC: &str = "Automatic";
/// The most characters of an album's name a message shows: the name comes from a file.
const MAX_NAME: usize = 100;

/// The plug-in's settings, as `settings.get` gives them.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Settings {
    pub add_to_album: bool,
    pub stack: bool,
    /// [`AUTOMATIC`], a version's label or name, or anything the host kept (it can't check a choice
    /// whose options only the plug-in gives).
    pub version: String,
}

impl Default for Settings {
    fn default() -> Self {
        Settings { add_to_album: true, stack: false, version: AUTOMATIC.into() }
    }
}

impl Settings {
    /// The settings now; the defaults from an app without settings. Otherwise a host that can't give
    /// them is a message for the user: guessing could run the wrong version or file the DNGs elsewhere.
    pub fn read(host: &mut impl Host) -> Result<Settings, String> {
        match host.call("settings.get", json!({})) {
            Ok(Value::Object(m)) => Ok(Settings::from_map(&m)),
            Err(e) if crate::unknown_method(&e) => Ok(Settings::default()),
            Err(e) => Err(format!("This plug-in's settings couldn't be read ({e}), so nothing was sent. Try again.")),
            Ok(other) => Err(format!(
                "This plug-in's settings couldn't be read (the app gave {}), so nothing was sent. Try again.",
                crate::filing::shown(&other.to_string())
            )),
        }
    }

    /// Each value that has the right type; the default for the rest. A version never chosen reads
    /// as `""` (the field declares no options): that's [`AUTOMATIC`].
    pub fn from_map(m: &Map<String, Value>) -> Settings {
        let d = Settings::default();
        let version = m.get(VERSION_SETTING).and_then(Value::as_str).filter(|v| !v.is_empty()).map_or(d.version, str::to_string);
        Settings {
            add_to_album: m.get(ADD_TO_ALBUM).and_then(Value::as_bool).unwrap_or(d.add_to_album),
            stack: m.get(STACK).and_then(Value::as_bool).unwrap_or(d.stack),
            version,
        }
    }

    pub fn to_map(&self) -> Map<String, Value> {
        let mut m = Map::new();
        m.insert(ADD_TO_ALBUM.into(), Value::Bool(self.add_to_album));
        m.insert(STACK.into(), Value::Bool(self.stack));
        m.insert(VERSION_SETTING.into(), Value::from(self.version.clone()));
        m
    }

    pub fn is_automatic(&self) -> bool {
        self.version == AUTOMATIC
    }
}

/// The album the DNGs go into.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Album {
    /// The setting is off.
    Off,
    /// The album the photos were sent from.
    Into { id: String, name: String },
    /// The setting is on but the photos weren't sent from an album: why, as a sentence's end
    /// ("you sent the photos from a folder, not an album").
    Not(String),
}

/// Where a run's DNGs go.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Filing {
    pub stack: bool,
    pub album: Album,
}

impl Filing {
    /// Where the DNGs of photos sent from `source` (the host's `source`) go, with `settings`.
    pub fn new(settings: &Settings, source: &Value) -> Filing {
        let album = if settings.add_to_album { album_of(source) } else { Album::Off };
        Filing { stack: settings.stack, album }
    }

    /// Where the DNGs of a recorded run go: as recorded when it was sent. A run sent before the
    /// settings existed stacks them, as the plug-in did then.
    pub fn of_record(record: &RunRecord) -> Filing {
        match &record.settings {
            Some(m) => Filing::new(&Settings::from_map(m), &record.source),
            None => Filing { stack: true, album: Album::Off },
        }
    }

    /// The album's id, for `library.add`.
    pub fn album_id(&self) -> Option<&str> {
        match &self.album {
            Album::Into { id, .. } => Some(id),
            _ => None,
        }
    }
}

/// The album `source` names, or why it names none.
fn album_of(source: &Value) -> Album {
    let kind = source.get("kind").and_then(Value::as_str);
    let name = source.get("name").and_then(Value::as_str).map(shown);
    let why = match kind {
        Some("album") => {
            // the host gives ids as strings; a number is read too
            let id = source.get("id").and_then(|i| i.as_str().map(str::to_string).or_else(|| i.as_u64().map(|n| n.to_string())));
            match id.filter(|i| !i.is_empty()) {
                Some(id) => return Album::Into { id, name: name.unwrap_or_else(|| "?".into()) },
                None => "Chromaforge didn't say which album you sent the photos from".to_string(),
            }
        }
        Some("smartAlbum") => match name {
            Some(n) => format!("you sent the photos from the smart album \"{n}\", which picks its photos by its rules"),
            None => "you sent the photos from a smart album, which picks its photos by its rules".into(),
        },
        Some("folder") => "you sent the photos from a folder, not an album".into(),
        Some("allPhotos") => "you sent the photos from All Photographs, not an album".into(),
        Some("other") if source.get("name").and_then(Value::as_str) == Some("albumFolder") => {
            "you sent the photos from an album folder, which holds albums, not photos".into()
        }
        Some(_) => "you sent the photos from a view that isn't an album".into(),
        None => "Chromaforge didn't say where you sent the photos from".into(),
    };
    Album::Not(why)
}

/// `name` as a message shows it: no control characters, at most [`MAX_NAME`] characters.
pub fn shown(name: &str) -> String {
    let mut out: String = name.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect();
    if name.chars().filter(|c| !c.is_control()).nth(MAX_NAME).is_some() {
        out.push('…');
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn on() -> Settings {
        Settings { add_to_album: true, stack: false, version: AUTOMATIC.into() }
    }

    #[test]
    fn an_album_that_holds_photos_is_used_and_every_other_source_says_why_not() {
        let album = Filing::new(&on(), &json!({"kind": "album", "id": "4", "name": "Trip"}));
        assert_eq!(album.album, Album::Into { id: "4".into(), name: "Trip".into() });
        let quick = Filing::new(&on(), &json!({"kind": "album", "id": "1", "name": "Quick Collection", "quick": true}));
        assert_eq!(quick.album_id(), Some("1"), "the Quick Collection counts");
        let why = |source: Value| match Filing::new(&on(), &source).album {
            Album::Not(why) => why,
            other => panic!("{source}: {other:?}"),
        };
        assert_eq!(why(json!({"kind": "folder", "path": "/p"})), "you sent the photos from a folder, not an album");
        assert_eq!(why(json!({"kind": "allPhotos"})), "you sent the photos from All Photographs, not an album");
        assert_eq!(
            why(json!({"kind": "smartAlbum", "id": "9", "name": "5 stars"})),
            "you sent the photos from the smart album \"5 stars\", which picks its photos by its rules"
        );
        assert!(why(json!({"kind": "other", "name": "albumFolder"})).contains("album folder"));
        assert_eq!(why(json!({"kind": "other", "name": "picks"})), "you sent the photos from a view that isn't an album");
        assert!(why(Value::Null).contains("didn't say"), "an older app sends no source");
        assert!(why(json!({"kind": "album", "name": "x"})).contains("didn't say which album"));
        assert!(why(json!({"kind": "album", "id": ""})).contains("didn't say which album"));
        assert_eq!(Filing::new(&on(), &json!({"kind": "album", "id": 4})).album_id(), Some("4"));
    }

    #[test]
    fn with_the_setting_off_no_album_is_used_and_stack_follows_its_setting() {
        let s = Settings { add_to_album: false, stack: true, version: AUTOMATIC.into() };
        let f = Filing::new(&s, &json!({"kind": "album", "id": "4", "name": "Trip"}));
        assert_eq!(f, Filing { stack: true, album: Album::Off });
    }

    #[test]
    fn settings_read_what_fits_and_default_the_rest() {
        assert_eq!(Settings::from_map(&Map::new()), Settings::default());
        assert_eq!(Settings::default(), Settings { add_to_album: true, stack: false, version: "Automatic".into() });
        let m = json!({"addToAlbum": "yes", "stack": true, "version": ""});
        let s = Settings::from_map(m.as_object().unwrap());
        assert_eq!(s, Settings { add_to_album: true, stack: true, version: AUTOMATIC.into() }, "a version never chosen is Automatic");
        let back = Settings::from_map(&Settings { add_to_album: false, stack: true, version: "DxO PureRAW 2".into() }.to_map());
        assert_eq!(back, Settings { add_to_album: false, stack: true, version: "DxO PureRAW 2".into() });
    }

    #[test]
    fn a_run_recorded_before_the_settings_stacks_as_it_did_then() {
        let old = RunRecord::default();
        assert_eq!(Filing::of_record(&old), Filing { stack: true, album: Album::Off });
        let new = RunRecord { source: json!({"kind": "album", "id": "4", "name": "Trip"}), settings: Some(on().to_map()), ..RunRecord::default() };
        assert_eq!(Filing::of_record(&new).album_id(), Some("4"));
    }

    #[test]
    fn names_from_a_file_are_shown_short_and_without_control_characters() {
        assert_eq!(shown("Trip\n2026\u{7}"), "Trip2026");
        let long = "é".repeat(MAX_NAME + 5);
        assert_eq!(shown(&long).chars().count(), MAX_NAME + 1);
        assert!(shown(&long).ends_with('…'));
        assert_eq!(shown(&"a".repeat(MAX_NAME)), "a".repeat(MAX_NAME));
    }
}
