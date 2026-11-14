//! `cargo xtask bump`: sets the version of refpact, the crate and the TypeScript package
//! together.
//!
//! ```text
//! cargo xtask bump current              print the version, and check that all agree
//! cargo xtask bump patch|minor|major    0.1.0 -> 0.1.1 | 0.2.0 | 1.0.0
//! cargo xtask bump 1.2.3                an exact version
//! ```
//!
//! The version is in four files: `version` of `[workspace.package]` in `Cargo.toml`, the
//! entry of each crate in `Cargo.lock`, `version` in `packages/refpact/package.json`, and the
//! version of the core that `crates/refpact/Cargo.toml` requires. That requirement is exact:
//! the crate and its core are released together, so an update of the crate brings its core.
//! The task edits the text of each and keeps the rest as it is. It prints the new version.

use std::cmp::Reverse;
use std::fs;

use crate::root;

pub const USAGE: &str = "cargo xtask bump current|patch|minor|major|X.Y.Z";
const FILES: [&str; 4] = [
    "Cargo.toml",
    "Cargo.lock",
    "packages/refpact/package.json",
    "crates/refpact/Cargo.toml",
];
const CRATES: [&str; 3] = ["refpact", "refpact-core", "refpact-wasm"];
/// The start of the line of the core in the dependencies of the crate, and the start of its
/// exact requirement in that line. The rest of the line can change.
const CORE: [&str; 2] = ["\nrefpact-core = ", "version = \"="];

/// A version in the text of one of `FILES`.
struct Place {
    label: String,
    file: usize,
    start: usize,
    end: usize,
}

