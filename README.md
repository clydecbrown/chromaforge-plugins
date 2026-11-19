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
| `index` branch | not in this tree: a branch holding only `index.json`, the current release of every plug-in, one top-level entry each: Chromaforge's update feed, kept up to date by the release workflow (Releases) |

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

## Signing

A release is signed by its author, so Chromaforge can tell who made a plug-in and that an update comes from the
same author as the version installed. The signature is a `photo-plugin.signature` custom section inside the `.wasm`
(the last section, written by `plugin-stamp`'s `sign`), so a file installed by hand carries it too and the release has
no separate signature file. `cargo xtask release <tag> --sign` does it, in this order: build, stamp, check the
manifest, **sign**, and only then checksum (`.sha256`, `release.json`, the `index.json` entry): signing changes the
bytes, so the published file and every checksum of it are the signed one. Nothing stamps a file after it is signed
(stamping drops the signature).

- **The key** is an ed25519 key made once with Chromaforge's `plugin-stamp keygen <file>`, which prints the public
  key's fingerprint (four groups of four hex digits). Its text goes into the repository secret `PLUGIN_SIGNING_KEY`
  and the fingerprint into the repository variable `PLUGIN_SIGNER`. The key never goes into the repository. Only the
  release workflow's build step gets it, on a tag push (pull requests never do), and `xtask` removes it from the
  environment of every `cargo` it starts, so no build script sees it. It is never printed.
- **The expected signer.** `--sign` refuses to sign unless the key belongs to `PLUGIN_SIGNER`, and checks the signed
  file as the app will (stamp intact, signature valid, signer as expected) before anything is written;
  `cargo xtask verify <file>` runs the same check on a file, and the workflow runs it again on the file it publishes.
  Without `--sign` (a local `cargo xtask release <tag>` into `dist/`) the file is stamped but unsigned, and says so.
- **A lost or replaced key** isn't recovered by a protocol: the next version is installed by hand, and Chromaforge
  asks about the new signer. Keep the key's backup out of the repository.
- **Rebuilding a release** to compare it with the published file: a rebuilder can't make the signature without the
  key, so stamp the published file again with `plugin-stamp` (stamping drops the signature and leaves the stamped
  file) and compare that with your own build of the tag. The signature itself is deterministic: the same key signs
  the same bytes to the same section.

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
   "<Display name> plug-in <version>". The release holds the signed `<name>-<version>.wasm` (see Signing), a `.sha256` of
   it and the changelog section as notes. Finally the workflow updates the plug-in's entry (top level, keyed by its folder name) in `index.json` on the `index` branch, the update feed.
   Other plug-ins aren't touched.

The `index` branch is made once, by hand, and a release fails (before building anything) when it is missing, so a
mistyped name can't start a second index that drops the other plug-ins' entries:

```sh
git switch --orphan index && echo '{"format": 1}' > index.json && git add index.json &&
  git commit -m "index: start" && git push origin index && git switch main
```

Before tagging, `cargo xtask release <tag>` builds the release into `dist/` locally (checking the manifest `id` against `index.json` in the workspace if there is one, or `--index <file>`), without publishing.

Release builds are reproducible: the toolchain is pinned (`rust-toolchain.toml`, bumped on purpose) and
`xtask` maps build paths to `/cargo` and `/src`. So a tag built anywhere with that toolchain matches the
published stamped file. Stamping is deterministic, so the stamped file is reproducible too; the published file and
its `.sha256` are of that file plus the author's signature, which a rebuild compares after taking it off (see Signing).

### Finding the newest release

The feed is `https://raw.githubusercontent.com/clydecbrown/chromaforge-plugins/index/index.json` (raw URLs are cached for
about five minutes, so a new release shows up after that).

GitHub's "latest release" is per repository, so it points at whichever plug-in shipped last. Each plug-in's
README links to its own releases instead (`…/releases?q=<name>-v&expanded=true`). `index.json`, on the `index` branch (so `main` takes pull requests only), lists every
plug-in's current version, download URL and SHA-256 in one file. Chromaforge reads it as a plug-in's update feed (a plug-in names this URL as its feed): the host takes the
top-level entry whose `id` equals the plug-in's manifest `id`, under any key, over `https` only and at most 1 MB:

```json
{
  "format": 1,
  "<name>": { "name": "…", "id": "io.example.name", "version": "0.1.0", "tag": "<name>-v0.1.0",
              "url": "…/<name>-0.1.0.wasm", "sha256": "…", "requires": ["settings"],
              "permissions": ["catalog", "programs"], "notes": "…",
              "changelog": "…/blob/<name>-v0.1.0/plugins/<name>/CHANGELOG.md",
              "releases": "…/releases?q=<name>-v&expanded=true" }
}
```

An app finds a plug-in's entry by `id`, the manifest's, since the key is the folder name (so no plug-in is named
`format`). Everything but `notes` is
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
