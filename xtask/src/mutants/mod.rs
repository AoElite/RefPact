//! `cargo xtask mutants`: checks that every detection has a test that fails without it.
//!
//! ```text
//! cargo xtask mutants [rust|ts|all] [filter]
//! ```
//!
//! For each mutation in `list.rs`, the task changes one piece of code in a copy of the
//! sources so that one detection is gone, runs the test that the mutation names, and requires
//! that this test fails. A mutation that no test notices is a detection that is not proven.
//!
//! Only the named test runs, in the test target that has it: one `cargo test -p <crate>
//! --test <file> -- <name>` for Rust, one Vitest file for TypeScript. Vitest starts once for
//! each copy (`xtask/node/vitest-server.mjs`) and reads only the mutated file again.
//!
//! Most mutations do not run one by one. `schemata.rs` builds them into one copy, each behind
//! a switch: for Rust the environment variable `REFPACT_MUTANT`, for TypeScript
//! `globalThis.__refpactMutant` in its `{ }` block (found with `xtask/node/ts-blocks.mjs`).
//! The Rust test binaries then run once for each mutation, and the TypeScript mutations all run
//! in one Vitest run. A mutation that cannot be switched (it is not in a function body or
//! block, or its function does not compile as a schema, or its test fails already with no
//! mutation) runs in a copy of its own. Several copies of the sources run mutations at the
//! same time, and the Rust and TypeScript suites run side by side.
//!
//! The Rust copies are in `target/mutation-*`. The TypeScript copies are
//! `packages/.refpact-mutation-<n>`, beside the package so that they find the same
//! `node_modules` and fixtures. Run `pnpm install` first; the `rust` suite does not need Node.
//!
//! The filter keeps the mutations whose name or file contains it, for example `imports.ts`.
//! `MUTATION_JOBS` sets the number of copies of each suite (default: a third of the cores).

mod list;
mod rust;
mod schemata;
mod typescript;

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

pub const USAGE: &str = "cargo xtask mutants [rust|ts|all] [filter]";

/// A change that removes one detection, and the test that must then fail.
pub struct Mutation {
    pub name: String,
    /// The file, from the root of the repository for Rust and of the package for TypeScript.
    pub file: String,
    /// A text that occurs exactly once in the file.
    pub find: String,
    pub replace: String,
    /// For Rust the name of a test function; for TypeScript a part of the full name of a
    /// test, or the name of a test file.
    pub test: String,
}

/// The result of a run of tests.
#[derive(Default)]
pub struct Outcome {
    /// Not zero when a test failed or the run did not finish.
    pub code: i32,
    pub failed: BTreeSet<String>,
    pub output: String,
}

/// The tests of one language.
trait Suite: Sync {
    /// A copy of the sources that one mutation at a time can change.
    type Copy: Send + AsRef<Path>;
    /// What a mutation that runs as a schema needs to give its result.
    type Fast: Send + Sync;

    fn name(&self) -> &'static str;
    fn first_copy(&mut self) -> Self::Copy;
    fn more_copy(&mut self, first: &Self::Copy, index: usize) -> Self::Copy;
    /// Runs all tests with no mutation. Gives the output when they fail.
    fn baseline(&mut self, copy: &mut Self::Copy) -> Result<(), String>;
    /// Whether the failed test `failure` is the test that a mutation names.
    fn matches(&self, test: &str, failure: &str) -> bool;
    /// Builds the mutations that can be switched at run time into one copy. Gives the ones
    /// that run like that, by their index in `chosen`.
    fn schemata(&mut self, first: &Self::Copy, chosen: &[Mutation]) -> HashMap<usize, Self::Fast>;
    fn run_fast(&self, fast: &Self::Fast) -> Outcome;
    /// Runs the test `test` in a copy where `file` has a mutation.
    fn run(&self, copy: &mut Self::Copy, test: &str, file: &str) -> Outcome;
    fn close(&mut self, copies: Vec<Self::Copy>);
}

