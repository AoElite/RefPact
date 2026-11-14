//! Allow-lists, cycles and baselines, on the planted fixture (`src/rings`).

use std::path::PathBuf;
use std::sync::LazyLock;

use refpact::{
    all_except, any_of, rule, Baseline, Crate, Detail, ErrorCode, Failure, Rule, Violation,
};

static PLANTED: LazyLock<Crate> = LazyLock::new(|| {
    Crate::options(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust/planted"
    ))
    .resolving_members()
    .load()
    .unwrap()
});
static APP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::options(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust/app"
    ))
    .resolving_members()
    .load()
    .unwrap()
});

/// Line, kind and path of each violation.
fn uses(rule: &Rule) -> Vec<(u32, String, String)> {
    rule.evaluate(&PLANTED)
        .unwrap()
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Uses {
                target,
                kind,
                written,
                ..
            } => {
                assert_eq!(written, &None, "an allow-list reports the path as written");
                (violation.line, kind.clone(), target.join("::"))
            }
            other => panic!("not a use: {other:?}"),
        })
        .collect()
}

fn owned(list: &[(u32, &str, &str)]) -> Vec<(u32, String, String)> {
    list.iter()
        .map(|(line, kind, path)| (*line, (*kind).to_owned(), (*path).to_owned()))
        .collect()
}

fn strict(allowed: &[&str]) -> Rule {
    rule("strict uses little")
        .modules(any_of(["crate::rings::strict"]))
        .must_only_use(any_of(allowed))
}

// ---------------------------------------------------------------------------------------
// Allow-lists

/// An allow-list reports every other use, and not the module's own items. A use through a
/// re-export is allowed when the definition is. A test module is a module of its own. An
/// allowed target that does not exist is an error.
#[test]
fn allow_lists() {
    // `Own` and `own` of `strict.rs`, used in lines 9, 13, 14 and 22, are not in the list.
    assert_eq!(
        uses(&strict(&["std"])),
        owned(&[
            (3, "use", "crate::facade::Generator"),
            (4, "use", "crate::rings::leaf::Leaf"),
            (10, "type", "crate::rings::leaf::Leaf"),
            (17, "type", "crate::facade::Generator"),
            (22, "expr", "core::mem::size_of"),
        ])
    );
    assert_eq!(
        uses(&strict(&["std", "core", "crate::codegen"])),
        owned(&[
            (4, "use", "crate::rings::leaf::Leaf"),
            (10, "type", "crate::rings::leaf::Leaf")
        ])
    );
    strict(&["std", "core", "crate::facade", "crate::rings::leaf"]).check(&PLANTED);

    let with_tests = rule("strict and its tests")
        .modules(any_of(["crate::rings::strict::**"]))
        .must_only_use(any_of([
            "std",
            "core",
            "crate::facade",
            "crate::rings::leaf",
        ]));
    assert_eq!(
        uses(&with_tests),
        owned(&[(30, "expr", "crate::rings::strict::own")])
    );
    with_tests.except_in_test_code().check(&PLANTED);

    let errors = strict(&["std", "crate::rings::lief"])
        .evaluate(&PLANTED)
        .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(errors[0].value.as_deref(), Some("crate::rings::lief"));
}

// ---------------------------------------------------------------------------------------
// Cycles

/// The cycles as text: the files and lines of the steps, and the size of the tangle.
fn cycles(violations: &[Violation]) -> Vec<(String, Vec<String>, usize)> {
    violations
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Cycle { steps, size } => (
                violation.unit.join("::"),
                steps
                    .iter()
                    .map(|step| {
                        format!(
                            "{} -> {} at {}:{}",
                            step.from.join("::"),
                            step.to.join("::"),
                            step.file,
                            step.line
                        )
                    })
                    .collect(),
                *size,
            ),
            other => panic!("not a cycle: {other:?}"),
        })
        .collect()
}

fn ring_of_three() -> (String, Vec<String>, usize) {
    (
        "crate::rings::alpha".to_owned(),
        vec![
            "crate::rings::alpha -> crate::rings::beta at src/rings/alpha.rs:1".to_owned(),
            "crate::rings::beta -> crate::rings::gamma at src/rings/beta.rs:1".to_owned(),
            "crate::rings::gamma -> crate::rings::alpha at src/rings/gamma.rs:4".to_owned(),
        ],
        3,
    )
}

