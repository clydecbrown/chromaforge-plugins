//! A stand-in for DxO PureRAW 3 in the plug-in mode its Lightroom plug-in starts it in, as observed from
//! the outside (`--as-lightroom-plugin --lr-version <v> --batch-file <file>`). It does what PureRAW
//! does with the files, without processing any pixels:
//!
//! 1. refuses to start without `--lr-version`, and refuses an empty batch (no results, no trigger);
//! 2. reads the batch, one path per line, and deletes it; blank lines, relative and missing paths are
//!    dropped without a word, as PureRAW drops them;
//! 3. "processes" the rest in path order: each output is `<folder>/DxO/<name>-<ext>_DxO_DeepPRIME.dng`
//!    next to its original, written over an earlier one. A file whose name contains `fail` fails (no
//!    `-` line, no output), as an unreadable or truncated file does in PureRAW;
//! 4. writes the results list `dxo-imported-files-PureRawv3` into `$TMPDIR`, then the empty trigger
//!    file `dxo-trigger-PureRawv3`, and exits.
//!
//! A path sent twice is processed twice: one `-` line, the same output line twice. [`Mode`] picks the
//! other observed endings. The binary reads its mode from `FAKE_PURERAW_MODE` and its folder from
//! `TMPDIR`.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// The results list, in `$TMPDIR`.
pub const RESULTS: &str = "dxo-imported-files-PureRawv3";
/// The empty file written after the results.
pub const TRIGGER: &str = "dxo-trigger-PureRawv3";

/// How the run ends.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    /// The user processes the photos.
    #[default]
    Process,
    /// The user closes the dialog without processing: the batch is read and deleted, nothing else.
    Close,
    /// Another plug-in-mode PureRAW is open: the batch is never read, and nothing is written.
    Busy,
    /// Like `Process`, but every output line is written twice.
    RepeatOutputs,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "" | "process" => Some(Mode::Process),
            "close" => Some(Mode::Close),
            "busy" => Some(Mode::Busy),
            "repeat" => Some(Mode::RepeatOutputs),
            _ => None,
        }
    }
}

/// What a run did, for tests.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Outcome {
    /// The originals processed, in path order.
    pub processed: Vec<String>,
    /// Whether a results list was written.
    pub results: bool,
}

/// The output PureRAW writes for `original` with its default preferences.
pub fn output_of(original: &str) -> Option<String> {
    let path = Path::new(original);
    let folder = path.parent()?;
    let name = path.file_name()?.to_str()?;
    let (stem, ext) = name.rsplit_once('.').unwrap_or((name, ""));
    let out = folder.join("DxO").join(format!("{stem}-{ext}_DxO_DeepPRIME.dng"));
    out.to_str().map(str::to_string)
}

