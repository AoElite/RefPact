//! The Rust suite of `cargo xtask mutants`: copies of the workspace in `target/mutation-*`.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use serde_json::Value;

use super::schemata::{self, Span};
use super::{
    copy_tree, failed_rust_tests, output, output_within, read, say, slashed, tail, write, Mutation,
    Outcome, Suite,
};

/// What a copy of the workspace needs: the sources of the crates and what their tests read.
/// The copies hold `xtask` because the workspace names it, but they do not build or test it
/// (`--exclude xtask`): it has no detections.
const ENTRIES: [&str; 8] = [
    "Cargo.toml",
    "Cargo.lock",
    "README.md",
    "crates",
    "xtask",
    "conformance",
    "docs",
    "fixtures",
];

/// The longest that one test binary may run for one mutation.
const LIMIT: Duration = Duration::from_secs(120);

pub struct Rust {
    root: PathBuf,
    work: PathBuf,
    /// For the name of each test, the crates and test targets that have it, as the
    /// arguments of `cargo test` that select the target.
    targets: HashMap<String, BTreeSet<(String, Vec<String>)>>,
    /// The test binaries of the schemata.
    binaries: Vec<Binary>,
}

/// A test binary that `cargo test --no-run` built.
struct Binary {
    executable: PathBuf,
    /// The directory of its package.
    directory: PathBuf,
    package: String,
    /// The arguments of `cargo test` that select its target.
    selector: Vec<String>,
    tests: Vec<String>,
}

pub struct Fast {
    number: usize,
    test: String,
}

impl Rust {
    pub fn new(root: &Path) -> Self {
        // In the build folder of the repository and not in the temporary folder, which is
        // often small.
        let work = root
            .join("target")
            .join(format!("mutation-{}", std::process::id()));
        let _ = fs::remove_dir_all(&work);
        fs::create_dir_all(&work)
            .unwrap_or_else(|error| panic!("cannot make {}: {error}", work.display()));
        Rust {
            root: root.to_path_buf(),
            work,
            targets: HashMap::new(),
            binaries: Vec::new(),
        }
    }

    fn cargo(copy: &Path, args: &[&str]) -> Command {
        let mut command = Command::new("cargo");
        command
            .args(args)
            .current_dir(copy)
            .env("CARGO_TARGET_DIR", copy.join("target"))
            .env("CARGO_TERM_COLOR", "never");
        command
    }

