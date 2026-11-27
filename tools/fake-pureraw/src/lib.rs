//! A stand-in for DxO PureRAW in the plug-in mode its Lightroom plug-in starts it in, as observed from
//! the outside (`--as-lightroom-plugin --lr-version <v> --batch-file <file>`). It does what PureRAW 3
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
//! other observed endings.
//!
//! [`run_as`] takes the version. PureRAW 2 differs as observed: no header but the collection's name,
//! and after each photo (in file name order) it appends a `- <original>` / `0 <output>` pair and writes
//! the trigger; at the end it appends `+ selectCollection` and the same block as PureRAW 3, and writes
//! the trigger again. Any other version is PureRAW 3's format with its own number, as the plug-in
//! guesses for versions it hasn't seen.
//!
//! The binary reads its mode from `FAKE_PURERAW_MODE`, its version from `FAKE_PURERAW_VERSION` (3 if
//! unset) and its folder from `TMPDIR`.

use std::collections::BTreeSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// How PureRAW `version` spells its file names: PureRAW 6 with a capital RAW, as observed; earlier ones
/// `PureRawv`.
fn spelling(version: u32) -> &'static str {
    if version == 6 { "PureRAWv" } else { "PureRawv" }
}

/// PureRAW `version`'s results list, in `$TMPDIR`.
pub fn results_file(version: u32) -> String {
    format!("dxo-imported-files-{}{version}", spelling(version))
}

/// The empty file PureRAW `version` writes after the results.
pub fn trigger_file(version: u32) -> String {
    format!("dxo-trigger-{}{version}", spelling(version))
}

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
    /// The user cancels while the second photo is processed: only the first is listed.
    Cancel,
}

