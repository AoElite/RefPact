//! Where source text comes from: a directory on disk, or a map in memory for tests.

use std::collections::hash_map::DefaultHasher;
use std::collections::BTreeMap;
use std::hash::{Hash, Hasher};
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};

mod analyzer;
pub(crate) mod json;

pub(crate) use analyzer::{expansions, Expansions};

/// Source files, addressed by `/`-separated paths relative to one base directory.
///
/// Several threads read files at the same time.
pub(crate) trait Sources: Sync {
    fn read(&self, path: &str) -> Option<String>;
    /// All `.rs` files below `dir`, in a stable order.
    fn rust_files(&self, dir: &str) -> Vec<String>;
}

pub(crate) struct Disk {
    pub base: PathBuf,
}

/// The program that makes the index.
pub(crate) const RUST_ANALYZER: &str = "rust-analyzer";

impl Disk {
    /// The index that `rust-analyzer scip` writes for the directory, as bytes. The program
    /// is found as the shell finds it.
    ///
    /// The index is kept in the `target` directory of the code, and is read from there while
    /// the `inputs` and the version of the program stay the same. The inputs are the files
    /// whose text decides the index: the sources and the manifests.
    ///
    /// A rule is checked against what the index says, so the index is kept only where the
    /// code is: who can write there can change the code too. A directory that all users of
    /// a machine share, such as the temporary directory of the system, is not such a place.
    pub fn index(&self, inputs: &[String]) -> Result<Vec<u8>, String> {
        self.index_with(RUST_ANALYZER, inputs)
    }

    fn index_with(&self, program: &str, inputs: &[String]) -> Result<Vec<u8>, String> {
        let base = std::fs::canonicalize(&self.base).unwrap_or_else(|_| self.base.clone());
        let mut place = DefaultHasher::new();
        base.hash(&mut place);
        // The index of one directory has one file: `<directory>-<state>.scip`.
        let own = format!("{:016x}-", place.finish());
        let kept = format!("{own}{:016x}.scip", self.state(program, inputs));
        let cache = self.kept_in();
        if let Ok(bytes) = std::fs::read(cache.join(&kept)) {
            return Ok(bytes);
        }

        static NUMBER: AtomicU32 = AtomicU32::new(0);
        let name = format!(
            "{}-{}.part",
            std::process::id(),
            NUMBER.fetch_add(1, Ordering::Relaxed)
        );
        // Without the directory, the index is made and not kept.
        let made = std::fs::create_dir_all(&cache).is_ok();
        let output = match made {
            true => cache.join(name),
            false => std::env::temp_dir().join(name),
        };
        // The program writes to a file that this process made. When something is already at
        // the name, such as a link to another file, nothing is written.
        let mut new = std::fs::OpenOptions::new();
        if let Err(error) = new.write(true).create_new(true).open(&output) {
            return Err(format!(
                "the file for the index, {}, cannot be made ({error})",
                output.display()
            ));
        }
        let run = std::process::Command::new(program)
            .arg("scip")
            .arg(&self.base)
            .arg("--output")
            .arg(&output)
            .output();
        let bytes = std::fs::read(&output);
        let run = run.map_err(|error| {
            let _ = std::fs::remove_file(&output);
            format!(
                "`{program}` cannot be started ({error}); install it with \
                 `rustup component add rust-analyzer`"
            )
        });
        let failed = match &run {
            Ok(run) if run.status.success() => None,
            Ok(run) => {
                let said = String::from_utf8_lossy(&run.stderr);
                let last = said.lines().rev().find(|line| !line.trim().is_empty());
                Some(format!(
                    "`{program} scip` failed: {}",
                    last.unwrap_or("it gave no message")
                ))
            }
            Err(message) => Some(message.clone()),
        };
        if let Some(message) = failed {
            // The file is not there when the program did not run.
            let _ = std::fs::remove_file(&output);
            return Err(message);
        }
        let bytes = bytes.map_err(|error| format!("`{program} scip` wrote no index ({error})"))?;
        // The index of an older state of the directory is of no use.
        for entry in std::fs::read_dir(&cache).into_iter().flatten().flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&own) && name.ends_with(".scip") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        // Another process can make the same index at the same time. A file is in its place
        // only when it is complete.
        if !made || std::fs::rename(&output, cache.join(&kept)).is_err() {
            let _ = std::fs::remove_file(&output);
        }
        Ok(bytes)
    }

    /// Where the index of the directory is kept: in the directory of the build, which is
    /// `CARGO_TARGET_DIR` when that is set, and `target` in the directory otherwise.
    pub fn kept_in(&self) -> PathBuf {
        let target = match std::env::var_os("CARGO_TARGET_DIR") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => self.base.join("target"),
        };
        target.join("refpact-index")
    }

    /// A number that changes when the index can change: with the text of an input, and
    /// with the version of the program.
    pub fn state(&self, program: &str, inputs: &[String]) -> u64 {
        let mut state = DefaultHasher::new();
        let version = std::process::Command::new(program)
            .arg("--version")
            .output();
        version.map(|run| run.stdout).ok().hash(&mut state);
        for input in inputs {
            input.hash(&mut state);
            std::fs::read(self.base.join(input)).ok().hash(&mut state);
        }
        state.finish()
    }
}

