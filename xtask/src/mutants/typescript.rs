//! The TypeScript suite of `cargo xtask mutants`: copies of the package in
//! `packages/.refpact-mutation-<n>`, each with a Vitest process
//! (`xtask/node/vitest-server.mjs`).

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fs;
use std::io::{BufRead, BufReader, PipeReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, Command, Stdio};

use serde_json::{json, Value};

use super::schemata::{self, Change};
use super::{
    copy_tree, output, read, say, slashed, symlink, tail, write, Mutation, Outcome, Suite,
};

pub struct TypeScript {
    root: PathBuf,
    package: PathBuf,
    /// The full name of each test, with its file in the package.
    tests: Vec<(String, String)>,
    /// The copies that were made, to remove at the end.
    made: Vec<PathBuf>,
    /// The Vitest process of the schemata, and what its one run gave.
    schemata_server: Option<Server>,
    by_file: HashMap<String, Vec<String>>,
    schemata_output: String,
}

pub struct Copy {
    path: PathBuf,
    server: Option<Server>,
    /// The file that the last mutation changed back, which Vitest must read again.
    restored: Option<String>,
}

impl AsRef<Path> for Copy {
    fn as_ref(&self) -> &Path {
        &self.path
    }
}

/// The copies of the test files that a mutation runs, in the run of the schemata.
pub struct Fast {
    files: Vec<String>,
}

/// A Vitest process that runs test files on request.
struct Server {
    child: Child,
    stdin: Option<ChildStdin>,
    /// Its output and error output together.
    output: BufReader<PipeReader>,
}

impl TypeScript {
    pub fn new(root: &Path) -> Self {
        TypeScript {
            root: root.to_path_buf(),
            package: root.join("packages").join("refpact"),
            tests: Vec::new(),
            made: Vec::new(),
            schemata_server: None,
            by_file: HashMap::new(),
            schemata_output: String::new(),
        }
    }

    fn make(&mut self, name: &str) -> PathBuf {
        let copy = self
            .root
            .join("packages")
            .join(format!(".refpact-mutation-{name}"));
        let _ = fs::remove_dir_all(&copy);
        copy_tree(&self.package, &copy, &["node_modules", "dist"], false);
        symlink(
            &self.package.join("node_modules"),
            &copy.join("node_modules"),
            true,
        );
        self.made.push(copy.clone());
        copy
    }

    /// The environment of the tests. Their temporary files go into the copy, so that they go
    /// with it: a test file that a mutation breaks while it loads does not remove its own.
    fn environment(command: &mut Command, copy: &Path) {
        let temporary = copy.join(".tmp");
        fs::create_dir_all(&temporary)
            .unwrap_or_else(|error| panic!("cannot make {}: {error}", temporary.display()));
        command.current_dir(copy).env("TMPDIR", temporary);
    }

    /// Runs Vitest once on `files`, or on all test files. With all, it also learns the names
    /// of the tests.
    fn vitest(&mut self, copy: &Path, files: &[String]) -> Outcome {
        let report = copy.join("mutation-report.json");
        let mut command = Command::new(copy.join("node_modules/.bin/vitest"));
        command
            .args(["run", "--reporter=json"])
            .arg(format!("--outputFile={}", report.display()))
            .args(files);
        Self::environment(&mut command, copy);
        let (code, text) = output(&mut command);
        let mut failed = BTreeSet::new();
        if let Ok(json) = fs::read_to_string(&report) {
            let results: Value = serde_json::from_str(&json).unwrap_or_default();
            let results = results["testResults"]
                .as_array()
                .cloned()
                .unwrap_or_default();
            let relative = |file: &Value| {
                let name = Path::new(file["name"].as_str().unwrap_or_default());
                slashed(name.strip_prefix(copy).unwrap_or(name))
            };
            if files.is_empty() {
                // The names of all tests, with the file of each. `vitest list` does not list
                // the tests that a loop makes, such as the conformance cases.
                self.tests = results
                    .iter()
                    .flat_map(|file| {
                        let path = relative(file);
                        file["assertionResults"]
                            .as_array()
                            .into_iter()
                            .flatten()
                            .map(move |test| {
                                let name = test["fullName"].as_str().unwrap_or_default();
                                (name.to_string(), path.clone())
                            })
                    })
                    .collect();
            }
            for file in &results {
                let assertions = file["assertionResults"]
                    .as_array()
                    .cloned()
                    .unwrap_or_default();
                if file["status"] == "failed" && assertions.is_empty() {
                    let name = Path::new(file["name"].as_str().unwrap_or_default());
                    let base = name.file_name().unwrap_or_default().to_string_lossy();
                    failed.insert(base.into_owned());
                }
                for result in &assertions {
                    if result["status"] == "failed" {
                        let name = result["fullName"].as_str().unwrap_or_default();
                        failed.insert(name.to_string());
                    }
                }
            }
            let _ = fs::remove_file(&report);
        }
        Outcome {
            code,
            failed,
            output: text,
        }
    }

