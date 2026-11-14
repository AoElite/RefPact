//! The tasks of the refpact repository. Run them with `cargo xtask <task>`.
//!
//! - `bump current|patch|minor|major|X.Y.Z` sets the version of the crate and of the
//!   TypeScript package together. See `bump.rs`.
//! - `mutants [rust|ts|all] [filter]` checks that every detection has a test that fails
//!   without it. See `mutants/mod.rs`.
//! - `wasm` builds the core as WebAssembly for the TypeScript package. See `wasm.rs`.

mod bump;
mod mutants;
mod wasm;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("bump") => bump::run(&args[1..]),
        Some("mutants") => mutants::run(&args[1..]),
        Some("wasm") => wasm::run(&args[1..]),
        _ => Err(format!(
            "usage:\n    {}\n    {}\n    {}",
            bump::USAGE,
            mutants::USAGE,
            wasm::USAGE
        )),
    };
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("xtask: {message}");
            ExitCode::FAILURE
        }
    }
}

/// The root of the repository, the parent of the directory of this crate.
fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("the xtask crate is in the repository")
        .to_path_buf()
}