impl Sources for Disk {
    fn read(&self, path: &str) -> Option<String> {
        std::fs::read_to_string(self.base.join(path)).ok()
    }

    fn rust_files(&self, dir: &str) -> Vec<String> {
        let mut found = Vec::new();
        let mut pending = vec![dir.to_owned()];
        while let Some(current) = pending.pop() {
            let Ok(entries) = std::fs::read_dir(self.base.join(&current)) else {
                continue;
            };
            for entry in entries.flatten() {
                let name = entry.file_name().to_string_lossy().into_owned();
                let path = join(&current, &name);
                match entry.file_type() {
                    Ok(kind) if kind.is_dir() => pending.push(path),
                    Ok(kind) if kind.is_file() && name.ends_with(".rs") => found.push(path),
                    _ => {}
                }
            }
        }
        found.sort();
        found
    }
}

pub(crate) struct Memory {
    pub files: BTreeMap<String, String>,
}

impl Sources for Memory {
    fn read(&self, path: &str) -> Option<String> {
        self.files.get(path).cloned()
    }

    fn rust_files(&self, dir: &str) -> Vec<String> {
        let prefix = if dir.is_empty() {
            String::new()
        } else {
            format!("{dir}/")
        };
        self.files
            .keys()
            .filter(|path| path.starts_with(&prefix) && path.ends_with(".rs"))
            .cloned()
            .collect()
    }
}