pub fn run(args: &[String]) -> Result<(), String> {
    let suite = args.first().map_or("all", String::as_str);
    let filter = args.get(1).map_or("", String::as_str);
    if args.len() > 2 || !matches!(suite, "rust" | "ts" | "all") {
        return Err(format!("usage: {USAGE}"));
    }
    let cores = thread::available_parallelism().map_or(6, usize::from);
    let jobs = match std::env::var("MUTATION_JOBS") {
        Ok(value) => value
            .parse()
            .map_err(|_| format!("MUTATION_JOBS is not a number: {value}"))?,
        Err(_) => (cores / 3).max(2),
    };
    let root = crate::root();

    let survived = thread::scope(|scope| {
        let rust = (suite != "ts").then(|| {
            scope.spawn(|| check(rust::Rust::new(&root), list::rust(), filter, jobs, cores))
        });
        let typescript = (suite != "rust").then(|| {
            scope.spawn(|| {
                let suite = typescript::TypeScript::new(&root);
                check(suite, list::typescript(), filter, jobs, cores)
            })
        });
        let mut survived = Vec::new();
        for handle in [rust, typescript].into_iter().flatten() {
            survived.extend(handle.join().expect("a suite panicked"));
        }
        survived
    });
    if !survived.is_empty() {
        return Err(format!(
            "{} mutation(s) not caught: {}",
            survived.len(),
            survived.join(", ")
        ));
    }
    println!("every mutation is caught");
    Ok(())
}

/// Runs the mutations of one suite. Gives the names of those that no test caught.
fn check<S: Suite>(
    mut suite: S,
    mutations: Vec<Mutation>,
    filter: &str,
    jobs: usize,
    cores: usize,
) -> Vec<String> {
    let chosen: Vec<Mutation> = mutations
        .into_iter()
        .filter(|mutation| mutation.name.contains(filter) || mutation.file.contains(filter))
        .collect();
    if chosen.is_empty() {
        return Vec::new();
    }
    let mut first = suite.first_copy();
    if let Err(output) = suite.baseline(&mut first) {
        say(tail(&output, 3000));
        suite.close(vec![first]);
        return vec![format!(
            "{}: the tests fail before any mutation",
            suite.name()
        )];
    }
    let fast = suite.schemata(&first, &chosen);
    let mut copies = vec![first];
    for index in 1..jobs.min(chosen.len() - fast.len()) {
        let copy = suite.more_copy(&copies[0], index);
        copies.push(copy);
    }

    let pool = Pool::new(copies);
    let next = AtomicUsize::new(0);
    let survived = Mutex::new(Vec::new());
    // The schemata only run tests, so more of them can run at once.
    let workers = if fast.is_empty() { jobs } else { cores };
    let suite_ref = &suite;
    thread::scope(|scope| {
        for _ in 0..workers {
            scope.spawn(|| loop {
                let index = next.fetch_add(1, Ordering::SeqCst);
                let Some(mutation) = chosen.get(index) else {
                    break;
                };
                let caught = match fast.get(&index) {
                    Some(fast) => verdict(suite_ref, mutation, suite_ref.run_fast(fast)),
                    None => run_in_copy(suite_ref, &pool, mutation),
                };
                if !caught {
                    survived
                        .lock()
                        .unwrap()
                        .push((index, mutation.name.clone()));
                }
            });
        }
    });
    suite.close(pool.into_inner());
    let mut survived = survived.into_inner().unwrap();
    survived.sort();
    survived.into_iter().map(|(_, name)| name).collect()
}

/// Runs one mutation in a free copy of the sources. Gives whether its test caught it.
fn run_in_copy<S: Suite>(suite: &S, pool: &Pool<S::Copy>, mutation: &Mutation) -> bool {
    let mut copy = pool.take();
    let path = copy.as_ref().join(&mutation.file);
    let original = read(&path);
    let count = original.matches(mutation.find.as_str()).count();
    if count != 1 {
        pool.give(copy);
        say(&format!(
            "STALE     {}: {}: the text occurs {count} times in {}",
            suite.name(),
            mutation.name,
            mutation.file
        ));
        return false;
    }
    write(
        &path,
        &original.replacen(&mutation.find, &mutation.replace, 1),
    );
    let outcome = suite.run(&mut copy, &mutation.test, &mutation.file);
    write(&path, &original);
    pool.give(copy);
    verdict(suite, mutation, outcome)
}