    /// The test binaries that `cargo test --no-run` built, from its JSON messages, each with
    /// its tests.
    fn binaries(messages: &[Value]) -> Vec<Binary> {
        let built = messages.iter().filter(|message| {
            message["reason"] == "compiler-artifact" && message["executable"].is_string()
        });
        built
            .map(|message| {
                let manifest = Path::new(message["manifest_path"].as_str().unwrap_or_default());
                let directory = manifest.parent().unwrap_or(Path::new(".")).to_path_buf();
                let package = directory
                    .file_name()
                    .map(|name| name.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let target = &message["target"];
                let kinds = target["kind"].as_array().cloned().unwrap_or_default();
                let name = target["name"].as_str().unwrap_or_default().to_string();
                let selector = if kinds.iter().any(|kind| kind == "lib") {
                    vec!["--lib".to_string()]
                } else if kinds.iter().any(|kind| kind == "bin") {
                    vec!["--bin".to_string(), name]
                } else {
                    vec!["--test".to_string(), name]
                };
                let executable = PathBuf::from(message["executable"].as_str().unwrap_or_default());
                let tests = Self::tests_of(&executable, &directory);
                Binary {
                    executable,
                    directory,
                    package,
                    selector,
                    tests,
                }
            })
            .collect()
    }

    /// The tests of a test binary.
    fn tests_of(executable: &Path, directory: &Path) -> Vec<String> {
        let (_, listing) = output(
            Command::new(executable)
                .args(["--list", "--format", "terse"])
                .current_dir(directory),
        );
        listing
            .lines()
            .filter_map(|entry| entry.strip_suffix(": test"))
            .map(str::to_string)
            .collect()
    }

    /// Runs the test binaries of the schemata with mutation `number` switched on: those that
    /// have `test`, or all of them.
    fn run_binaries(&self, number: usize, test: Option<&str>) -> Outcome {
        let chosen: Vec<&Binary> = self
            .binaries
            .iter()
            .filter(|binary| {
                test.is_none_or(|test| binary.tests.iter().any(|name| self.matches(test, name)))
            })
            .collect();
        let filter = test.filter(|_| !chosen.is_empty());
        let binaries = if chosen.is_empty() {
            self.binaries.iter().collect()
        } else {
            chosen
        };
        let mut outcome = Outcome::default();
        for binary in binaries {
            let mut command = Command::new(&binary.executable);
            command
                .args(filter)
                .current_dir(&binary.directory)
                .env("REFPACT_MUTANT", number.to_string())
                .env("CARGO_TERM_COLOR", "never")
                .env("CARGO_MANIFEST_DIR", &binary.directory);
            match output_within(&mut command, LIMIT) {
                Some((code, stdout, stderr)) => {
                    if code != 0 {
                        outcome.code = code;
                    }
                    outcome.failed.extend(failed_rust_tests(&stdout));
                    outcome.output.push_str(&stdout);
                    outcome.output.push_str(&stderr);
                }
                None => {
                    outcome.code = 1;
                    outcome.output.push_str("timed out\n");
                    outcome.failed.insert(match test {
                        Some(test) => format!("{test} (timed out)"),
                        None => "timed out".to_string(),
                    });
                }
            }
        }
        outcome
    }

    /// The JSON messages of a build of the tests, and whether it succeeded.
    fn build_tests(copy: &Path) -> (bool, Vec<Value>, String) {
        let (code, text) = output(&mut Self::cargo(
            copy,
            &[
                "test",
                "--workspace",
                "--exclude",
                "xtask",
                "--offline",
                "--no-run",
                "--message-format=json",
            ],
        ));
        let messages = text
            .lines()
            .filter(|line| line.starts_with('{'))
            .filter_map(|line| serde_json::from_str(line).ok())
            .collect();
        (code == 0, messages, text)
    }

    /// The numbers of the mutations whose schemata an error of the build points into.
    fn blamed(
        copy: &Path,
        messages: &[Value],
        spans: &BTreeMap<String, Vec<Span>>,
    ) -> BTreeSet<usize> {
        let mut blamed = BTreeSet::new();
        for message in messages {
            let diagnostic = &message["message"];
            if message["reason"] != "compiler-message" || diagnostic["level"] != "error" {
                continue;
            }
            for span in diagnostic["spans"].as_array().into_iter().flatten() {
                let name = Path::new(span["file_name"].as_str().unwrap_or_default());
                let file = slashed(name.strip_prefix(copy).unwrap_or(name));
                let text = fs::read_to_string(copy.join(&file)).unwrap_or_default();
                let line = span["line_start"].as_u64().unwrap_or(1) as usize;
                let offset: usize = text
                    .split('\n')
                    .take(line - 1)
                    .map(|line| line.len() + 1)
                    .sum();
                for (start, end, numbers) in spans.get(&file).into_iter().flatten() {
                    if *start <= offset && offset <= *end {
                        blamed.extend(numbers);
                    }
                }
            }
        }
        blamed
    }
}

impl Suite for Rust {
    type Copy = PathBuf;
    type Fast = Fast;

    fn name(&self) -> &'static str {
        "rust"
    }

    fn first_copy(&mut self) -> PathBuf {
        let copy = self.work.join("0");
        fs::create_dir_all(&copy)
            .unwrap_or_else(|error| panic!("cannot make {}: {error}", copy.display()));
        for entry in ENTRIES {
            let source = self.root.join(entry);
            if source.is_dir() {
                copy_tree(
                    &source,
                    &copy.join(entry),
                    &["target", "node_modules"],
                    false,
                );
            } else {
                fs::copy(&source, copy.join(entry))
                    .unwrap_or_else(|error| panic!("cannot copy {}: {error}", source.display()));
            }
        }
        // The dependencies that the repository has built already, so that the first build
        // compiles only the crates of the workspace.
        let built = self.root.join("target").join("debug");
        for part in ["deps", "build", ".fingerprint"] {
            if built.join(part).is_dir() {
                let target = copy.join("target").join("debug").join(part);
                copy_tree(&built.join(part), &target, &[], true);
            }
        }
        copy
    }

    fn more_copy(&mut self, first: &PathBuf, index: usize) -> PathBuf {
        // The build is copied too, so that each copy builds only the crates of the workspace.
        let copy = self.work.join(index.to_string());
        copy_tree(first, &copy, &["incremental"], true);
        copy
    }

    fn baseline(&mut self, copy: &mut PathBuf) -> Result<(), String> {
        let (code, text) = output(&mut Self::cargo(
            copy,
            &[
                "test",
                "--workspace",
                "--exclude",
                "xtask",
                "--offline",
                "--no-fail-fast",
            ],
        ));
        if code != 0 {
            return Err(text);
        }
        // Which test target has each test.
        let (_, messages, _) = Self::build_tests(copy);
        for binary in Self::binaries(&messages) {
            for test in binary.tests {
                self.targets
                    .entry(test)
                    .or_default()
                    .insert((binary.package.clone(), binary.selector.clone()));
            }
        }
        Ok(())
    }

