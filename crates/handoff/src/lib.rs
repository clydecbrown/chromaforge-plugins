//! Shared logic for plug-ins that send files to another program and take the results back.
//!
//! Never released alone: each plug-in that uses it ships it inside its own `.wasm`. It is versioned
//! with the repository (`0.0.0`, `publish = false`); a change here goes out in the next release of
//! each plug-in that wants it.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, clippy::unimplemented, clippy::todo, clippy::unreachable)]