fn verdict<S: Suite>(suite: &S, mutation: &Mutation, outcome: Outcome) -> bool {
    let (name, test) = (suite.name(), &mutation.test);
    let message = if outcome.code == 0 {
        format!(
            "SURVIVED  {name}: {}: the test passes without it",
            mutation.name
        )
    } else if outcome.failed.is_empty() {
        format!(
            "BROKEN    {name}: {}: no test ran\n{}",
            mutation.name,
            tail(&outcome.output, 1500)
        )
    } else if !outcome
        .failed
        .iter()
        .any(|failure| suite.matches(test, failure))
    {
        let failed: Vec<&String> = outcome.failed.iter().take(8).collect();
        format!(
            "WRONG     {name}: {}: expected `{test}` to fail; failed: {failed:?}",
            mutation.name
        )
    } else {
        say(&format!("caught    {name}: {}  ({test})", mutation.name));
        return true;
    };
    say(&message);
    false
}

/// The copies of the sources that are free.
struct Pool<C> {
    free: Mutex<Vec<C>>,
    returned: Condvar,
    count: usize,
}

impl<C> Pool<C> {
    fn new(copies: Vec<C>) -> Self {
        Pool {
            count: copies.len(),
            free: Mutex::new(copies),
            returned: Condvar::new(),
        }
    }

    fn take(&self) -> C {
        let mut free = self.free.lock().unwrap();
        loop {
            if let Some(copy) = free.pop() {
                return copy;
            }
            free = self.returned.wait(free).unwrap();
        }
    }

    fn give(&self, copy: C) {
        self.free.lock().unwrap().push(copy);
        self.returned.notify_one();
    }

    fn into_inner(self) -> Vec<C> {
        let copies = self.free.into_inner().unwrap();
        assert_eq!(copies.len(), self.count, "every copy is back");
        copies
    }
}

/// Prints one message. A message of several lines is not mixed with others.
fn say(text: &str) {
    println!("{text}");
}

/// The end of `text`, at most `bytes` long.
fn tail(text: &str, bytes: usize) -> &str {
    let mut start = text.len().saturating_sub(bytes);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    &text[start..]
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn write(path: &Path, text: &str) {
    fs::write(path, text)
        .unwrap_or_else(|error| panic!("cannot write {}: {error}", path.display()));
}

/// The longest that a build or a whole run of the tests may take. A command that takes
/// longer does not end by itself.
const RUN_LIMIT: Duration = Duration::from_secs(30 * 60);

/// Runs a command to its end, for at most [`RUN_LIMIT`]. Gives its exit code (-1 when a
/// signal stopped it) and its output and error output together.
fn output(command: &mut Command) -> (i32, String) {
    let Some((code, stdout, stderr)) = output_within(command, RUN_LIMIT) else {
        panic!(
            "{command:?} did not end in {} minutes and was stopped",
            RUN_LIMIT.as_secs() / 60
        );
    };
    (code, stdout + &stderr)
}

/// Runs a command for at most `limit`. Gives its exit code, output and error output, or
/// `None` when it did not end in time and was stopped.
fn output_within(command: &mut Command, limit: Duration) -> Option<(i32, String, String)> {
    let mut child = command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap_or_else(|error| panic!("cannot run {command:?}: {error}"));
    let drain = |mut pipe: Box<dyn Read + Send>| {
        thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        })
    };
    let stdout = drain(Box::new(child.stdout.take().unwrap()));
    let stderr = drain(Box::new(child.stderr.take().unwrap()));
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child.try_wait().unwrap() {
            break Some(status);
        }
        if start.elapsed() > limit {
            let _ = child.kill();
            let _ = child.wait();
            break None;
        }
        thread::sleep(Duration::from_millis(20));
    };
    let (stdout, stderr) = (stdout.join().unwrap(), stderr.join().unwrap());
    status.map(|status| (status.code().unwrap_or(-1), stdout, stderr))
}

