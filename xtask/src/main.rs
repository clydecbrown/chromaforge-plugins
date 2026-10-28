//! Repository tasks, run as `cargo xtask <task>`. CI calls these, so a check that fails in CI fails
//! the same way here.
//!
//! - `ci`: everything a pull request runs (`check`, then `plugin` for every plug-in).
//! - `check`: fmt, clippy and the native tests of the whole workspace.
//! - `plugins`: the plug-in folder names as a JSON array (CI's matrix).
//! - `plugin <name>`: one plug-in's layout, its wasm32 clippy and its release build.
//! - `release <tag> [--out <dir>]`: checks a `<name>-v<version>` tag against `plugins/<name>`, builds
//!   that plug-in only, and writes `<name>-<version>.wasm`, its `.sha256`, `notes.md` (the changelog
//!   section) and `release.json` to `<dir>` (default `dist`).
//! - `index <release.json> --repo <owner/name> [--file <path>]`: puts that release into `index.json`.
//!
//! `CHROMAFORGE_SDK=<Chromaforge checkout>` builds against that checkout's `sdk/plugin` instead of the pinned
//! `rev`, for host features not merged yet: each cargo call gets the patch as `--config` and runs without
//! `--locked`, and `Cargo.lock` is put back afterwards, so nothing to commit changes. `release` refuses it.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use serde_json::{Value, json};
use sha2::{Digest, Sha256};

const WASM_TARGET: &str = "wasm32-unknown-unknown";

type Result<T, E = String> = std::result::Result<T, E>;

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    if let Err(e) = run(&args) {
        eprintln!("xtask: {e}");
        std::process::exit(1);
    }
}

fn run(args: &[String]) -> Result<()> {
    let root = root();
    let rest = args.get(1..).unwrap_or_default();
    let _lock = match sdk_override(&root)? {
        Some(_) if args.first().map(String::as_str) == Some("release") => {
            return Err("release builds use the pinned SDK: unset CHROMAFORGE_SDK".into());
        }
        Some(sdk) => {
            eprintln!("xtask: building against {} (CHROMAFORGE_SDK); Cargo.lock is put back afterwards", sdk.path.display());
            Some(LockGuard::new(&root)?)
        }
        None => None,
    };
    match args.first().map(String::as_str) {
        Some("ci") => {
            check(&root)?;
            for p in plugins(&root)? {
                plugin(&root, &p)?;
            }
            Ok(())
        }
        Some("check") => check(&root),
        Some("plugins") => {
            let names: Vec<_> = plugins(&root)?.into_iter().map(|p| p.dir).collect();
            println!("{}", json!(names));
            Ok(())
        }
        Some("plugin") => {
            let [name] = rest else { return Err("usage: plugin <name>".into()) };
            plugin(&root, &find_plugin(&root, name)?)
        }
        Some("release") => {
            let tag = rest.first().ok_or("usage: release <tag> [--out <dir>]")?;
            let out = flag(rest, "--out").map_or_else(|| root.join("dist"), PathBuf::from);
            release(&root, tag, &out)
        }
        Some("index") => {
            let record = rest.first().ok_or("usage: index <release.json> --repo <owner/name> [--file <path>]")?;
            let repo = flag(rest, "--repo").ok_or("index needs --repo <owner/name>")?;
            let file = flag(rest, "--file").map_or_else(|| root.join("index.json"), PathBuf::from);
            index(Path::new(record), &repo, &file)
        }
        _ => Err("usage: cargo xtask <ci | check | plugins | plugin <name> | release <tag> | index <release.json>>".into()),
    }
}

fn flag(args: &[String], name: &str) -> Option<String> {
    args.iter().position(|a| a == name).and_then(|i| args.get(i + 1)).cloned()
}

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).parent().expect("xtask lives in the workspace").to_path_buf()
}

fn cargo(root: &Path, args: &[&str]) -> Result<()> {
    cargo_with(root, args, &[])
}

fn cargo_with(root: &Path, args: &[&str], envs: &[(&str, String)]) -> Result<()> {
    let mut args: Vec<String> = args.iter().map(|a| a.to_string()).collect();
    // After the subcommand, not before: `cargo clippy` runs its own `cargo check`, which only sees the
    // subcommand's arguments. `cargo fmt` would pass `--config` on to rustfmt, and needs no SDK.
    if let Some(sdk) = sdk_override(root)?
        && args.first().is_some_and(|sub| sub != "fmt")
    {
        args.retain(|a| a != "--locked");
        args.splice(1..1, ["--config".to_string(), sdk.patch_config()]);
    }
    eprintln!("$ cargo {}", args.join(" "));
    let status = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(&args)
        .envs(envs.iter().map(|(k, v)| (k, v)))
        .current_dir(root)
        .status()
        .map_err(|e| format!("cargo: {e}"))?;
    if status.success() { Ok(()) } else { Err(format!("cargo {} failed", args.join(" "))) }
}

