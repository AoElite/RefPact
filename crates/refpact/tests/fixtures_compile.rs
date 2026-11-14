//! The fixtures are real crates. If they did not compile, the planted forms would not be
//! proof that the library reads real Rust.

use std::process::Command;

fn check(fixture: &str) {
    let manifest = format!(
        "{}/../../fixtures/rust/{fixture}/Cargo.toml",
        env!("CARGO_MANIFEST_DIR")
    );
    let target = format!("{}/../../target/fixtures", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO"))
        .args([
            "check",
            "--offline",
            "--quiet",
            "--tests",
            "--manifest-path",
            &manifest,
        ])
        .env("CARGO_TARGET_DIR", target)
        .output()
        .expect("cargo runs");
    assert!(
        output.status.success(),
        "{fixture} does not compile:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn the_clean_fixture_compiles() {
    check("app");
}

#[test]
fn the_planted_fixture_compiles() {
    check("planted");
}

#[test]
fn the_workspace_fixture_compiles() {
    check("workspace");
}

#[test]
fn the_layered_fixture_compiles() {
    check("layered");
}

#[test]
fn the_fixture_of_the_calls_compiles() {
    check("calls");
}

#[test]
fn the_fixture_of_the_packages_compiles() {
    check("packages");
}
