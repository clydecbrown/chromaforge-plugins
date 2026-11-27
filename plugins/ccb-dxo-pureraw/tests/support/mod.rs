//! A stand-in for Chromaforge's side of the plug-in calls, close to what the host at the pinned SDK
//! does: `program.list` and `program.start` (a real run folder, `{tmpDir}` expanded), the run's
//! folder lent for reading, `library.add`'s folder and album rules and its re-read of a changed file,
//! the plug-in's settings, the Library source the session shows, and the host's error texts the
//! plug-in reacts to.

#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use ccb_dxo_pureraw::Host;
use ccb_dxo_pureraw::versions::{bundle_id, version_of};
use lcc_plugin_sdk::Request;
use lcc_plugin_sdk::serde_json::{Map, Value, json};

/// The host's cap on a file read (`fs.readText`).
pub const MAX_IO_BYTES: usize = 8 * 1024 * 1024;

/// A photo of the fake library.
#[derive(Clone, Debug)]
pub struct LibPhoto {
    pub id: String,
    pub path: Option<String>,
    pub kind: &'static str,
    pub deleted: bool,
    /// Seen while browsing a folder, not in the library.
    pub local: bool,
    /// A virtual copy: the photo it was copied from (it shares that photo's file).
    pub copy_of: Option<String>,
    /// The photo it's stacked on (added by `library.add` with `stack`).
    pub stacked_on: Option<String>,
    /// The file's contents when the library last read it.
    pub read: Vec<u8>,
}

/// An album of the fake library.
#[derive(Clone, Debug)]
pub struct FakeAlbum {
    pub id: String,
    pub name: String,
    pub photos: Vec<String>,
    /// A smart album: it holds rules, not photos.
    pub smart: bool,
}

/// A started run.
#[derive(Clone, Debug)]
pub struct Started {
    pub run: String,
    pub args: Vec<String>,
    pub files: Map<String, Value>,
    pub tmp: PathBuf,
    /// The version started (`bundleId`, or the first installed, as the host picks).
    pub version: u32,
}

pub struct FakeHost {
    /// A scratch folder for this test: photos and run folders.
    pub root: PathBuf,
    pub photos: Vec<LibPhoto>,
    /// The versions installed, in any order (the host lists them in the manifest's).
    pub installed: Vec<u32>,
    pub granted: bool,
    pub catalog: bool,
    /// `libraryAdd` granted.
    pub library_add: bool,
    /// A run of the program is still open (the host allows one at a time).
    pub busy: bool,
    /// A Chromaforge without programs and `library.add`.
    pub old: bool,
    /// Answering a prepare step: the host refuses writes.
    pub preparing: bool,
    pub started: Vec<Started>,
    /// The `bundleId` of each `program.start` that started, as the plug-in gave it.
    pub started_with: Vec<String>,
    /// The app is removed after `program.list` answered, before `program.start`.
    pub gone_at_start: bool,
    /// The folder lent to the current `program.end` call.
    pub lent: Option<PathBuf>,
    pub adds: Vec<Value>,
    pub log: Vec<String>,
    pub calls: usize,
    pub albums: Vec<FakeAlbum>,
    /// The settings as stored: the host fills in the defaults for keys never set.
    pub settings: Map<String, Value>,
    /// The Library source the session shows: the host sends it with each command.
    pub source: Value,
    /// `settings.get` fails.
    pub settings_broken: bool,
}

/// Send Selected Photos as the host runs it: with the session's source.
pub fn send(h: &mut FakeHost, selection: &[String]) -> Result<Value, String> {
    let source = h.source.clone();
    ccb_dxo_pureraw::send(h, selection, &source)
}

