# Instructions for agents

This repository holds plug-ins for Chromaforge, the photo library and raw developer
([emmertex/lightcraft-classic](https://github.com/emmertex/lightcraft-classic)). Each plug-in is a WebAssembly
module that runs in Chromaforge's plug-in sandbox, inside the user's app, next to their photo library. The rules
below come from Chromaforge's own `AGENTS.md`, cut down to what applies to plug-ins. `README.md` explains the layout,
versions and releases, and `plugins/README.md` explains how to add a plug-in.

## Start here

- **The host's contract is Chromaforge's `docs/plugins.md` and `sdk/plugin`, at the `rev` this workspace pins**
  (`lcc-plugin-sdk` in `Cargo.toml`), not at Chromaforge's latest `main`. What a plug-in may call, its permissions
  and its budgets are all defined there.
- **Bumping the SDK `rev` is a review first.** Read `git diff <old>..<new> -- crates/plugins sdk/` in Chromaforge
  for changes to the ABI, the host calls and the permissions. Then bump, and run `cargo xtask ci`.
- **Quality gate before every commit: `cargo xtask ci`.** It runs fmt, clippy `-D warnings` and the native tests of
  every crate, then each plug-in's wasm32 clippy and release build. Commit only green states, one topic per
  commit, and never mix two plug-ins in one commit.

## Names

- The product is **Chromaforge**. Don't use any other or earlier product name in code, docs, UI strings or tests.
  The one exception is the repository URL above.
- Crates here have plain names (`handoff`, `pureraw`). Never `lcc-*`: those are Chromaforge's own crates.
- A plug-in's folder name is its tag prefix, its issue label and the name of its release file.

## Never crash (outranks feature work)

A plug-in runs inside the app people trust with their photos. A panic becomes a wasm trap. Chromaforge survives
it, but the user gets a vague error, and a hook can stop halfway through its work, for example after adding some
of a batch's files but not the rest.

- **Non-test code never panics:** no `unwrap()`, `expect()`, `panic!`, `unreachable!`, `todo!` or
  `unimplemented!`. Return `Err` with a message the user can act on, via `?`, `ok_or(..)?`, `let … else` or
  `if let`. Use a fallback (`unwrap_or…`) only where it can't silently write a wrong value. Sole exception: a
  provably infallible literal, as `#[allow(clippy::expect_used)]` plus `.expect("why it can't fail")`.
- **Enforced by clippy:** the root of every shipped crate (`plugins/*`, `crates/*`) carries
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo,
  clippy::unreachable)]`, and `clippy.toml` allows these in tests only. A new crate starts with the attribute.
  `xtask` and `tools/` are dev tools and don't need it.
- **No `unsafe`** (`unsafe_code = "deny"` for the workspace). The SDK's `export_plugin!` is the only unsafe token a
  plug-in needs.
- **Treat input as hostile:** the host's replies, dialog values, file contents, another program's output and
  arithmetic on any of these. Use `get()` instead of `[i]` / `[a..b]` for offsets that come from input; checked or
  saturating math for lengths, offsets and counts; no division by zero and no negative casts to `usize`. Cap
  allocations sized by input, and slice strings only at char boundaries.
- **Bound recursion and loops** over input: depth limits, seen-sets, a maximum number of files per call.
- **Expect refusals:** the user may grant only some of the requested permissions, and may revoke any of them later.
  Handle `permission … not granted` with a message that names the permission and what it's needed for.
- **Prove it:** every crash fix lands with a small regression test that panicked before the fix.

## How the host runs a plug-in

- **Every call runs in a fresh instance.** Keep state in the plug-in's namespace (`host::ns_get` / `ns_set`) or in
  files it may write, never in statics.
- **Budgets per call:** instructions, wall-clock time (time spent in host calls counts), 256 MiB of memory,
  10 000 host calls, 16 MiB per message, 8 MiB per file or HTTP body. Split big jobs into chunks the user can
  re-run, and make re-running safe: a second run must not add or change anything twice.
- **Ask for as few permissions as possible.** Each one is an unticked checkbox at install, so the user decides from
  the plug-in's README, whose **Permissions** section says what each permission is for. A plug-in that asks for
  more in an update gets nothing new until the user approves it.
- **The version comes from Cargo:** the manifest reads `env!("CARGO_PKG_VERSION")`, never a version typed into it.

## Clean-room and licences

- **A plug-in for a third-party program talks to it only through what was observed of its documented or visible
  behaviour** (command-line arguments, files it reads and writes). Never read, disassemble or copy anything
  inside a vendor's app bundle or its own plug-ins.
- **Never send OS-level synthetic input** (osascript or System Events keystrokes and clicks, `cliclick`,
  accessibility automation). It goes to whatever window is in front: the user's terminal or another app.
- **Never use Adobe assets** (icons, images, fonts, presets, profiles, LUTs, sample photos), not even
  open-licensed ones. Observing Lightroom's behaviour is fine; copying its assets is not.
- **No GPL, LGPL, AGPL or MPL code or dependencies.** The repository is MIT. A plug-in that needs another licence
  carries its own `LICENSE` and sets `license` in its `Cargo.toml`.
- **No licence headers in source files.** Licensing lives in `Cargo.toml`, `LICENSE` and per-folder notes.
- **Every asset or fixture is our own work, CC0 or permissively licensed**, with its source and licence noted next
  to it (`fixtures/README.md`). When in doubt about a licence, don't use the asset.

## Testing

- **Unit tests run natively.** Each plug-in builds as an `rlib` too, and host calls answer "no host" off wasm.
  Keep the logic in plain functions you can test without the host.
- **New features need at least one test that would fail without them.** Run each new test before the fix (or with
  the fix reverted) and watch it fail for the right reason.
- **Round trips run against the stand-ins in `tools/`**, never against the real third-party program in CI.
- **Never test against a real photo library or real settings.** Run Chromaforge with a scratch settings folder and a
  scratch or in-memory library (`--memory`) when you try a plug-in in the app. Drive it through the CLI, the control
  channel or MCP (`plugin.install`, `plugin.run`, `plugin.exportTo`, `plugin.log`).
- **Never commit media** or photos from a real library. Fixtures are small, generated or CC0.

## Working in parallel

- One git worktree and one `CARGO_TARGET_DIR` per agent. Never share a target dir between two checkouts: Cargo
  judges freshness by file times, so one checkout can reuse another's build and test the wrong code. Delete your
  target dir when you're done.
- Keep every `Cargo.toml` valid at all times: the workspace takes `crates/*`, `plugins/*` and `tools/*` by glob, so
  one broken manifest breaks everyone's build.
- Use your own scratch folder for files a test or run produces. Never delete shared paths.