/// The names of the failed tests in the output of a Rust test binary.
fn failed_rust_tests(output: &str) -> impl Iterator<Item = String> + '_ {
    output.lines().filter_map(|line| {
        let name = line.strip_prefix("test ")?.strip_suffix(" ... FAILED")?;
        (!name.is_empty() && !name.contains(char::is_whitespace)).then(|| name.to_string())
    })
}

/// Copies a directory. `ignore` names entries to leave out at every level. A symbolic link
/// is copied as a link when `keep_links` is true, and as what it points to when it is not.
/// The files keep their times of change, so that Cargo sees a copied build as up to date.
fn copy_tree(from: &Path, to: &Path, ignore: &[&str], keep_links: bool) {
    let fail = |error: std::io::Error| -> ! {
        panic!(
            "cannot copy {} to {}: {error}",
            from.display(),
            to.display()
        )
    };
    fs::create_dir_all(to).unwrap_or_else(|error| fail(error));
    for entry in fs::read_dir(from).unwrap_or_else(|error| fail(error)) {
        let entry = entry.unwrap_or_else(|error| fail(error));
        let name = entry.file_name();
        if ignore.iter().any(|ignored| name == *ignored) {
            continue;
        }
        let (source, target) = (entry.path(), to.join(&name));
        let mut metadata = fs::symlink_metadata(&source).unwrap_or_else(|error| fail(error));
        if metadata.file_type().is_symlink() {
            if keep_links {
                let points_to = fs::read_link(&source).unwrap_or_else(|error| fail(error));
                symlink(&points_to, &target, source.is_dir());
                continue;
            }
            metadata = fs::metadata(&source).unwrap_or_else(|error| fail(error));
        }
        if metadata.is_dir() {
            copy_tree(&source, &target, ignore, keep_links);
        } else {
            fs::copy(&source, &target).unwrap_or_else(|error| fail(error));
            if let Ok(modified) = metadata.modified() {
                if let Ok(file) = fs::File::options().write(true).open(&target) {
                    let _ = file.set_modified(modified);
                }
            }
        }
    }
}

fn symlink(points_to: &Path, link: &Path, is_dir: bool) {
    #[cfg(unix)]
    let made = {
        let _ = is_dir;
        std::os::unix::fs::symlink(points_to, link)
    };
    #[cfg(windows)]
    let made = if is_dir {
        std::os::windows::fs::symlink_dir(points_to, link)
    } else {
        std::os::windows::fs::symlink_file(points_to, link)
    };
    made.unwrap_or_else(|error| panic!("cannot make the link {}: {error}", link.display()));
}

/// A path as text with `/`, as the mutations and the tools name files.
fn slashed(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A mutation whose text is gone, or occurs twice, would be skipped as stale. This finds
    /// such mutations without running any.
    #[test]
    fn every_mutation_finds_its_text_once() {
        let root = crate::root();
        let package = root.join("packages/refpact");
        let rust = list::rust().into_iter().map(|mutation| (&root, mutation));
        let typescript = list::typescript()
            .into_iter()
            .map(|mutation| (&package, mutation));
        let stale: Vec<String> = rust
            .chain(typescript)
            .filter_map(|(base, mutation)| {
                let text = read(&base.join(&mutation.file));
                let count = text.matches(mutation.find.as_str()).count();
                (count != 1)
                    .then(|| format!("{} ({}: {count} times)", mutation.name, mutation.file))
            })
            .collect();
        assert!(stale.is_empty(), "stale mutations:\n{}", stale.join("\n"));
    }

    #[test]
    fn failed_tests_in_the_output() {
        let output = "test a ... ok\ntest b::c ... FAILED\ntest d e ... FAILED\nfailures:\n";
        assert_eq!(failed_rust_tests(output).collect::<Vec<_>>(), ["b::c"]);
    }

    #[test]
    fn the_tail_keeps_whole_characters() {
        assert_eq!(tail("aé", 1), "");
        assert_eq!(tail("aé", 2), "é");
        assert_eq!(tail("ab", 9), "ab");
    }
}