    /// The test files that run for a test that a mutation names.
    fn files_of(&self, test: &str) -> Vec<String> {
        if test.ends_with(".test.ts") {
            return vec![format!("tests/{test}")];
        }
        let found: BTreeSet<String> = self
            .tests
            .iter()
            .filter(|(name, _)| name.contains(test) || name.replace(" > ", " ").contains(test))
            .map(|(_, path)| path.clone())
            .collect();
        if found.is_empty() {
            self.tests
                .iter()
                .map(|(_, path)| path.clone())
                .collect::<BTreeSet<_>>()
        } else {
            found
        }
        .into_iter()
        .collect()
    }

    fn start_server(&self, copy: &Path, options: Option<Value>) -> Server {
        let mut command = Command::new("node");
        command.arg(
            self.root
                .join("xtask")
                .join("node")
                .join("vitest-server.mjs"),
        );
        if let Some(options) = options {
            command.arg(options.to_string());
        }
        Self::environment(&mut command, copy);
        let (reader, writer) = std::io::pipe().expect("a pipe");
        let error_writer = writer.try_clone().expect("a pipe");
        command
            .stdin(Stdio::piped())
            .stdout(writer)
            .stderr(error_writer);
        let mut child = command
            .spawn()
            .unwrap_or_else(|error| panic!("cannot start Vitest: {error}"));
        // The command holds the other ends of the pipe; the reader sees its end only without them.
        drop(command);
        Server {
            stdin: child.stdin.take(),
            child,
            output: BufReader::new(reader),
        }
    }

    /// Sends one request to a Vitest process. Gives the outcome, and the failures of each test
    /// file.
    fn ask(server: &mut Server, request: &Value) -> (Outcome, HashMap<String, Vec<String>>) {
        let stdin = server.stdin.as_mut().expect("the process is open");
        writeln!(stdin, "{request}")
            .and_then(|()| stdin.flush())
            .expect("a Vitest process takes requests");
        let mut text = String::new();
        let mut line = String::new();
        loop {
            line.clear();
            let read = server.output.read_line(&mut line).unwrap_or(0);
            if read == 0 {
                panic!("a Vitest process stopped:\n{}", tail(&text, 2000));
            }
            if let Some(answer) = line.strip_prefix("@@result ") {
                let answer: Value = serde_json::from_str(answer).expect("an answer in JSON");
                let failed: BTreeSet<String> = strings(&answer["failed"]).into_iter().collect();
                let by_file = answer["byFile"]
                    .as_object()
                    .into_iter()
                    .flatten()
                    .map(|(file, failures)| (file.clone(), strings(failures)))
                    .collect();
                let code = i32::from(!failed.is_empty());
                return (
                    Outcome {
                        code,
                        failed,
                        output: text,
                    },
                    by_file,
                );
            }
            text.push_str(&line);
        }
    }

    fn stop(server: Option<Server>) {
        if let Some(mut server) = server {
            drop(server.stdin.take());
            let _ = server.child.wait();
        }
    }
}

fn strings(value: &Value) -> Vec<String> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|item| item.as_str().map(str::to_string))
        .collect()
}

/// The offset in UTF-16 code units, as TypeScript counts, of a byte offset into `text`.
fn utf16_of(text: &str, byte: usize) -> usize {
    text[..byte].encode_utf16().count()
}

/// The byte offset into `text` of an offset in UTF-16 code units.
fn byte_of(text: &str, utf16: usize) -> usize {
    let mut units = 0;
    for (byte, c) in text.char_indices() {
        if units >= utf16 {
            return byte;
        }
        units += c.len_utf16();
    }
    text.len()
}

/// The name of an original test file for a failure in its copy for one mutation:
/// `x.m12.test.ts` becomes `x.test.ts`.
fn original_name(failure: &str) -> String {
    if let Some(stem) = failure.strip_suffix(".test.ts") {
        if let Some((before, number)) = stem.rsplit_once(".m") {
            if !number.is_empty() && number.bytes().all(|byte| byte.is_ascii_digit()) {
                return format!("{before}.test.ts");
            }
        }
    }
    failure.to_string()
}

