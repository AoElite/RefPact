//! The fixtures are real crates. If they did not compile, the planted forms would not be
//! proof that the library reads real Rust.

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// The longest that the check of one fixture may take. A build that waits for something
/// that does not come, such as a lock on the directory of the build, is stopped.
const LIMIT: Duration = Duration::from_secs(10 * 60);

fn check(fixture: &str) {
    let manifest = format!(
        "{}/../../fixtures/rust/{fixture}/Cargo.toml",
        env!("CARGO_MANIFEST_DIR")
    );
    let target = format!("{}/../../target/fixtures", env!("CARGO_MANIFEST_DIR"));
    let mut child = Command::new(env!("CARGO"))
        .args([
            "check",
            "--offline",
            "--quiet",
            "--tests",
            "--manifest-path",
            &manifest,
        ])
        .env("CARGO_TARGET_DIR", target)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped())
        .spawn()
        .expect("cargo runs");
    // Another thread reads what cargo says, so that cargo does not wait for a reader.
    let mut errors = child.stderr.take().expect("the error output of cargo");
    let said = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = errors.read_to_string(&mut text);
        text
    });
    let end = Instant::now() + LIMIT;
    let status = loop {
        match child.try_wait().expect("cargo is waited for") {
            Some(status) => break Some(status),
            None if Instant::now() < end => std::thread::sleep(Duration::from_millis(20)),
            None => {
                let _ = child.kill();
                let _ = child.wait();
                break None;
            }
        }
    };
    let said = said.join().expect("the output of cargo is read");
    let status = status.unwrap_or_else(|| {
        panic!(
            "the check of {fixture} did not end in {} minutes:\n{said}",
            LIMIT.as_secs() / 60
        )
    });
    assert!(status.success(), "{fixture} does not compile:\n{said}");
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
