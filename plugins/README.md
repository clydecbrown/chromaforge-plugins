# Plug-ins

One folder per plug-in, each a crate released on its own (see the top-level README → Releases). The folder
name is the plug-in's short name: its tag prefix (`<name>-v<version>`), its issue label and the name of its
release file (`<name>-<version>.wasm`). Lowercase letters, digits and `-`.

## Adding one

`plugins/<name>/` needs:

- `Cargo.toml`:

  ```toml
  [package]
  name = "<name>"
  version = "0.1.0"            # the plug-in's version: the release tag must match it
  edition.workspace = true
  license.workspace = true
  publish.workspace = true

  [package.metadata.chromaforge]
  display-name = "<Name>"      # release titles: "<Name> plug-in 0.1.0"

  [lib]
  crate-type = ["cdylib", "rlib"]   # rlib: the native tests

  [dependencies]
  lcc-plugin-sdk.workspace = true

  [lints]
  workspace = true
  ```

- `src/lib.rs` whose manifest takes its version from Cargo, so what Chromaforge shows can't drift from the
  release:

  ```rust
  const MANIFEST: &str = concat!(
      r#"{"id": "<reverse-dns id>", "name": "<Name>", "version": ""#,
      env!("CARGO_PKG_VERSION"),
      r#"", "permissions": {…}, …}"#
  );
  ```

- `README.md`: what it does, a **Permissions** section (each permission it asks for and why: Chromaforge
  shows them unticked on install, and the user decides from this), and a link to its own releases,
  `https://github.com/clydecbrown/chromaforge-plugins/releases?q=<name>-v&expanded=true` (GitHub's "latest
  release" is per repository, so it would point at whichever plug-in shipped last).
- `CHANGELOG.md` with a `## [x.y.z] - YYYY-MM-DD` section per release; the release notes are that section.
- An entry in `.github/ISSUE_TEMPLATE/*.yml` (the plug-in dropdown) and a label named `<name>`.

`cargo xtask plugin <name>` checks the layout and builds the wasm.

A plug-in that needs another licence than the repository's MIT carries its own `LICENSE` in its folder and
sets `license` in its `Cargo.toml`.