impl FakeHost {
    pub fn new(test: &str) -> Self {
        let root = std::env::temp_dir().join(format!("ccb-dxo-pureraw-{test}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();
        // the folder rule compares resolved paths, as the host does
        let root = fs::canonicalize(&root).unwrap();
        Self {
            root,
            photos: Vec::new(),
            installed: vec![3],
            granted: true,
            catalog: true,
            library_add: true,
            busy: false,
            old: false,
            preparing: false,
            started: Vec::new(),
            started_with: Vec::new(),
            gone_at_start: false,
            lent: None,
            adds: Vec::new(),
            log: Vec::new(),
            calls: 0,
            albums: Vec::new(),
            settings: Map::new(),
            source: Value::Null,
            settings_broken: false,
        }
        .browsing_its_folder()
    }

    fn browsing_its_folder(mut self) -> Self {
        self.source = json!({"kind": "folder", "path": self.root});
        self
    }

    /// Stores setting `key` as Settings… would.
    pub fn set(&mut self, key: &str, value: Value) {
        self.settings.insert(key.to_string(), value);
    }

    /// A regular album holding `photos`; its id.
    pub fn album(&mut self, name: &str, photos: &[&str]) -> String {
        let id = format!("{}", 100 + self.albums.len());
        self.albums.push(FakeAlbum { id: id.clone(), name: name.into(), photos: photos.iter().map(|p| p.to_string()).collect(), smart: false });
        id
    }

    /// The session shows album `id`, as the host's `source` says it.
    pub fn show_album(&mut self, id: &str) {
        let a = self.albums.iter().find(|a| a.id == id).unwrap();
        let kind = if a.smart { "smartAlbum" } else { "album" };
        self.source = json!({"kind": kind, "id": a.id, "name": a.name});
    }

    pub fn album_photos(&self, id: &str) -> Vec<String> {
        self.albums.iter().find(|a| a.id == id).map(|a| a.photos.clone()).unwrap_or_default()
    }

    /// Writes a raw file at `rel` (below the scratch folder) and adds it to the library; its id.
    pub fn raw(&mut self, rel: &str) -> String {
        self.file(rel, "raw")
    }

    pub fn file(&mut self, rel: &str, kind: &'static str) -> String {
        let path = self.root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, format!("raw data of {rel}")).unwrap();
        self.photo(Some(path.display().to_string()), kind)
    }

    /// Adds a photo without writing a file: `None` is a demo photo.
    pub fn photo(&mut self, path: Option<String>, kind: &'static str) -> String {
        let id = (self.photos.len() + 1).to_string();
        let read = path.as_deref().and_then(|p| fs::read(p).ok()).unwrap_or_default();
        self.photos.push(LibPhoto { id: id.clone(), path, kind, deleted: false, local: false, copy_of: None, stacked_on: None, read });
        id
    }

    pub fn by_id(&self, id: &str) -> &LibPhoto {
        self.photos.iter().find(|p| p.id == id).unwrap()
    }

    pub fn by_path(&self, path: &str) -> Option<&LibPhoto> {
        self.photos.iter().find(|p| p.path.as_deref() == Some(path))
    }

    /// A virtual copy of `master`; its id.
    pub fn virtual_copy(&mut self, master: &str) -> String {
        let mut copy = self.by_id(master).clone();
        copy.id = (self.photos.len() + 1).to_string();
        copy.copy_of = Some(master.to_string());
        let id = copy.id.clone();
        self.photos.push(copy);
        id
    }

    pub fn path_of(&self, id: &str) -> String {
        self.by_id(id).path.clone().unwrap()
    }

    /// The photos stacked on `id`.
    pub fn stacked_on(&self, id: &str) -> Vec<&LibPhoto> {
        self.photos.iter().filter(|p| p.stacked_on.as_deref() == Some(id)).collect()
    }

    pub fn last_run(&self) -> &Started {
        self.started.last().expect("a run was started")
    }

    /// The `program.end` request for the last run, its folder lent to the call.
    pub fn end_request(&mut self, status: &str) -> Request {
        let s = self.last_run().clone();
        self.lent = Some(s.tmp.clone());
        Request(json!({"hook": "program.end", "run": s.run, "program": "pureraw", "status": status, "code": null, "tmpDir": s.tmp}))
    }

    pub fn logged(&self, needle: &str) -> bool {
        self.log.iter().any(|l| l.contains(needle))
    }

    fn lent_path(&self, p: &Value) -> Result<PathBuf, String> {
        let path = PathBuf::from(p.get("path").and_then(Value::as_str).ok_or("missing `path`")?);
        match &self.lent {
            Some(dir) if path.starts_with(dir) => Ok(path),
            _ => Err(format!("{}: not below a readable root", path.display())),
        }
    }

    fn inspect(&self, id: &str) -> Result<Value, String> {
        let p = self.photos.iter().find(|p| p.id == id).ok_or("no such photo")?;
        let source = match &p.path {
            Some(path) => json!({"type": "file", "path": path}),
            None => json!({"type": "demo", "scene": 3}),
        };
        let name = p.path.as_deref().map_or("Demo", |x| x.rsplit('/').next().unwrap_or(x));
        let mut v = json!({"id": p.id.parse::<u64>().unwrap(), "source": source, "file_name": name, "kind": p.kind,
            "format": "dng", "deleted": p.deleted, "local": p.local});
        if let Some(m) = &p.copy_of {
            v["copy_of"] = json!(m.parse::<u64>().unwrap());
        }
        Ok(v)
    }

    fn start(&mut self, p: &Value) -> Result<Value, String> {
        if !self.granted {
            return Err("pureraw: permission `programs` not granted to this plug-in".into());
        }
        let version = match p.get("bundleId") {
            None => self.listed().first().copied(),
            Some(Value::String(b)) => {
                let v = version_of(b).ok_or(format!("bundleId {b:?} is not one of pureraw's"))?;
                self.installed.contains(&v).then_some(v)
            }
            Some(_) => return Err("`bundleId` must be a string".into()),
        };
        let version = version.filter(|_| !self.gone_at_start);
        let Some(version) = version else {
            let wanted = p.get("bundleId").and_then(Value::as_str).map(|b| format!(" ({b})")).unwrap_or_default();
            return Err(format!("DxO PureRAW is not installed{wanted}"));
        };
        if self.busy {
            return Err("DxO PureRAW is already running for this plug-in; wait for it to end".into());
        }
        let run = format!("run-{}", self.started.len() + 1);
        let tmp = self.root.join("runs").join(&run);
        fs::create_dir_all(&tmp).unwrap();
        let files = p.get("files").and_then(Value::as_object).cloned().unwrap_or_default();
        for (name, text) in &files {
            assert!(!name.contains('/') && name != "..", "a plain file name: {name}");
            fs::write(tmp.join(name), text.as_str().expect("file contents are text")).unwrap();
        }
        let args: Vec<String> = p["args"]
            .as_array()
            .expect("args are a list")
            .iter()
            .map(|a| a.as_str().expect("args are strings").replace("{tmpDir}", &tmp.display().to_string()))
            .collect();
        self.started_with.push(p.get("bundleId").and_then(Value::as_str).unwrap_or_default().to_string());
        self.started.push(Started { run: run.clone(), args, files, tmp: tmp.clone(), version });
        Ok(json!({"run": run, "tmpDir": tmp}))
    }

    /// The installed versions in the manifest's order: newest first.
    fn listed(&self) -> Vec<u32> {
        let mut v = self.installed.clone();
        v.sort_by(|a, b| b.cmp(a));
        v.dedup();
        v
    }

    fn library_add(&mut self, p: &Value) -> Result<Value, String> {
        let path = p.get("path").and_then(Value::as_str).ok_or("missing `path`")?.to_string();
        let from = p.get("from").and_then(Value::as_str).ok_or("missing `from`")?.to_string();
        let stack = p.get("stack").and_then(Value::as_bool).unwrap_or(false);
        let album = match p.get("album") {
            None | Some(Value::Null) => None,
            Some(Value::String(a)) => Some(a.clone()),
            Some(a) => return Err(format!("`album`: {a} is not an id")),
        };
        let origin = self.photos.iter().find(|x| x.id == from && !x.deleted && !x.local).ok_or(format!("photo {from} is not in the library"))?;
        let original = origin.path.clone().ok_or(format!("photo {from} has no file"))?;
        if !path.starts_with('/') {
            return Err(format!("{path}: the path must be absolute"));
        }
        let folder = Path::new(&original).parent().unwrap().to_path_buf();
        let target = fs::canonicalize(&path).map_err(|_| format!("{path} can't be found"))?;
        if target == Path::new(&original) {
            return Err(format!("{path} is photo {from}'s own original"));
        }
        if !target.starts_with(&folder) {
            return Err(format!("{path} is not in the folder of photo {from}'s original ({}) or below it", folder.display()));
        }
        // `album`: a regular album that holds `from`, checked before anything changes
        let album = match album {
            None => None,
            Some(a) => {
                let al = self.albums.iter().position(|x| x.id == a).ok_or(format!("no album {a}"))?;
                if self.albums[al].smart {
                    return Err(format!("album {:?} can't hold photos (a smart album or a folder)", self.albums[al].name));
                }
                if !self.albums[al].photos.contains(&from) {
                    return Err(format!("album {:?} doesn't hold photo {from}", self.albums[al].name));
                }
                Some(al)
            }
        };
        let bytes = fs::read(&target).map_err(|e| format!("{path} could not be added: {e}"))?;
        let file_in = |albums: &mut Vec<FakeAlbum>, id: &str| {
            if let Some(al) = album.filter(|al| !albums[*al].photos.iter().any(|x| x == id)) {
                albums[al].photos.push(id.to_string());
            }
        };
        if let Some(existing) = self.photos.iter_mut().find(|x| x.path.as_deref() == Some(path.as_str())) {
            let reloaded = existing.read != bytes;
            existing.read = bytes;
            if stack {
                existing.stacked_on = Some(from);
            }
            let id = existing.id.clone();
            file_in(&mut self.albums, &id);
            return Ok(json!({"photo": id, "added": false, "reloaded": reloaded}));
        }
        let id = (self.photos.len() + 1).to_string();
        file_in(&mut self.albums, &id);
        self.photos.push(LibPhoto {
            id: id.clone(),
            path: Some(path),
            kind: "raw",
            deleted: false,
            local: false,
            copy_of: None,
            stacked_on: stack.then_some(from),
            read: bytes,
        });
        Ok(json!({"photo": id, "added": true, "reloaded": false}))
    }
}

impl Host for FakeHost {
    fn call(&mut self, method: &str, params: Value) -> Result<Value, String> {
        self.calls += 1;
        if self.old && matches!(method, "program.list" | "program.start" | "library.add") {
            return Err(format!("unknown host method {method:?}"));
        }
        if self.preparing && matches!(method, "program.start" | "library.add" | "fs.writeText" | "ns.set" | "settings.set") {
            return Err(format!("{method}: a prepare step can only read"));
        }
        match method {
            "program.list" => {
                let installed: Vec<Value> = self
                    .listed()
                    .into_iter()
                    .map(|v| {
                        let mut i = json!({"bundleId": bundle_id(v), "version": format!("{v}.7.0")});
                        if self.granted {
                            i["path"] = json!(format!("/Applications/DxO PureRAW {v}.app"));
                        }
                        i
                    })
                    .collect();
                Ok(json!([{"program": "pureraw", "name": "DxO PureRAW", "granted": self.granted, "installed": installed}]))
            }
            "photo.inspect" => {
                if !self.catalog {
                    return Err("permission `catalog` not granted to this plug-in".into());
                }
                self.inspect(params.get("id").and_then(Value::as_str).ok_or("photo.inspect: `id` must be a string here")?)
            }
            "program.start" => self.start(&params),
            "fs.readText" => {
                let path = self.lent_path(&params)?;
                let bytes = fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
                if bytes.len() > MAX_IO_BYTES {
                    return Err(format!("{}: larger than {MAX_IO_BYTES} bytes", path.display()));
                }
                String::from_utf8(bytes).map(Value::from).map_err(|_| format!("{}: not UTF-8 text", path.display()))
            }
            "fs.exists" => Ok(Value::from(self.lent_path(&params)?.exists())),
            "settings.get" => {
                if self.old {
                    return Err("unknown host method \"settings.get\"".into());
                }
                if self.settings_broken {
                    return Err("settings.get: the settings file couldn't be read".into());
                }
                // as the host does: every key, its default when never set
                let mut v = json!({"addToAlbum": true, "stack": false, "version": ""});
                for (k, value) in &self.settings {
                    v[k] = value.clone();
                }
                Ok(v)
            }
            "library.add" => {
                // as the host does: the permission first, then the params
                if !self.library_add {
                    return Err("permission `libraryAdd` not granted to this plug-in".into());
                }
                if !params.is_object() {
                    return Err("library.add: params must be {path, from, stack?, album?}".into());
                }
                self.adds.push(params.clone());
                self.library_add(&params)
            }
            _ => Err(format!("unknown host method {method:?}")),
        }
    }

    fn log(&mut self, line: &str) {
        self.log.push(line.to_string());
    }
}

impl Drop for FakeHost {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
