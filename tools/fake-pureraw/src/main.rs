//! `fake-pureraw --as-lightroom-plugin --lr-version <v> --batch-file <file>`: see the library.

use std::path::PathBuf;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let tmp = std::env::var_os("TMPDIR").map_or_else(std::env::temp_dir, PathBuf::from);
    let mode = std::env::var("FAKE_PURERAW_MODE").unwrap_or_default();
    let Some(mode) = fake_pureraw::Mode::parse(&mode) else {
        eprintln!("fake-pureraw: unknown FAKE_PURERAW_MODE {mode:?} (process, close, busy, repeat)");
        return ExitCode::FAILURE;
    };
    match fake_pureraw::run(&args, &tmp, mode) {
        Ok(_) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("fake-pureraw: {e}");
            ExitCode::FAILURE
        }
    }
}
