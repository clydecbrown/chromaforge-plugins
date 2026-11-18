# Chromaforge plug-ins

Plug-ins for [Chromaforge](https://github.com/emmertex/chromaforge), the photo library and raw
developer. Each one is a WebAssembly module built with Chromaforge's guest SDK (`sdk/plugin` there, ABI v1;
see its `docs/plugins.md`). You install it from File ▸ Plug-ins ▸ Plug-in Manager… and choose which
permissions to grant.

Several plug-ins share this repository, and each one is released on its own.

## Layout

| Path | What |
|---|---|
| `plugins/<name>/` | one plug-in per folder: its crate (the version in `Cargo.toml` is the plug-in's version), `README.md` with a **Permissions** section, `CHANGELOG.md`. How to add one: `plugins/README.md` |
| `crates/` | shared code, never released alone (`handoff`: sending files to another program and taking the results back) |
| `tools/` | test stand-ins for the programs plug-ins talk to, so the round trip runs in CI. Every folder here is a crate (the workspace takes `tools/*`) |
| `fixtures/` | shared test input |
| `xtask/` | repository tasks: CI checks, release builds, `index.json` |
| `index.json` | the current release of every plug-in, kept up to date by the release workflow |

## Building

```sh
rustup target add wasm32-unknown-unknown   # rust-toolchain.toml asks for it too
cargo xtask ci                             # what CI runs on a pull request
cargo xtask plugin <name>                  # one plug-in: layout check, wasm32 clippy, release build
# → target/wasm32-unknown-unknown/release/stamped/<name>.wasm  (the stamped file)
```

`cargo xtask check` runs fmt, clippy and the native tests of the whole workspace (shared crates, stand-ins
and the round trips included). The SDK is a git dependency pinned to a Chromaforge commit
(`[workspace.dependencies]` in `Cargo.toml`). Bump it on purpose, after reading what changed in `sdk/plugin`.

To build against host features that aren't merged yet, point `CHROMAFORGE_SDK` at a Chromaforge checkout:
`CHROMAFORGE_SDK=../chromaforge cargo xtask ci`. Every `xtask` build then uses that checkout's `sdk/plugin`,
and `Cargo.lock` is put back afterwards, so nothing to commit changes. `cargo xtask release` refuses it: releases
use the pinned SDK.

### Stamping

The build stamps each plug-in: `xtask` copies the built `.wasm` to `release/stamped/` and writes into the copy a
`photo-plugin.manifest` custom section, so Chromaforge can read the manifest without running the module. The
section is a copy of what `lcc_manifest` returns, not a second source: change the manifest in the plug-in's code,
never in the section. The tool is Chromaforge's `plugin-stamp` (`sdk/stamp`), a git dependency of `xtask` at the
same `rev` as the SDK (`Cargo.toml` keeps the two equal; a test checks it). After stamping, `xtask` reads the file
back and requires exactly one section equal to `lcc_manifest`; a refusal fails the build, naming the plug-in, and
leaves no `.wasm`. The stamped file is the one `release` checksums and publishes.

The build then reads `id` and `version` back from the section and checks them against the package: the manifest's
`version` must equal `Cargo.toml`'s, and both fields must be non-empty strings. `release` also refuses a manifest
`id` that differs from the one `index.json` already lists for that plug-in (an app treats a changed `id` as a
different plug-in, and its grants are lost); a first release, or an entry with no `id`, has nothing to compare.

## Versions

Each plug-in has its own version, in its `Cargo.toml`. Its manifest reads it with `env!("CARGO_PKG_VERSION")`,
so the version Chromaforge shows can't drift from the release. Shared crates stay at `0.0.0` with
`publish = false` and are versioned with the repository. A change to one reaches every plug-in that uses it
in CI, and each of those plug-ins ships it in a release of its own when it wants the change.

## Releases

A tag `<name>-v<version>` releases that plug-in alone:

1. On a branch, set the new `version` in `plugins/<name>/Cargo.toml` and add a `## [<version>] - <date>`
   section to its `CHANGELOG.md`. Merge to `main`.
2. Tag the merge on `main` and push the tag: `git tag ccb-dxo-pureraw-v0.1.0 && git push origin ccb-dxo-pureraw-v0.1.0`.
3. The release workflow (`.github/workflows/release.yml`) runs `cargo xtask release <tag>`. That checks the
   tag is on `main`, that `plugins/<name>` exists, and that its version and changelog section match the tag.
   Then it builds only that plug-in for `wasm32-unknown-unknown` and publishes a release titled
   "<Display name> plug-in <version>". The release holds `<name>-<version>.wasm`, a `.sha256` and the changelog
   section as notes. Finally the workflow updates the plug-in's entry in `index.json` on `main`.
   Other plug-ins aren't touched.

Before tagging, `cargo xtask release <tag>` builds the release into `dist/` locally, without publishing.

Release builds are reproducible: the toolchain is pinned (`rust-toolchain.toml`, bumped on purpose) and
`xtask` maps build paths to `/cargo` and `/src`. So a tag built anywhere with that toolchain matches the
published `.sha256`. Stamping is deterministic, so the stamped file is reproducible too.

### Finding the newest release

GitHub's "latest release" is per repository, so it points at whichever plug-in shipped last. Each plug-in's
README links to its own releases instead (`…/releases?q=<name>-v&expanded=true`). `index.json` lists every
plug-in's current version, download URL and SHA-256 in one file, in the form a plug-in store could read:

```json
{
  "format": 1,
  "plugins": {
    "<name>": { "name": "…", "id": "io.example.name", "version": "0.1.0", "tag": "<name>-v0.1.0",
                "url": "…/<name>-0.1.0.wasm", "sha256": "…", "requires": ["settings"],
                "permissions": ["catalog", "programs"], "notes": "…",
                "changelog": "…/blob/<name>-v0.1.0/plugins/<name>/CHANGELOG.md",
                "releases": "…/releases?q=<name>-v&expanded=true" }
  }
}
```

An app finds a plug-in's entry by `id`, the manifest's, since the key is the folder name. Everything but `notes` is
read from the built module, never typed by hand:

- `id`, `requires` (host features, as the manifest has them) and `permissions` (the names of the permission
  groups it asks for, such as `catalog` or `programs`; a list for a quick look before downloading, while the
  values themselves are in the module and shown at install) come from the manifest section. A plug-in that asks
  for no permissions has an empty list.
- `notes` is the changelog section for the released version as plain text (no headings, links or markup), at most
  280 characters, cut on a word boundary with `…`. An entry is replaced whole by each release, so notes, `requires`
  and `permissions` never carry over from an earlier version. `release` already needs a changelog section for the
  version; if the section reduces to nothing, the entry has no `notes` and `index` says so.
- `changelog` links the plug-in's `CHANGELOG.md` at the release's tag; `releases` is still the list of releases.
- `format` stays 1: only fields were added, and entries from before them are read as they were.

## CI

On every pull request: the workspace checks, then a matrix over `plugins/*` that runs each plug-in's wasm32
clippy and release build and keeps the `.wasm` as an artifact for a week. Every plug-in builds on every
change: with a few plug-ins that's simpler than path filters.

## Issues

Each plug-in has a label named like its folder, and the issue templates ask which plug-in an issue is about.

## Licence

MIT (`LICENSE`). A plug-in that needs another licence carries its own `LICENSE` in its folder and sets
`license` in its `Cargo.toml`.