impl Suite for TypeScript {
    type Copy = Copy;
    type Fast = Fast;

    fn name(&self) -> &'static str {
        "ts"
    }

    fn first_copy(&mut self) -> Copy {
        self.more_copy_named("0")
    }

    fn more_copy(&mut self, _first: &Copy, index: usize) -> Copy {
        self.more_copy_named(&index.to_string())
    }

    fn baseline(&mut self, copy: &mut Copy) -> Result<(), String> {
        let outcome = self.vitest(&copy.path, &[]);
        if outcome.code == 0 {
            Ok(())
        } else {
            Err(outcome.output)
        }
    }

    fn matches(&self, test: &str, failure: &str) -> bool {
        failure.contains(test)
    }

    /// Builds the mutations that sit in a block of statements into one copy, each behind a
    /// switch on `globalThis.__refpactMutant`, and runs them all in one Vitest run. Each
    /// mutation gets its own copies of the test files that it names,
    /// `tests/<name>.m<number>.test.ts`; a setup file reads the number from the name of the
    /// file, and turns the cross-checks off again. The copy does not change during the run, so
    /// the workers keep the modules that they have imported (`isolate: false`). The original
    /// test files run in the same run with no mutation: a mutation whose test fails there is
    /// left to `run`.
    fn schemata(&mut self, first: &Copy, chosen: &[Mutation]) -> HashMap<usize, Fast> {
        let copy = self.make("schemata");
        let mut changes: BTreeMap<String, Vec<Change>> = BTreeMap::new();
        for (index, mutation) in chosen.iter().enumerate() {
            let text = read(&first.path.join(&mutation.file));
            if text.matches(mutation.find.as_str()).count() == 1 {
                let start = text.find(mutation.find.as_str()).unwrap_or_default();
                changes
                    .entry(mutation.file.clone())
                    .or_default()
                    .push(Change {
                        number: index + 1,
                        start,
                        end: start + mutation.find.len(),
                        replace: mutation.replace.clone(),
                    });
            }
        }

        // The blocks, from the TypeScript parser, which counts in UTF-16 code units.
        let mut texts = BTreeMap::new();
        let mut request = serde_json::Map::new();
        for (file, file_changes) in &changes {
            let path = slashed(&first.path.join(file));
            let text = read(&first.path.join(file));
            let spans: Vec<Value> = file_changes
                .iter()
                .map(|change| json!([utf16_of(&text, change.start), utf16_of(&text, change.end)]))
                .collect();
            request.insert(path.clone(), Value::Array(spans));
            texts.insert(file.clone(), (path, text));
        }
        let request_file = copy.join("mutation-blocks.json");
        write(&request_file, &Value::Object(request).to_string());
        let parsed = Command::new("node")
            .arg(self.root.join("xtask").join("node").join("ts-blocks.mjs"))
            .arg(&request_file)
            .current_dir(&copy)
            .output()
            .unwrap_or_else(|error| panic!("cannot run node: {error}"));
        let _ = fs::remove_file(&request_file);
        let blocks: Option<Value> = parsed
            .status
            .success()
            .then(|| serde_json::from_slice(&parsed.stdout).ok())
            .flatten();
        let Some(blocks) = blocks else {
            let errors = String::from_utf8_lossy(&parsed.stderr);
            say(&format!(
                "note      ts: the blocks of the mutations were not found; every mutation runs on its own\n{}",
                tail(&errors, 1500)
            ));
            return HashMap::new();
        };

        let mut switched = BTreeSet::new();
        for (file, file_changes) in &changes {
            let (path, text) = &texts[file];
            let file_blocks: Vec<Option<(usize, usize)>> = blocks[path]
                .as_array()
                .into_iter()
                .flatten()
                .map(|block| {
                    let open = block[0].as_u64()? as usize;
                    let close = block[1].as_u64()? as usize;
                    Some((byte_of(text, open), byte_of(text, close)))
                })
                .collect();
            let (bodies, _) = schemata::plan_blocks(&file_blocks, file_changes);
            let (new, _) = schemata::rewrite(text, &bodies, schemata::typescript_branch);
            write(&copy.join(file), &new);
            switched.extend(bodies.values().flatten().map(|change| change.number));
        }

        let setup = copy.join("tests").join("mutant-setup.ts");
        write(
            &setup,
            r#"import { expect } from "vitest";
import { checks } from "../src/frontend/checks.ts";

checks.aliases = false;
checks.walk = false;
const number = /\.m(\d+)\.test\.ts$/.exec(expect.getState().testPath ?? "");
(globalThis as { __refpactMutant?: number }).__refpactMutant = number ? Number(number[1]) : 0;
"#,
        );
        let originals: Vec<String> = self
            .tests
            .iter()
            .map(|(_, path)| path.clone())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let mut files_of_mutant = BTreeMap::new();
        for (index, mutation) in chosen.iter().enumerate() {
            if !switched.contains(&(index + 1)) {
                continue;
            }
            let mut files = Vec::new();
            for original in self.files_of(&mutation.test) {
                let stem = original.strip_suffix(".test.ts").unwrap_or(&original);
                let mutant = format!("{stem}.m{}.test.ts", index + 1);
                fs::copy(copy.join(&original), copy.join(&mutant))
                    .unwrap_or_else(|error| panic!("cannot copy {original}: {error}"));
                files.push(mutant);
            }
            files_of_mutant.insert(index, files);
        }
        let cores = std::thread::available_parallelism().map_or(4, usize::from);
        let options = json!({
            "isolate": false,
            "setupFiles": [slashed(&setup)],
            "maxWorkers": cores,
        });
        let mut server = self.start_server(&copy, Some(options));
        let all: Vec<&String> = originals
            .iter()
            .chain(files_of_mutant.values().flatten())
            .collect();
        let (outcome, by_file) = Self::ask(&mut server, &json!({ "files": all }));
        self.schemata_server = Some(server);
        self.schemata_output = outcome.output;
        let unreliable: BTreeSet<&String> = originals
            .iter()
            .filter_map(|original| by_file.get(original))
            .flatten()
            .collect();
        let mut fast = HashMap::new();
        for (index, files) in files_of_mutant {
            let test = &chosen[index].test;
            if !unreliable.iter().any(|failure| self.matches(test, failure)) {
                fast.insert(index, Fast { files });
            }
        }
        self.by_file = by_file;
        say(&format!(
            "note      ts: {} of {} mutations run as schemata",
            fast.len(),
            chosen.len()
        ));
        fast
    }

    fn run_fast(&self, fast: &Fast) -> Outcome {
        let mut outcome = Outcome::default();
        let mut missing = false;
        for file in &fast.files {
            match self.by_file.get(file) {
                Some(failures) => outcome
                    .failed
                    .extend(failures.iter().map(|failure| original_name(failure))),
                None => missing = true,
            }
        }
        outcome.code = i32::from(missing || !outcome.failed.is_empty());
        if missing {
            outcome.output = self.schemata_output.clone();
        }
        outcome
    }

    fn run(&self, copy: &mut Copy, test: &str, file: &str) -> Outcome {
        if copy.server.is_none() {
            copy.server = Some(self.start_server(&copy.path, None));
        }
        let changed: BTreeSet<&str> = [Some(file), copy.restored.as_deref()]
            .into_iter()
            .flatten()
            .collect();
        let request = json!({ "files": self.files_of(test), "changed": changed });
        let server = copy.server.as_mut().expect("started above");
        let (outcome, _) = Self::ask(server, &request);
        copy.restored = Some(file.to_string());
        outcome
    }

    fn close(&mut self, copies: Vec<Copy>) {
        for copy in copies {
            Self::stop(copy.server);
        }
        Self::stop(self.schemata_server.take());
        for copy in self.made.drain(..) {
            let _ = fs::remove_dir_all(copy);
        }
    }
}

impl TypeScript {
    fn more_copy_named(&mut self, name: &str) -> Copy {
        Copy {
            path: self.make(name),
            server: None,
            restored: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offsets_in_utf16_and_bytes() {
        let text = "a€b😀c";
        // Bytes: a 0, € 1..4, b 4, 😀 5..9, c 9. UTF-16: a 0, € 1, b 2, 😀 3..5, c 5.
        assert_eq!(utf16_of(text, 4), 2);
        assert_eq!(utf16_of(text, 9), 5);
        assert_eq!(byte_of(text, 2), 4);
        assert_eq!(byte_of(text, 5), 9);
        assert_eq!(byte_of(text, 6), text.len());
    }

    #[test]
    fn copies_of_test_files_name_their_original() {
        assert_eq!(original_name("loading.m12.test.ts"), "loading.test.ts");
        assert_eq!(original_name("a.m.test.ts"), "a.m.test.ts");
        assert_eq!(original_name("rules some test"), "rules some test");
    }
}