/// A cycle is reported once, at its first step, with every step. Slices join a module with
/// the modules below it. A rule that cannot fail is an error.
#[test]
fn cycle_rules() {
    let found = rule("no cycles")
        .modules(any_of(["crate::rings::*"]))
        .must_be_acyclic()
        .evaluate(&PLANTED)
        .unwrap();
    assert_eq!(cycles(&found), vec![ring_of_three()]);
    assert_eq!(
        (found[0].file.as_str(), found[0].line),
        ("src/rings/alpha.rs", 1)
    );
    let failure = rule("no cycles")
        .modules(any_of(["crate::rings::*"]))
        .must_be_acyclic()
        .verify(&PLANTED)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `no cycles` is violated in 1 place(s):\n  \
         src/rings/alpha.rs:1:18: [no cycles] crate::rings::alpha is in a cycle of 3 (the tangle has 3 in all):\n      \
         crate::rings::alpha -> crate::rings::beta  at src/rings/alpha.rs:1:18, uses crate::rings::beta::Beta (use)\n      \
         crate::rings::beta -> crate::rings::gamma  at src/rings/beta.rs:1:32, uses crate::rings::gamma::Gamma (type)\n      \
         crate::rings::gamma -> crate::rings::alpha  at src/rings/gamma.rs:4:29, uses crate::rings::alpha::Alpha (type)"
    );

    // `leaf::tests` uses `delta`, and `delta` uses `leaf`. Between slices that is a cycle.
    let between = rule("no cycles between rings")
        .modules(any_of(["crate::rings::**"]).except(["crate::rings"]))
        .must_be_acyclic_between(any_of(["crate::rings::*"]));
    let two = (
        "crate::rings::delta".to_owned(),
        vec![
            "crate::rings::delta -> crate::rings::leaf at src/rings/delta.rs:1".to_owned(),
            "crate::rings::leaf -> crate::rings::delta at src/rings/leaf.rs:5".to_owned(),
        ],
        2,
    );
    assert_eq!(
        cycles(&between.evaluate(&PLANTED).unwrap()),
        vec![ring_of_three(), two]
    );
    assert_eq!(
        cycles(&between.except_in_test_code().evaluate(&PLANTED).unwrap()),
        vec![ring_of_three()]
    );
    rule("no cycles between the parts")
        .modules(all_except(["crate"]))
        .must_be_acyclic_between(any_of(["crate::*"]))
        .except_in_test_code()
        .check(&APP);

    let one = rule("r")
        .modules(any_of(["crate::rings::alpha"]))
        .must_be_acyclic()
        .evaluate(&PLANTED);
    assert_eq!(one.unwrap_err()[0].code, ErrorCode::TooFewNodes);
    let no_slice = rule("r")
        .modules(any_of(["crate::rings::*"]))
        .must_be_acyclic_between(any_of(["crate::rings::*", "crate::circles::*"]))
        .evaluate(&PLANTED)
        .unwrap_err();
    assert_eq!(no_slice[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(no_slice[0].value.as_deref(), Some("crate::circles::*"));
}

// ---------------------------------------------------------------------------------------
// Baselines

/// A baseline file in a new directory, removed when the test ends.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Scratch {
        let dir = std::env::temp_dir().join(format!("refpact-{}-{name}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        Scratch { dir }
    }

    fn path(&self) -> PathBuf {
        self.dir.join("nested/architecture.baseline")
    }

    fn baseline(&self) -> Baseline {
        Baseline::file(self.path())
    }

    fn text(&self) -> String {
        std::fs::read_to_string(self.path()).unwrap()
    }

    fn set(&self, text: &str) {
        std::fs::create_dir_all(self.path().parent().unwrap()).unwrap();
        std::fs::write(self.path(), text).unwrap();
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

const HEADER: &str = "# refpact baseline: violations that are accepted for now.\n\
                      # Columns, separated by tabs: rule, unit, what, count.\n";

fn leaf_rule() -> Rule {
    rule("strict does not use leaf")
        .modules(any_of(["crate::rings::strict"]))
        .must_not_use(any_of(["crate::rings::leaf"]))
}

const LEAF_LINES: &str = "strict does not use leaf\tcrate::rings::strict\tuses type crate::rings::leaf::Leaf\t1\n\
                          strict does not use leaf\tcrate::rings::strict\tuses use crate::rings::leaf::Leaf\t1\n";

/// A written baseline has one line for each key and no line numbers. It accepts what it
/// lists: a new violation fails, an entry for a violation that is gone fails, and a file that
/// is not there accepts nothing. Rules share a file. A plain check does not read it.
#[test]
fn baseline_files() {
    let scratch = Scratch::new("files");
    assert!(matches!(
        leaf_rule().verify_with_baseline(&PLANTED, &scratch.baseline()),
        Err(Failure::Violated { violations, .. }) if violations.len() == 2
    ));

    leaf_rule()
        .write_baseline(&PLANTED, &scratch.baseline())
        .unwrap();
    assert_eq!(scratch.text(), format!("{HEADER}{LEAF_LINES}"));
    leaf_rule().check_with_baseline(&PLANTED, &scratch.baseline());
    assert!(
        matches!(leaf_rule().verify(&PLANTED), Err(Failure::Violated { violations, .. }) if violations.len() == 2)
    );

    // Two rules in one file; writing one again does not touch the other.
    let no_cycles = rule("no cycles")
        .modules(any_of(["crate::rings::*"]))
        .must_be_acyclic();
    no_cycles
        .write_baseline(&PLANTED, &scratch.baseline())
        .unwrap();
    leaf_rule()
        .write_baseline(&PLANTED, &scratch.baseline())
        .unwrap();
    let cycle_line =
        "no cycles\tcrate::rings::alpha\tcycle crate::rings::alpha > crate::rings::beta > crate::rings::gamma\t1\n";
    assert_eq!(scratch.text(), format!("{HEADER}{cycle_line}{LEAF_LINES}"));
    no_cycles.check_with_baseline(&PLANTED, &scratch.baseline());

    scratch.set(
        "strict does not use leaf\tcrate::rings::strict\tuses use crate::rings::leaf::Leaf\t1\n",
    );
    match leaf_rule().verify_with_baseline(&PLANTED, &scratch.baseline()) {
        Err(Failure::Violated { violations, .. }) => {
            assert_eq!(
                violations
                    .iter()
                    .map(|violation| violation.line)
                    .collect::<Vec<_>>(),
                [10]
            );
        }
        other => panic!("{other:?}"),
    }

    scratch.set(&format!("{LEAF_LINES}strict does not use leaf\tcrate::rings::strict\tuses expr crate::rings::leaf::Leaf\t2\n"));
    let failure = leaf_rule()
        .verify_with_baseline(&PLANTED, &scratch.baseline())
        .unwrap_err();
    match &failure {
        Failure::StaleBaseline { entries, .. } => {
            assert_eq!(entries.len(), 1);
            assert_eq!(
                (entries[0].what.as_str(), entries[0].count, entries[0].found),
                ("uses expr crate::rings::leaf::Leaf", 2, 0)
            );
        }
        other => panic!("{other:?}"),
    }
    let text = failure.to_string();
    assert!(text.contains("accepts violations that are gone"), "{text}");
    assert!(
        text.contains(
            "uses expr crate::rings::leaf::Leaf (the baseline accepts 2, the code has 0)"
        ),
        "{text}"
    );
    assert!(text.contains("REFPACT_BASELINE=update"), "{text}");

    // Writing a rule without violations removes its entries and keeps the others.
    scratch.set("strict is clean\tcrate::rings::strict\tuses use crate::tree::Gone\t1\nother\tcrate::a\tcontains unsafe-block\t1\n");
    let clean = rule("strict is clean")
        .modules(any_of(["crate::rings::strict"]))
        .must_not_use(any_of(["crate::tree"]));
    clean.write_baseline(&PLANTED, &scratch.baseline()).unwrap();
    assert_eq!(
        scratch.text(),
        format!("{HEADER}other\tcrate::a\tcontains unsafe-block\t1\n")
    );
}

/// A baseline that cannot be read fails with the line and is not replaced. A rule that
/// cannot be checked, or whose name has a tab, fails also with a baseline.
#[test]
fn baseline_errors() {
    let scratch = Scratch::new("errors");
    let wrong = rule("typo")
        .modules(any_of(["crate::rings::strikt"]))
        .must_not_use(any_of(["crate::rings::leaf"]));
    assert!(matches!(
        wrong.verify_with_baseline(&PLANTED, &scratch.baseline()),
        Err(Failure::Invalid { .. })
    ));
    assert!(matches!(
        wrong.write_baseline(&PLANTED, &scratch.baseline()),
        Err(Failure::Invalid { .. })
    ));
    assert!(!scratch.path().exists());

    let named = rule("a\tb")
        .modules(any_of(["crate::rings::strict"]))
        .must_not_use(any_of(["crate::rings::leaf"]));
    assert!(matches!(
        named.verify_with_baseline(&PLANTED, &scratch.baseline()),
        Err(Failure::Baseline { .. })
    ));

    scratch.set(&format!("{HEADER}{LEAF_LINES}this line has no tabs\n"));
    match leaf_rule().verify_with_baseline(&PLANTED, &scratch.baseline()) {
        Err(Failure::Baseline { message }) => assert!(
            message.ends_with("architecture.baseline:5: this line of the baseline is not valid"),
            "{message}"
        ),
        other => panic!("{other:?}"),
    }
    assert!(matches!(
        leaf_rule().write_baseline(&PLANTED, &scratch.baseline()),
        Err(Failure::Baseline { .. })
    ));
}