impl Mode {
    pub fn parse(s: &str) -> Option<Mode> {
        match s {
            "" | "process" => Some(Mode::Process),
            "close" => Some(Mode::Close),
            "busy" => Some(Mode::Busy),
            "repeat" => Some(Mode::RepeatOutputs),
            "cancel" => Some(Mode::Cancel),
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

/// Runs as PureRAW 3 would with `args` (without the program name) and `$TMPDIR` = `tmp`.
pub fn run(args: &[String], tmp: &Path, mode: Mode) -> io::Result<Outcome> {
    run_as(args, tmp, mode, 3)
}

/// Runs as PureRAW `version` would.
pub fn run_as(args: &[String], tmp: &Path, mode: Mode, version: u32) -> io::Result<Outcome> {
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
    let name_of = |p: &str| Path::new(p).file_name().and_then(|n| n.to_str()).unwrap_or(p).to_string();
    let sorted: BTreeSet<&str> = usable.iter().copied().collect();
    // processed one by one in file name order; a cancel keeps only the first
    let mut order: Vec<&str> = sorted.iter().copied().filter(|p| !name_of(p).contains("fail")).collect();
    order.sort_by_key(|p| (name_of(p), p.to_string()));
    if mode == Mode::Cancel {
        order.truncate(1);
    }
    let (results, trigger) = (tmp.join(results_file(version)), tmp.join(trigger_file(version)));

    let mut list = if version == 2 {
        String::from("2026-10-24 15:50\n")
    } else {
        format!("DxO PureRAW {version}\nEND_OF_COLLECTION_SET_SECTION\n2026-10-24 10:13END_OF_COLLECTION_NAME_SECTION\n")
    };
    for p in &order {
        let out = output_of(p).ok_or_else(|| io::Error::other(format!("{p}: no output name")))?;
        if let Some(dir) = Path::new(&out).parent() {
            fs::create_dir_all(dir)?;
        }
        fs::write(&out, format!("fake DNG made from {p}\n{}\n", fs::read(p)?.len()))?;
        if version == 2 {
            list.push_str(&format!("- {p}\n0 {out}\n"));
            fs::write(&results, &list)?;
            fs::write(&trigger, "")?;
        }
    }
    // the final block: the processed originals sorted by path, then the outputs indexed into them
    let processed: Vec<&str> = sorted.into_iter().filter(|p| order.contains(p)).collect();
    list.push_str("+ selectCollection\n");
    for p in &processed {
        list.push_str(&format!("- {p}\n"));
    }
    for (i, p) in processed.iter().enumerate() {
        let out = output_of(p).ok_or_else(|| io::Error::other(format!("{p}: no output name")))?;
        // PureRAW processes a path sent twice twice, under the same "-" line
        let times = usable.iter().filter(|u| **u == *p).count() * if mode == Mode::RepeatOutputs { 2 } else { 1 };
        for _ in 0..times {
            list.push_str(&format!("{i} {out}\n"));
        }
    }
    fs::write(&results, list)?;
    fs::write(&trigger, "")?;
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
        let list = fs::read_to_string(dir.join(results_file(3))).unwrap();
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
        assert!(dir.join(trigger_file(3)).is_file());
        assert!(!dir.join("batch").exists(), "the batch is deleted");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn closing_or_a_missing_lr_version_writes_nothing() {
        let dir = scratch("close");
        fs::write(dir.join("a.dng"), b"raw").unwrap();
        fs::write(dir.join("batch"), format!("{}\n", dir.join("a.dng").display())).unwrap();
        assert!(!run(&args(&dir.join("batch")), &dir, Mode::Close).unwrap().results);
        assert!(!dir.join(results_file(3)).exists() && !dir.join("batch").exists());
        fs::write(dir.join("batch"), format!("{}\n", dir.join("a.dng").display())).unwrap();
        let no_lr = vec!["--as-lightroom-plugin".to_string(), "--batch-file".into(), dir.join("batch").display().to_string()];
        assert!(!run(&no_lr, &dir, Mode::Process).unwrap().results);
        assert!(!dir.join(results_file(3)).exists());
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
    fn pureraw_6_names_its_files_with_a_capital_raw_as_observed() {
        assert_eq!(results_file(6), "dxo-imported-files-PureRAWv6");
        assert_eq!(trigger_file(6), "dxo-trigger-PureRAWv6");
        assert_eq!(results_file(3), "dxo-imported-files-PureRawv3", "the others keep PureRAW 3's spelling");
        assert_eq!(trigger_file(3), "dxo-trigger-PureRawv3");
    }

    fn photos(dir: &Path, names: &[&str]) -> Vec<String> {
        names
            .iter()
            .map(|f| {
                let p = dir.join(f);
                fs::create_dir_all(p.parent().unwrap()).unwrap();
                fs::write(&p, b"raw").unwrap();
                p.display().to_string()
            })
            .collect()
    }

    #[test]
    fn pureraw_2_lists_a_pair_per_photo_in_file_name_order_then_the_final_block() {
        let dir = scratch("v2");
        let p = photos(&dir, &["a/z.dng", "b/y.dng"]);
        fs::write(dir.join("batch"), format!("{}\n{}\n", p[0], p[1])).unwrap();
        let out = run_as(&args(&dir.join("batch")), &dir, Mode::Process, 2).unwrap();
        assert_eq!(out.processed, [p[0].clone(), p[1].clone()], "sorted by path in the final block");
        let (oz, oy) = (output_of(&p[0]).unwrap(), output_of(&p[1]).unwrap());
        let expected =
            format!("2026-10-24 15:50\n- {y}\n0 {oy}\n- {z}\n0 {oz}\n+ selectCollection\n- {z}\n- {y}\n0 {oz}\n1 {oy}\n", y = p[1], z = p[0]);
        assert_eq!(fs::read_to_string(dir.join(results_file(2))).unwrap(), expected);
        assert!(dir.join(trigger_file(2)).is_file());
        assert!(!dir.join(results_file(3)).exists(), "only the v2 names");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_cancel_lists_only_the_first_photo_processed() {
        for version in [2, 3] {
            let dir = scratch(&format!("cancel-{version}"));
            let p = photos(&dir, &["a/2.dng", "b/1.dng"]);
            fs::write(dir.join("batch"), format!("{}\n{}\n", p[0], p[1])).unwrap();
            let out = run_as(&args(&dir.join("batch")), &dir, Mode::Cancel, version).unwrap();
            assert_eq!(out.processed, [p[1].clone()], "v{version}: 1.dng comes first by name");
            let list = fs::read_to_string(dir.join(results_file(version))).unwrap();
            assert!(!list.contains(&p[0]), "v{version}: {list}");
            let _ = fs::remove_dir_all(&dir);
        }
    }

    #[test]
    fn another_version_writes_pureraw_3_s_format_under_its_own_names() {
        let dir = scratch("v6");
        let p = photos(&dir, &["a.dng"]);
        fs::write(dir.join("batch"), format!("{}\n", p[0])).unwrap();
        run_as(&args(&dir.join("batch")), &dir, Mode::Process, 6).unwrap();
        let list = fs::read_to_string(dir.join(results_file(6))).unwrap();
        assert!(list.starts_with("DxO PureRAW 6\nEND_OF_COLLECTION_SET_SECTION\n"), "{list}");
        assert!(list.ends_with(&format!("+ selectCollection\n- {}\n0 {}\n", p[0], output_of(&p[0]).unwrap())), "{list}");
        assert!(dir.join(trigger_file(6)).is_file());
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_output_name_keeps_the_extension() {
        assert_eq!(output_of("/p/a b/IMG_1.CR3").as_deref(), Some("/p/a b/DxO/IMG_1-CR3_DxO_DeepPRIME.dng"));
    }
}