pub fn run(args: &[String]) -> Result<(), String> {
    let [request] = args else {
        return Err(format!("usage: {USAGE}"));
    };
    let root = root();
    let paths = FILES.map(|file| root.join(file));
    let mut texts = Vec::new();
    for path in &paths {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
        texts.push(text);
    }

    let places = places(&texts)?;
    let value = |place: &Place| &texts[place.file][place.start..place.end];
    let old = value(&places[0]).to_string();
    if places.iter().any(|place| value(place) != old) {
        let found: Vec<String> = places
            .iter()
            .map(|place| format!("{} {}", place.label, value(place)))
            .collect();
        return Err(format!("the versions do not agree: {}", found.join(", ")));
    }
    if request == "current" {
        println!("{old}");
        return Ok(());
    }

    let [major, minor, patch] =
        parse(&old).ok_or_else(|| format!("the current version {old} is not X.Y.Z"))?;
    let new = match request.as_str() {
        "major" => format!("{}.0.0", major + 1),
        "minor" => format!("{major}.{}.0", minor + 1),
        "patch" => format!("{major}.{minor}.{}", patch + 1),
        exact if parse(exact).is_some() => exact.to_string(),
        other => return Err(format!("not a bump or a version: {other}")),
    };
    if new == old {
        return Err(format!("the version is already {old}"));
    }

    // From the last place to the first, so that each edit keeps the positions before it.
    let mut places = places;
    places.sort_by_key(|place| Reverse((place.file, place.start)));
    for place in &places {
        texts[place.file].replace_range(place.start..place.end, &new);
    }
    for (path, text) in paths.iter().zip(&texts) {
        fs::write(path, text)
            .map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    println!("{new}");
    Ok(())
}

/// The places of the version in the texts of `FILES`. The first is the one in `Cargo.toml`.
fn places(texts: &[String]) -> Result<Vec<Place>, String> {
    let manifest = &texts[0];
    let header = manifest
        .find("[workspace.package]\n")
        .ok_or("no [workspace.package] in Cargo.toml")?;
    let section_end = manifest[header + 1..]
        .find("\n[")
        .map_or(manifest.len(), |at| header + 1 + at);
    let lock_entries = CRATES.map(|name| format!("[[package]]\nname = \"{name}\"\nversion = \""));
    let mut searches = vec![
        (
            0,
            "Cargo.toml".to_string(),
            header..section_end,
            "\nversion = \"",
        ),
        (
            2,
            "package.json".to_string(),
            0..texts[2].len(),
            "\n  \"version\": \"",
        ),
        (
            3,
            "crates/refpact/Cargo.toml (refpact-core)".to_string(),
            core_line(&texts[3])?,
            CORE[1],
        ),
    ];
    for (name, entry) in CRATES.iter().zip(&lock_entries) {
        searches.push((1, format!("Cargo.lock ({name})"), 0..texts[1].len(), entry));
    }
    let mut places = Vec::new();
    for (file, label, range, prefix) in searches {
        let (start, end) = value_after(&texts[file][range.clone()], prefix)
            .ok_or_else(|| format!("no version in {label}"))?;
        let (start, end) = (range.start + start, range.start + end);
        places.push(Place {
            label,
            file,
            start,
            end,
        });
    }

    let package: serde_json::Value = serde_json::from_str(&texts[2])
        .map_err(|error| format!("package.json is not JSON: {error}"))?;
    let json = places
        .iter()
        .find(|place| place.file == 2)
        .expect("package.json has a place");
    if package["version"].as_str() != Some(&texts[2][json.start..json.end]) {
        return Err("the version in package.json is not the top-level one".to_string());
    }
    Ok(places)
}

/// The line of the core in the manifest of the crate.
fn core_line(manifest: &str) -> Result<std::ops::Range<usize>, String> {
    let start = manifest
        .find(CORE[0])
        .ok_or("no refpact-core in crates/refpact/Cargo.toml")?
        + 1;
    let end = manifest[start..]
        .find('\n')
        .map_or(manifest.len(), |at| start + at);
    Ok(start..end)
}

/// The start and end of the string value that follows the first `prefix` in `text`.
fn value_after(text: &str, prefix: &str) -> Option<(usize, usize)> {
    let start = text.find(prefix)? + prefix.len();
    let end = start + text[start..].find('"')?;
    Some((start, end))
}

/// The numbers of a version `X.Y.Z`, with no leading zeros.
fn parse(version: &str) -> Option<[u64; 3]> {
    let parts: Vec<&str> = version.split('.').collect();
    let [major, minor, patch] = parts.as_slice() else {
        return None;
    };
    let number = |part: &str| {
        let digits = !part.is_empty() && part.bytes().all(|byte| byte.is_ascii_digit());
        let canonical = part == "0" || !part.starts_with('0');
        if digits && canonical {
            part.parse().ok()
        } else {
            None
        }
    };
    Some([number(major)?, number(minor)?, number(patch)?])
}

#[cfg(test)]
mod tests {
    use super::{core_line, parse, run, value_after, CORE};

    // The crate requires exactly its own version of the core, and every entry of the lock
    // has it: a release of the crate with an older requirement keeps an old, locked core.
    #[test]
    fn the_versions_of_the_repository_agree() {
        run(&["current".to_string()]).unwrap();
    }

    // The requirement is found in any form of the line of the core, and not in another line.
    #[test]
    fn the_requirement_of_the_core_is_found_in_its_line() {
        let found = |manifest: &str| {
            let line = &manifest[core_line(manifest).ok()?];
            value_after(line, CORE[1]).map(|(start, end)| line[start..end].to_string())
        };
        let forms = [
            "[dependencies]\nrefpact-core = { path = \"../refpact-core\", version = \"=1.2.3\" }\n",
            "[dependencies]\nother = { version = \"=9.9.9\" }\nrefpact-core = { version = \"=1.2.3\", default-features = false, path = \"../refpact-core\" }",
        ];
        for manifest in forms {
            assert_eq!(found(manifest).as_deref(), Some("1.2.3"), "{manifest}");
        }
        assert_eq!(found("[dependencies]\nrefpact-core = { path = \"..\" }\nother = { version = \"=9.9.9\" }\n"), None);
        assert_eq!(
            found("[dependencies]\nother = { version = \"=9.9.9\" }\n"),
            None
        );
    }

    #[test]
    fn versions() {
        assert_eq!(parse("0.1.0"), Some([0, 1, 0]));
        assert_eq!(parse("10.20.30"), Some([10, 20, 30]));
        let bad = [
            "", "1", "1.2", "1.2.3.4", "01.2.3", "1.2.x", "1..3", "v1.2.3",
        ];
        for version in bad {
            assert_eq!(parse(version), None, "{version}");
        }
    }
}