    fn matches(&self, test: &str, failure: &str) -> bool {
        test == failure || failure.ends_with(&format!("::{test}"))
    }

    fn schemata(&mut self, first: &PathBuf, chosen: &[Mutation]) -> HashMap<usize, Fast> {
        let copy = self.work.join("schemata");
        copy_tree(first, &copy, &["incremental"], true);
        let mut by_file: BTreeMap<String, Vec<(usize, String, String)>> = BTreeMap::new();
        let mut number_of = BTreeMap::new();
        for (index, mutation) in chosen.iter().enumerate() {
            let text = read(&first.join(&mutation.file));
            if text.matches(mutation.find.as_str()).count() == 1 {
                number_of.insert(index, index + 1);
                by_file.entry(mutation.file.clone()).or_default().push((
                    index + 1,
                    mutation.find.clone(),
                    mutation.replace.clone(),
                ));
            }
        }

        let mut excluded = BTreeSet::new();
        let mut executables = None;
        for _ in 0..6 {
            let mut spans = BTreeMap::new();
            let mut crates = BTreeSet::new();
            for (file, mutations) in &by_file {
                let original = read(&first.join(file));
                let active: Vec<_> = mutations
                    .iter()
                    .filter(|(number, ..)| !excluded.contains(number))
                    .cloned()
                    .collect();
                let (bodies, outside) = schemata::plan(&original, &active);
                excluded.extend(outside);
                let (text, file_spans) =
                    schemata::rewrite(&original, &bodies, schemata::rust_branch);
                write(&copy.join(file), &text);
                spans.insert(file.clone(), file_spans);
                if !bodies.is_empty() {
                    crates.insert(file.split("/src/").next().unwrap_or_default().to_string());
                }
            }
            for krate in &crates {
                write(&copy.join(krate).join("mutant.rs"), schemata::MACRO);
                let lib_file = format!("{krate}/src/lib.rs");
                let current = if by_file.contains_key(&lib_file) {
                    read(&copy.join(&lib_file))
                } else {
                    read(&first.join(&lib_file))
                };
                write(&copy.join(&lib_file), &schemata::include_macro(&current));
            }
            let (built, messages, text) = Self::build_tests(&copy);
            if built {
                executables = Some(messages);
                break;
            }
            // The mutations in a body that an error points into are built alone, each in a
            // copy of its own.
            let blamed = Self::blamed(&copy, &messages, &spans);
            if blamed.is_empty() {
                say(&format!(
                    "note      rust: the schemata do not build; every mutation is built on its own\n{}",
                    tail(&text, 1500)
                ));
                return HashMap::new();
            }
            excluded.extend(blamed);
        }
        let Some(messages) = executables else {
            return HashMap::new();
        };

        // Which binary has each test, and the tests that fail with no mutation: a mutation
        // that names one of them is built on its own.
        self.binaries = Self::binaries(&messages);
        let unreliable = self.run_binaries(0, None).failed;
        let mut fast = HashMap::new();
        for (index, number) in number_of {
            let test = &chosen[index].test;
            let fails_already = unreliable.iter().any(|failure| self.matches(test, failure));
            if !excluded.contains(&number) && !fails_already {
                let test = test.clone();
                fast.insert(index, Fast { number, test });
            }
        }
        say(&format!(
            "note      rust: {} of {} mutations run as schemata",
            fast.len(),
            chosen.len()
        ));
        fast
    }

    fn run_fast(&self, fast: &Fast) -> Outcome {
        self.run_binaries(fast.number, Some(&fast.test))
    }

    fn run(&self, copy: &mut PathBuf, test: &str, _file: &str) -> Outcome {
        let mut targets = BTreeSet::new();
        for (name, places) in &self.targets {
            if self.matches(test, name) {
                targets.extend(places.iter().cloned());
            }
        }
        let mut commands: Vec<Command> = targets
            .iter()
            .map(|(package, selector)| {
                let mut args = vec!["test", "--offline", "--no-fail-fast", "-p", package];
                args.extend(selector.iter().map(String::as_str));
                args.extend(["--", test]);
                Self::cargo(copy, &args)
            })
            .collect();
        if commands.is_empty() {
            commands.push(Self::cargo(
                copy,
                &[
                    "test",
                    "--workspace",
                    "--exclude",
                    "xtask",
                    "--offline",
                    "--no-fail-fast",
                ],
            ));
        }
        let mut outcome = Outcome::default();
        for command in &mut commands {
            let (code, text) = output(command);
            outcome.code |= code;
            outcome.failed.extend(failed_rust_tests(&text));
            outcome.output.push_str(&text);
        }
        outcome
    }

    fn close(&mut self, _copies: Vec<PathBuf>) {
        let _ = fs::remove_dir_all(&self.work);
    }
}