/// Joins two relative paths and removes `.` and `..` segments.
pub(crate) fn join(dir: &str, rest: &str) -> String {
    let mut parts: Vec<&str> = Vec::new();
    for part in dir.split('/').chain(rest.split('/')) {
        match part {
            "" | "." => {}
            ".." => {
                if matches!(parts.last(), Some(last) if *last != "..") {
                    parts.pop();
                } else {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// The directory part of a relative path. Empty for a file in the base directory.
pub(crate) fn parent(path: &str) -> &str {
    path.rsplit_once('/').map_or("", |(dir, _)| dir)
}

/// The file name of a relative path.
pub(crate) fn file_name(path: &str) -> &str {
    path.rsplit_once('/').map_or(path, |(_, name)| name)
}

/// What `rustc --print cfg` writes for a target, or for the target of this machine: one
/// option of the target on each line.
pub(crate) fn target_options(target: Option<&str>) -> Result<String, String> {
    let program = std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let mut command = std::process::Command::new(&program);
    command.args(["--print", "cfg"]);
    if let Some(target) = target {
        command.args(["--target", target]);
    }
    let shown = program.to_string_lossy();
    let run = command
        .output()
        .map_err(|error| format!("`{shown}` cannot be started ({error})"))?;
    if !run.status.success() {
        let said = String::from_utf8_lossy(&run.stderr);
        let first = said.lines().find(|line| !line.trim().is_empty());
        return Err(format!(
            "`{shown} --print cfg` failed: {}",
            first.unwrap_or("it gave no message")
        ));
    }
    String::from_utf8(run.stdout).map_err(|_| format!("`{shown} --print cfg` wrote no text"))
}

/// A path of the file system without links. Cargo names files so; a link in the path of a
/// load must not make a file of the load seem to be outside it.
pub(crate) fn real(path: &std::path::Path) -> PathBuf {
    std::fs::canonicalize(path).unwrap_or_else(|_| path.to_owned())
}

/// What `cargo metadata` writes for the workspace of a directory: its packages, and what
/// the arguments ask for.
pub(crate) fn metadata(disk: &Disk, arguments: &[&str]) -> Result<String, String> {
    let program = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let shown = program.to_string_lossy();
    let run = std::process::Command::new(&program)
        .args(["metadata", "--format-version", "1"])
        .args(arguments)
        .current_dir(&disk.base)
        .output()
        .map_err(|error| format!("`{shown}` cannot be started ({error})"))?;
    if !run.status.success() {
        let said = String::from_utf8_lossy(&run.stderr);
        let first = said.lines().find(|line| !line.trim().is_empty());
        return Err(format!(
            "`{shown} metadata` failed: {}",
            first.unwrap_or("it gave no message")
        ));
    }
    String::from_utf8(run.stdout).map_err(|_| format!("`{shown} metadata` wrote no text"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn join_normalises() {
        assert_eq!(join("src", "a.rs"), "src/a.rs");
        assert_eq!(join("src/a", "../b.rs"), "src/b.rs");
        assert_eq!(join("", "src/./x.rs"), "src/x.rs");
        assert_eq!(join("src", "../../x.rs"), "../x.rs");
        assert_eq!(parent("src/a/b.rs"), "src/a");
        assert_eq!(parent("lib.rs"), "");
        assert_eq!(file_name("src/a/b.rs"), "b.rs");
    }

    /// Needs rust-analyzer: `rustup component add rust-analyzer`.
    #[test]
    fn an_index_is_made_again_only_when_an_input_changes() {
        let base = std::env::temp_dir().join(format!("refpact-kept-{}", std::process::id()));
        std::fs::create_dir_all(base.join("src")).unwrap();
        let manifest = "[package]\nname = \"kept\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
        std::fs::write(base.join("Cargo.toml"), manifest).unwrap();
        std::fs::write(base.join("src/lib.rs"), "pub fn one() {}\n").unwrap();
        let disk = Disk { base: base.clone() };
        let inputs = ["Cargo.toml".to_owned(), "src/lib.rs".to_owned()];
        // A program that is not there cannot make an index.
        let missing = "refpact-no-such-program";
        assert!(disk.index_with(missing, &inputs).is_err());

        let first = disk.index(&inputs).unwrap();
        // The files that are kept for this directory.
        let dir = disk.kept_in();
        let kept = || {
            let mut place = DefaultHasher::new();
            std::fs::canonicalize(&base).unwrap().hash(&mut place);
            let own = format!("{:016x}-", place.finish());
            let files = std::fs::read_dir(&dir).unwrap().flatten();
            files
                .map(|entry| entry.file_name().to_string_lossy().into_owned())
                .filter(|name| name.starts_with(&own))
                .collect::<Vec<_>>()
        };
        let before = kept();
        assert_eq!(before.len(), 1);
        // The second index is read, not made: it is what the file has.
        std::fs::write(dir.join(&before[0]), b"kept").unwrap();
        assert_eq!(disk.index(&inputs).unwrap(), b"kept");

        // A change to an input gives a new index, and the old one is removed.
        std::fs::write(
            base.join("src/lib.rs"),
            "pub fn one() {}\npub fn two() {}\n",
        )
        .unwrap();
        let second = disk.index(&inputs).unwrap();
        assert!(second.len() > first.len());
        let after = kept();
        assert_eq!(after.len(), 1);
        assert_ne!(after, before);

        std::fs::remove_file(dir.join(&after[0])).unwrap();
        std::fs::remove_dir_all(&base).unwrap();
    }
}