/// A local Chromaforge checkout to build the SDK from (`CHROMAFORGE_SDK`).
struct SdkOverride {
    /// The `git` URL of `lcc-plugin-sdk` in the workspace `Cargo.toml`: the source the patch replaces.
    git: String,
    /// `<checkout>/sdk/plugin`, absolute.
    path: PathBuf,
}

impl SdkOverride {
    fn patch_config(&self) -> String {
        // JSON string escapes are valid TOML basic-string escapes.
        format!("patch.{}.lcc-plugin-sdk.path={}", json!(self.git), json!(self.path.to_string_lossy()))
    }
}

fn sdk_override(root: &Path) -> Result<Option<SdkOverride>> {
    let Some(checkout) = env::var_os("CHROMAFORGE_SDK").filter(|v| !v.is_empty()) else { return Ok(None) };
    let path = Path::new(&checkout).join("sdk").join("plugin");
    let path = fs::canonicalize(&path)
        .map_err(|e| format!("CHROMAFORGE_SDK: {} is not a Chromaforge checkout with sdk/plugin ({e})", Path::new(&checkout).display()))?;
    let manifest = fs::read_to_string(root.join("Cargo.toml")).map_err(|e| format!("Cargo.toml: {e}"))?;
    let git = sdk_git(&manifest).ok_or("Cargo.toml: no `lcc-plugin-sdk = { git = \"…\" … }` line to patch")?.to_string();
    Ok(Some(SdkOverride { git, path }))
}

/// The `git` URL on the `lcc-plugin-sdk = { git = "…", … }` line of the workspace `Cargo.toml`.
fn sdk_git(manifest: &str) -> Option<&str> {
    let line = manifest.lines().find(|l| l.trim_start().starts_with("lcc-plugin-sdk"))?;
    let (_, after) = line.split_once("git")?;
    let after = after.trim_start().strip_prefix('=')?.trim_start().strip_prefix('"')?;
    after.split_once('"').map(|(url, _)| url)
}

/// Puts `Cargo.lock` back as it was when dropped: a patched SDK rewrites its source line.
struct LockGuard {
    path: PathBuf,
    saved: Vec<u8>,
}

impl LockGuard {
    fn new(root: &Path) -> Result<Self> {
        let path = root.join("Cargo.lock");
        let saved = fs::read(&path).map_err(|e| format!("Cargo.lock: {e}"))?;
        Ok(Self { path, saved })
    }
}

impl Drop for LockGuard {
    fn drop(&mut self) {
        if fs::read(&self.path).ok().as_deref() != Some(self.saved.as_slice())
            && let Err(e) = fs::write(&self.path, &self.saved)
        {
            eprintln!("xtask: could not put Cargo.lock back ({e}): `git checkout Cargo.lock` before committing");
        }
    }
}

fn check(root: &Path) -> Result<()> {
    cargo(root, &["fmt", "--all", "--check"])?;
    cargo(root, &["clippy", "--workspace", "--all-targets", "--locked", "--", "-D", "warnings"])?;
    cargo(root, &["test", "--workspace", "--locked"])
}

/// A crate in `plugins/`.
#[derive(Debug, Clone)]
struct Plugin {
    /// Folder name: the tag prefix, the label and the release file's name.
    dir: String,
    package: String,
    version: String,
    /// `[package.metadata.chromaforge] display-name`, for release titles.
    display_name: Option<String>,
    /// The `cdylib` target's name, which names the built `.wasm`.
    cdylib: Option<String>,
    path: PathBuf,
}

