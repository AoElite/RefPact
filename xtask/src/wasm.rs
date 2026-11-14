//! `cargo xtask wasm`: builds the core as WebAssembly for the TypeScript package.
//!
//! The module is `crates/refpact-wasm`, built with the `wasm` profile of the workspace and
//! copied to `packages/refpact/wasm/refpact_core.wasm`, where the package loads it. The copy
//! is written only when it changed, so a watcher sees no change after a build that did not
//! change the core.

use std::process::Command;

pub const USAGE: &str = "cargo xtask wasm    build the core for the TypeScript package";

const TARGET: &str = "wasm32-unknown-unknown";

pub fn run(args: &[String]) -> Result<(), String> {
    if !args.is_empty() {
        return Err(format!("usage: {USAGE}"));
    }
    let root = crate::root();
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".to_owned());
    let status = Command::new(cargo)
        .current_dir(&root)
        .args([
            "build",
            "--quiet",
            "--package",
            "refpact-wasm",
            "--profile",
            "wasm",
        ])
        .args(["--target", TARGET])
        .status()
        .map_err(|error| format!("cannot run cargo: {error}"))?;
    if !status.success() {
        return Err(format!(
            "the build of the core failed. Is the target installed? `rustup target add {TARGET}`"
        ));
    }
    let built = root
        .join("target")
        .join(TARGET)
        .join("wasm")
        .join("refpact_wasm.wasm");
    let bytes = std::fs::read(&built)
        .map_err(|error| format!("cannot read {}: {error}", built.display()))?;
    let dir = root.join("packages").join("refpact").join("wasm");
    let copy = dir.join("refpact_core.wasm");
    if std::fs::read(&copy).ok().as_deref() != Some(bytes.as_slice()) {
        std::fs::create_dir_all(&dir)
            .and_then(|()| std::fs::write(&copy, &bytes))
            .map_err(|error| format!("cannot write {}: {error}", copy.display()))?;
    }
    Ok(())
}