/// Runs as PureRAW would with `args` (without the program name) and `$TMPDIR` = `tmp`.
pub fn run(args: &[String], tmp: &Path, mode: Mode) -> io::Result<Outcome> {
    let flag = |name: &str| args.iter().position(|a| a == name).and_then(|i| args.get(i + 1));
    if !args.iter().any(|a| a == "--as-lightroom-plugin") {
        return Err(io::Error::other("not started as a plug-in: PureRAW would open its own window"));
    }
    let batch = flag("--batch-file").map(PathBuf::from).ok_or_else(|| io::Error::other("no --batch-file"))?;
    if mode == Mode::Busy {
        return Ok(Outcome::default());
    }
    let text = fs::read_to_string(&batch);
    let _ = fs::remove_file(&batch);
    // "Use option to indicate Lightroom version": refused after the batch is gone
    if flag("--lr-version").is_none() {
        return Ok(Outcome::default());
    }
    let text = text?;
    let lines: Vec<&str> = text.lines().map(|l| l.strip_suffix('\r').unwrap_or(l)).collect();
    let usable: Vec<&str> = lines.iter().copied().filter(|l| l.starts_with('/') && Path::new(l).is_file()).collect();
    if usable.is_empty() || mode == Mode::Close {
        return Ok(Outcome::default());
    }
    let sorted: BTreeSet<&str> = usable.iter().copied().collect();
    let processed: Vec<&str> =
        sorted.into_iter().filter(|p| !Path::new(p).file_name().and_then(|n| n.to_str()).is_some_and(|n| n.contains("fail"))).collect();

    let mut list = String::from("DxO PureRAW 3\nEND_OF_COLLECTION_SET_SECTION\n2026-10-24 10:13END_OF_COLLECTION_NAME_SECTION\n+ selectCollection\n");
    for p in &processed {
        list.push_str(&format!("- {p}\n"));
    }
    for (i, p) in processed.iter().enumerate() {
        let out = output_of(p).ok_or_else(|| io::Error::other(format!("{p}: no output name")))?;
        if let Some(dir) = Path::new(&out).parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&out, format!("fake DNG made from {p}\n{}\n", fs::read(p)?.len()))?;
        // PureRAW processes a path sent twice twice, under the same "-" line
        let times = usable.iter().filter(|u| **u == *p).count() * if mode == Mode::RepeatOutputs { 2 } else { 1 };
        for _ in 0..times {
            list.push_str(&format!("{i} {out}\n"));
        }
    }
    fs::write(tmp.join(RESULTS), list)?;
    fs::write(tmp.join(TRIGGER), "")?;
    Ok(Outcome { processed: processed.into_iter().map(str::to_string).collect(), results: true })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("fake-pureraw-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn args(batch: &Path) -> Vec<String> {
        ["--as-lightroom-plugin", "--lr-version", "99.0", "--batch-file"].iter().map(|s| s.to_string()).chain([batch.display().to_string()]).collect()
    }

    #[test]
    fn it_sorts_by_path_and_indexes_into_the_sorted_list() {
        let dir = scratch("sort");
        for f in ["b/x.dng", "a/y.dng"] {
            fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
            fs::write(dir.join(f), b"raw").unwrap();
        }
        let (b, a) = (dir.join("b/x.dng"), dir.join("a/y.dng"));
        fs::write(dir.join("batch"), format!("{}\n{}\n", b.display(), a.display())).unwrap();
        let out = run(&args(&dir.join("batch")), &dir, Mode::Process).unwrap();
        assert_eq!(out.processed, [a.display().to_string(), b.display().to_string()]);
        let list = fs::read_to_string(dir.join(RESULTS)).unwrap();
        assert!(
            list.ends_with(&format!(
                "- {}\n- {}\n0 {}\n1 {}\n",
                a.display(),
                b.display(),
                output_of(&a.display().to_string()).unwrap(),
                output_of(&b.display().to_string()).unwrap()
            )),
            "{list}"
        );
        assert!(dir.join(TRIGGER).is_file());
        assert!(!dir.join("batch").exists(), "the batch is deleted");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn closing_or_a_missing_lr_version_writes_nothing() {
        let dir = scratch("close");
        fs::write(dir.join("a.dng"), b"raw").unwrap();
        fs::write(dir.join("batch"), format!("{}\n", dir.join("a.dng").display())).unwrap();
        assert!(!run(&args(&dir.join("batch")), &dir, Mode::Close).unwrap().results);
        assert!(!dir.join(RESULTS).exists() && !dir.join("batch").exists());
        fs::write(dir.join("batch"), format!("{}\n", dir.join("a.dng").display())).unwrap();
        let no_lr = vec!["--as-lightroom-plugin".to_string(), "--batch-file".into(), dir.join("batch").display().to_string()];
        assert!(!run(&no_lr, &dir, Mode::Process).unwrap().results);
        assert!(!dir.join(RESULTS).exists());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_busy_pureraw_leaves_the_batch_unread() {
        let dir = scratch("busy");
        fs::write(dir.join("batch"), "/nowhere.dng\n").unwrap();
        assert!(!run(&args(&dir.join("batch")), &dir, Mode::Busy).unwrap().results);
        assert!(dir.join("batch").is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_output_name_keeps_the_extension() {
        assert_eq!(output_of("/p/a b/IMG_1.CR3").as_deref(), Some("/p/a b/DxO/IMG_1-CR3_DxO_DeepPRIME.dng"));
    }
}