fn plugins(root: &Path) -> Result<Vec<Plugin>> {
    let out = Command::new(env::var("CARGO").unwrap_or_else(|_| "cargo".into()))
        .args(["metadata", "--no-deps", "--format-version", "1"])
        .current_dir(root)
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !out.status.success() {
        return Err(format!("cargo metadata: {}", String::from_utf8_lossy(&out.stderr)));
    }
    let meta: Value = serde_json::from_slice(&out.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let plugins_dir = root.join("plugins");
    let mut found = Vec::new();
    for pkg in meta["packages"].as_array().into_iter().flatten() {
        let manifest = PathBuf::from(pkg["manifest_path"].as_str().unwrap_or_default());
        let Some(dir) = manifest.parent() else { continue };
        if dir.parent() != Some(plugins_dir.as_path()) {
            continue;
        }
        let cdylib = pkg["targets"].as_array().into_iter().flatten().find_map(|t| {
            let kinds = t["kind"].as_array()?;
            kinds.iter().any(|k| k == "cdylib").then(|| t["name"].as_str().map(|n| n.replace('-', "_")))?
        });
        found.push(Plugin {
            dir: dir.file_name().unwrap_or_default().to_string_lossy().into_owned(),
            package: pkg["name"].as_str().unwrap_or_default().to_string(),
            version: pkg["version"].as_str().unwrap_or_default().to_string(),
            display_name: pkg["metadata"]["chromaforge"]["display-name"].as_str().map(str::to_string),
            cdylib,
            path: dir.to_path_buf(),
        });
    }
    found.sort_by(|a, b| a.dir.cmp(&b.dir));
    Ok(found)
}

fn find_plugin(root: &Path, name: &str) -> Result<Plugin> {
    plugins(root)?.into_iter().find(|p| p.dir == name).ok_or_else(|| format!("no plug-in crate in plugins/{name}"))
}

/// What every plug-in folder must have before it can be built or released.
fn layout_problems(p: &Plugin) -> Vec<String> {
    let mut problems = Vec::new();
    if !valid_prefix(&p.dir) {
        problems.push(format!("plugins/{}: folder names are lowercase letters, digits and '-', not starting or ending with '-'", p.dir));
    }
    if p.cdylib.is_none() {
        problems.push(format!("plugins/{}: [lib] crate-type needs \"cdylib\"", p.dir));
    }
    if p.display_name.is_none() {
        problems.push(format!("plugins/{}: Cargo.toml needs [package.metadata.chromaforge] display-name", p.dir));
    }
    for file in ["README.md", "CHANGELOG.md"] {
        if !p.path.join(file).is_file() {
            problems.push(format!("plugins/{}: {file} is missing", p.dir));
        }
    }
    problems
}

fn plugin(root: &Path, p: &Plugin) -> Result<()> {
    let problems = layout_problems(p);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    cargo(root, &["clippy", "-p", &p.package, "--lib", "--target", WASM_TARGET, "--locked", "--", "-D", "warnings"])?;
    build_wasm(root, p).map(|_| ())
}

/// Builds the release `.wasm`. Dependencies' panic locations hold their source paths, which sit under
/// Cargo's home folder: mapped to `/cargo` (and the workspace to `/src`), so the file doesn't carry a
/// user's home folder and the same commit builds to the same bytes on any machine with the same toolchain.
fn build_wasm(root: &Path, p: &Plugin) -> Result<PathBuf> {
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|h| Path::new(&h).join(".cargo")))
        .ok_or("neither CARGO_HOME nor HOME is set")?;
    let flags = [format!("--remap-path-prefix={}=/cargo", cargo_home.display()), format!("--remap-path-prefix={}=/src", root.display())];
    cargo_with(
        root,
        &["build", "-p", &p.package, "--lib", "--release", "--target", WASM_TARGET, "--locked"],
        &[("CARGO_ENCODED_RUSTFLAGS", flags.join("\x1f"))],
    )?;
    let target_dir = env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let wasm = target_dir.join(WASM_TARGET).join("release").join(format!("{}.wasm", p.cdylib.as_deref().unwrap_or(&p.package)));
    if wasm.is_file() { Ok(wasm) } else { Err(format!("{} was not built", wasm.display())) }
}

/// Lowercase letters, digits and `-`, starting and ending with a letter or digit.
fn valid_prefix(s: &str) -> bool {
    s.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && s.bytes().next().is_some_and(|b| b != b'-')
        && s.bytes().last().is_some_and(|b| b != b'-')
}

