//! The baseline file. This module and that of the diagram file are the only parts of this
//! crate that write files.

use std::path::PathBuf;
use std::sync::Mutex;

use crate::core::{Applied, BaselineEntry, PathFormat, Violation};

use crate::Failure;

/// A file of violations that the project accepts for now.
///
/// A rule uses a baseline only when it is checked with
/// [`check_with_baseline`](crate::Rule::check_with_baseline). Plain
/// [`check`](crate::Rule::check) never reads one.
///
/// The file is text, one accepted violation per line, without line numbers of the source.
/// Commit it. To write it, run the tests once with `REFPACT_BASELINE=update`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Baseline {
    path: PathBuf,
}

/// Tests of one binary run on several threads, and several rules can share one file.
static WRITING: Mutex<()> = Mutex::new(());

/// Held while a file is written, so that two tests do not write one file at the same time.
pub(crate) fn writing() -> std::sync::MutexGuard<'static, ()> {
    WRITING
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Whether the environment variable asks the checks to write their files.
pub(crate) fn update_requested(variable: &str) -> bool {
    std::env::var(variable).is_ok_and(|value| value == "update")
}

impl Baseline {
    /// The baseline in the file at `path`. A file that does not exist is an empty baseline.
    ///
    /// In a test, use `concat!(env!("CARGO_MANIFEST_DIR"), "/tests/architecture.baseline")`.
    pub fn file(path: impl Into<PathBuf>) -> Baseline {
        Baseline { path: path.into() }
    }

    pub(crate) fn display(&self) -> String {
        self.path.display().to_string()
    }

    fn read(&self) -> Result<Vec<BaselineEntry>, Failure> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(error) => {
                return Err(Failure::Baseline {
                    message: format!("cannot read {}: {error}", self.display()),
                })
            }
        };
        crate::core::parse_baseline(&text).map_err(|error| Failure::Baseline {
            message: format!(
                "{}:{}: this line of the baseline is not valid",
                self.display(),
                error.line
            ),
        })
    }

    pub(crate) fn apply(&self, rule: &str, violations: Vec<Violation>) -> Result<Applied, Failure> {
        Ok(crate::core::apply_baseline(
            rule,
            violations,
            &self.read()?,
            &PathFormat::plain("::"),
        ))
    }

    /// Replaces the entries of one rule with its current violations.
    pub(crate) fn write(&self, rule: &str, violations: &[Violation]) -> Result<(), Failure> {
        let _guard = writing();
        let mut entries = self.read()?;
        entries.retain(|entry| entry.rule != rule);
        entries.extend(crate::core::baseline_entries(
            violations,
            &PathFormat::plain("::"),
        ));
        write_file(&self.path, &crate::core::format_baseline(&entries)).map_err(|error| {
            Failure::Baseline {
                message: format!("cannot write {}: {error}", self.display()),
            }
        })
    }
}

/// Writes a file, and the directory of the file when it is not there.
pub(crate) fn write_file(path: &std::path::Path, text: &str) -> std::io::Result<()> {
    if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
        std::fs::create_dir_all(dir)?;
    }
    // Written beside the file and moved, so that a reader never sees half a file.
    let mut partial = path.to_path_buf().into_os_string();
    partial.push(".partial");
    std::fs::write(&partial, text)?;
    std::fs::rename(&partial, path)
}