/// `x.y.z` with an optional `-pre` / `+build`, as Cargo takes it.
fn valid_version(v: &str) -> bool {
    let core = v.split(['-', '+']).next().unwrap_or_default();
    let parts: Vec<_> = core.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// `pureraw-v0.1.0` → (`pureraw`, `0.1.0`).
fn parse_tag(tag: &str) -> Result<(&str, &str)> {
    let (prefix, version) = tag.rsplit_once("-v").ok_or_else(|| format!("tag {tag:?} is not <plug-in>-v<version>"))?;
    if !valid_prefix(prefix) || !valid_version(version) {
        return Err(format!("tag {tag:?} is not <plug-in>-v<version> (e.g. pureraw-v0.1.0)"));
    }
    Ok((prefix, version))
}

/// The body of the `## [version]` (or `## version`, `## v<version>`) section, up to the next `## `.
fn changelog_section(text: &str, version: &str) -> Option<String> {
    let mut lines = text.lines();
    lines.by_ref().find(|line| {
        let Some(h) = line.strip_prefix("## ") else { return false };
        let h = h.trim_start();
        let h = h.strip_prefix('[').unwrap_or(h);
        let h = h.strip_prefix('v').unwrap_or(h);
        h.strip_prefix(version).is_some_and(|after| after.chars().next().is_none_or(|c| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'))))
    })?;
    let body: Vec<_> = lines.take_while(|line| !line.starts_with("## ")).collect();
    let body = body.join("\n").trim().to_string();
    (!body.is_empty()).then_some(body)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|b| format!("{b:02x}")).collect()
}

fn release(root: &Path, tag: &str, out: &Path) -> Result<()> {
    let (name, version) = parse_tag(tag)?;
    let p = find_plugin(root, name)?;
    let problems = layout_problems(&p);
    if !problems.is_empty() {
        return Err(problems.join("\n"));
    }
    if p.version != version {
        return Err(format!("tag {tag} says {version}, but plugins/{name}/Cargo.toml says {}", p.version));
    }
    let changelog = fs::read_to_string(p.path.join("CHANGELOG.md")).map_err(|e| format!("CHANGELOG.md: {e}"))?;
    let notes =
        changelog_section(&changelog, version).ok_or_else(|| format!("plugins/{name}/CHANGELOG.md has no (or an empty) section for {version}"))?;

    let built = build_wasm(root, &p)?;
    let bytes = fs::read(&built).map_err(|e| format!("{}: {e}", built.display()))?;
    let file = format!("{name}-{version}.wasm");
    let sha = sha256_hex(&bytes);
    let title = format!("{} plug-in {version}", p.display_name.as_deref().unwrap_or(name));

    fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let write = |file: &str, data: &[u8]| fs::write(out.join(file), data).map_err(|e| format!("{file}: {e}"));
    write(&file, &bytes)?;
    write(&format!("{file}.sha256"), format!("{sha}  {file}\n").as_bytes())?;
    write("notes.md", format!("{notes}\n\nSHA-256 of `{file}`: `{sha}`\n").as_bytes())?;
    let record = json!({"plugin": name, "name": p.display_name, "version": version, "tag": tag,
        "title": title, "file": file, "sha256": sha});
    write("release.json", format!("{record:#}\n").as_bytes())?;

    eprintln!("{title}: {} ({} bytes, sha256 {sha})", out.join(&file).display(), bytes.len());
    if let Some(gh) = env::var_os("GITHUB_OUTPUT") {
        let lines = format!("title={title}\nfile={file}\n");
        let mut text = fs::read_to_string(&gh).unwrap_or_default();
        text.push_str(&lines);
        fs::write(&gh, text).map_err(|e| format!("GITHUB_OUTPUT: {e}"))?;
    }
    Ok(())
}

/// Puts one release into the index: `{"format": 1, "plugins": {<name>: {…}}}`, keyed and sorted by
/// plug-in, so each plug-in has exactly one current entry.
fn update_index(index: &mut Value, record: &Value, repo: &str) -> Result<()> {
    let field = |k: &str| record[k].as_str().ok_or_else(|| format!("release.json has no {k}"));
    let (plugin, tag, file) = (field("plugin")?, field("tag")?, field("file")?);
    if !index.is_object() {
        *index = json!({"format": 1, "plugins": {}});
    }
    let plugins = index["plugins"].as_object_mut().ok_or("index.json: plugins is not an object")?;
    plugins.insert(
        plugin.to_string(),
        json!({
            "name": record["name"],
            "version": field("version")?,
            "tag": tag,
            "url": format!("https://github.com/{repo}/releases/download/{tag}/{file}"),
            "sha256": field("sha256")?,
            "releases": format!("https://github.com/{repo}/releases?q={plugin}-v&expanded=true"),
        }),
    );
    Ok(())
}

fn index(record: &Path, repo: &str, file: &Path) -> Result<()> {
    let read = |p: &Path| fs::read_to_string(p).map_err(|e| format!("{}: {e}", p.display()));
    let record: Value = serde_json::from_str(&read(record)?).map_err(|e| format!("release.json: {e}"))?;
    let mut index = if file.exists() { serde_json::from_str(&read(file)?).map_err(|e| format!("{}: {e}", file.display()))? } else { Value::Null };
    update_index(&mut index, &record, repo)?;
    fs::write(file, format!("{index:#}\n")).map_err(|e| format!("{}: {e}", file.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tag_splits_into_plugin_and_version() {
        assert_eq!(parse_tag("pureraw-v0.1.0"), Ok(("pureraw", "0.1.0")));
        assert_eq!(parse_tag("dxo-pureraw-v1.2.3-beta.1"), Ok(("dxo-pureraw", "1.2.3-beta.1")));
    }

    #[test]
    fn a_malformed_tag_is_refused() {
        for tag in ["pureraw", "v0.1.0", "pureraw-v0.1", "PureRAW-v0.1.0", "pureraw-vx.y.z", "-v0.1.0", "pure_raw-v0.1.0", "pureraw--v0.1.0"] {
            assert!(parse_tag(tag).is_err(), "{tag} should be refused");
        }
    }

    const CHANGELOG: &str = "# Changelog\n\n## [Unreleased]\n\n- next\n\n## [0.1.10] - 2026-11-02\n\n- ten\n\n\
        ## [0.1.1] - 2026-10-30\n\n### Fixed\n\n- one\n- two\n\n## 0.1.0\n\n- first\n\n## v0.0.9\n\n## 0.0.8\n\n- eight\n";

    #[test]
    fn the_changelog_section_is_the_one_for_that_version_only() {
        assert_eq!(changelog_section(CHANGELOG, "0.1.1").as_deref(), Some("### Fixed\n\n- one\n- two"));
        assert_eq!(changelog_section(CHANGELOG, "0.1.10").as_deref(), Some("- ten"));
        assert_eq!(changelog_section(CHANGELOG, "0.1.0").as_deref(), Some("- first"));
        assert_eq!(changelog_section(CHANGELOG, "0.0.8").as_deref(), Some("- eight"));
    }

    #[test]
    fn a_missing_or_empty_changelog_section_gives_none() {
        assert_eq!(changelog_section(CHANGELOG, "0.2.0"), None);
        assert_eq!(changelog_section(CHANGELOG, "0.0.9"), None, "empty section");
        assert_eq!(changelog_section(CHANGELOG, "0.1"), None, "a prefix of a version is not that version");
    }

    #[test]
    fn the_index_keeps_one_entry_per_plugin() {
        let mut index = Value::Null;
        let rec = |plugin: &str, v: &str| {
            json!({"plugin": plugin, "name": "X", "version": v, "tag": format!("{plugin}-v{v}"),
                "file": format!("{plugin}-{v}.wasm"), "sha256": "ab"})
        };
        update_index(&mut index, &rec("pureraw", "0.1.0"), "o/r").unwrap();
        update_index(&mut index, &rec("other", "1.0.0"), "o/r").unwrap();
        update_index(&mut index, &rec("pureraw", "0.2.0"), "o/r").unwrap();
        assert_eq!(index["format"], 1);
        assert_eq!(index["plugins"].as_object().unwrap().len(), 2);
        assert_eq!(index["plugins"]["pureraw"]["version"], "0.2.0");
        assert_eq!(index["plugins"]["pureraw"]["url"], "https://github.com/o/r/releases/download/pureraw-v0.2.0/pureraw-0.2.0.wasm");
        assert_eq!(index["plugins"]["other"]["version"], "1.0.0");
    }

    #[test]
    fn the_sdk_git_url_comes_from_the_workspace_manifest() {
        let manifest = "[workspace.dependencies]\n# lcc-plugin-sdk: a comment\n\
            lcc-plugin-sdk = { git = \"https://example.org/app\", rev = \"abc\" }\nserde_json = \"1\"\n";
        assert_eq!(sdk_git(manifest), Some("https://example.org/app"));
        assert_eq!(sdk_git("lcc-plugin-sdk = { path = \"../sdk\" }"), None, "a path dependency has nothing to patch");
        assert_eq!(sdk_git(&fs::read_to_string(root().join("Cargo.toml")).unwrap()), Some("https://github.com/emmertex/chromaforge"));
    }

    #[test]
    fn the_patch_config_quotes_url_and_path() {
        let sdk = SdkOverride { git: "https://example.org/app".into(), path: PathBuf::from("/a b/\"q\"/sdk/plugin") };
        assert_eq!(sdk.patch_config(), r#"patch."https://example.org/app".lcc-plugin-sdk.path="/a b/\"q\"/sdk/plugin""#);
    }

    #[test]
    fn sha256_is_lowercase_hex() {
        assert_eq!(sha256_hex(b"abc"), "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad");
    }
}
