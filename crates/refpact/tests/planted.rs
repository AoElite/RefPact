//! Every detection form, on the planted fixture.
//!
//! Each file of `fixtures/rust/planted/src/forms` and `.../tree` marks its violations with
//! `//~` comments (the format is described in the fixture's `lib.rs`). One test runs the rules
//! on all of them and requires that the violations are exactly the marked ones: the same
//! file, line, kind and path. A form that the library stops detecting fails that test, and
//! its message names the missing violations.

use std::sync::LazyLock;

use refpact::{any_of, rule, Crate, Detail, Kind, Rule, Violation};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rust/planted");

static PLANTED: LazyLock<Crate> =
    LazyLock::new(|| Crate::options(FIXTURE).resolving_members().load().unwrap());

/// The targets that the planted fixture uses on purpose.
const FORBIDDEN: [&str; 15] = [
    "crate::codegen",
    "std::fs",
    "std::env",
    "std::process",
    "std::net",
    "std::thread",
    "std::time",
    "getrandom",
    "rand::thread_rng",
    "rand::rng",
    "rand::rngs::OsRng",
    "rand::rngs::ThreadRng",
    "rand_core::OsRng",
    "tracekit",
    "logkit",
];

fn uses(modules: &[&str]) -> Rule {
    rule("planted uses")
        .modules(any_of(modules))
        .must_not_use(any_of(FORBIDDEN))
}

fn contains(modules: &[&str]) -> Rule {
    rule("planted unsafe")
        .modules(any_of(modules))
        .must_not_contain(refpact::unsafe_code())
}

/// One violation in the notation of the `//~` comments.
fn notation(violation: &Violation) -> (String, u32, String) {
    let text = match &violation.detail {
        Detail::Uses {
            target,
            written,
            kind,
            certainty,
            ..
        } => {
            let mut text = format!("{kind} {}", target.join("::"));
            if certainty.as_str() == "candidate" {
                text.push_str(" ?");
            }
            if let Some(written) = written {
                text.push_str(&format!(" written={}", written.join("::")));
            }
            text
        }
        Detail::Contains { marker } => format!("marker {marker}"),
        Detail::Cycle { steps, .. } => format!("cycle of {}", steps.len()),
        Detail::Unused => "unused".to_owned(),
        Detail::Reaches { end, .. } => format!("reaches {}", end.target.join("::")),
        Detail::Unreachable => "unreachable".to_owned(),
        Detail::Outside => "outside".to_owned(),
        Detail::Item { item, .. } => format!("item {}", item.join("::")),
        Detail::TooLong { lines, .. } => format!("too long {lines}"),
        Detail::Custom { message, .. } => format!("custom {message}"),
    };
    (violation.file.clone(), violation.line, text)
}

fn found(rules: &[Rule]) -> Vec<(String, u32, String)> {
    let mut found: Vec<_> = rules
        .iter()
        .flat_map(|rule| {
            rule.evaluate(&PLANTED)
                .unwrap_or_else(|errors| panic!("{errors:?}"))
        })
        .map(|violation| notation(&violation))
        .collect();
    found.sort();
    found
}

/// The `//~` comments of the fixture files in the given directories, and below them.
fn marked(directories: &[&str]) -> Vec<(String, u32, String)> {
    fn walk(directory: &std::path::Path, files: &mut Vec<std::path::PathBuf>) {
        for entry in std::fs::read_dir(directory).unwrap() {
            let path = entry.unwrap().path();
            if path.is_dir() {
                walk(&path, files);
            } else if path.extension().is_some_and(|extension| extension == "rs") {
                files.push(path);
            }
        }
    }
    let mut files = Vec::new();
    for directory in directories {
        walk(&std::path::Path::new(FIXTURE).join(directory), &mut files);
    }
    let mut marked = Vec::new();
    for path in files {
        let file = path
            .strip_prefix(FIXTURE)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        let text = std::fs::read_to_string(&path).unwrap();
        for (index, line) in text.lines().enumerate() {
            for comment in line.split("//~ ").skip(1) {
                marked.push((file.clone(), index as u32 + 1, comment.trim().to_owned()));
            }
        }
    }
    marked.sort();
    marked
}

/// Every detection form and every way to declare a module, in one test: the rules find
/// exactly the violations that the `//~` comments of `src/forms` and `src/tree` mark. The
/// file `forms/local_names.rs` marks none; a violation found there is an extra one.
#[test]
fn every_planted_violation_is_found_and_nothing_else() {
    let modules = ["crate::forms::**", "crate::tree::**"];
    let marked = marked(&["src/forms", "src/tree"]);
    let found = found(&[uses(&modules), contains(&modules)]);
    let missing: Vec<_> = marked
        .iter()
        .filter(|entry| !found.contains(entry))
        .collect();
    let extra: Vec<_> = found
        .iter()
        .filter(|entry| !marked.contains(entry))
        .collect();
    assert!(
        missing.is_empty() && extra.is_empty() && found.len() == marked.len(),
        "not found: {missing:#?}\nnot marked: {extra:#?}"
    );
    assert!(
        marked.len() > 100,
        "the fixture marks only {} violations",
        marked.len()
    );

    // Inline modules are modules of their own, and so is a file below one of them.
    let mut units: Vec<String> = uses(&["crate::tree::**"])
        .evaluate(&PLANTED)
        .unwrap()
        .iter()
        .map(|violation| violation.unit.join("::"))
        .collect();
    units.retain(|unit| unit.starts_with("crate::tree::inline"));
    units.sort();
    units.dedup();
    assert_eq!(
        units,
        [
            "crate::tree::inline",
            "crate::tree::inline::deeper",
            "crate::tree::inline::from_file"
        ]
    );
}

// --- Combination: no list may weaken a rule ---------------------------------------------

/// A list of targets must not weaken a rule: one target is caught, and the same target
/// together with a second one must be caught as well, in both orders (see
/// docs/comparison.md for a tool where this differs). The same holds
/// for the patterns of the modules.
#[test]
fn no_list_weakens_a_rule() {
    let with = |targets: &[&str]| {
        let rule = rule("targets")
            .modules(any_of(["crate::forms::use_simple"]))
            .must_not_use(any_of(targets));
        rule.evaluate(&PLANTED)
            .unwrap()
            .iter()
            .map(notation)
            .collect::<Vec<_>>()
    };
    let one = with(&["crate::codegen"]);
    assert_eq!(
        one,
        [(
            "src/forms/use_simple.rs".to_owned(),
            1,
            "use crate::codegen::Generator".to_owned()
        )]
    );
    assert_eq!(with(&["crate::codegen", "crate::tree"]), one);
    assert_eq!(with(&["crate::tree", "crate::codegen"]), one);
    assert_eq!(
        with(&["std::net", "crate::tree", "crate::codegen", "getrandom"]),
        one
    );

    let with = |modules: &[&str]| found(&[uses(modules)]);
    let one = with(&["crate::forms::use_simple"]);
    assert_eq!(one.len(), 1);
    let two = with(&["crate::forms::use_simple", "crate::forms::use_group"]);
    assert_eq!(two.len(), 2);
    assert!(two.contains(&one[0]));
    assert_eq!(
        with(&["crate::forms::use_group", "crate::forms::use_simple"]),
        two
    );
}

fn errors(rule: Rule) -> Vec<(String, Option<String>)> {
    rule.evaluate(&PLANTED)
        .expect_err("the rule must be an error")
        .into_iter()
        .map(|error| (error.code.as_str().to_owned(), error.value))
        .collect()
}

#[test]
fn a_rule_that_selects_nothing_is_an_error() {
    let code = "pattern-matches-nothing".to_owned();
    // A module pattern that matches nothing, alone and beside one that matches.
    assert_eq!(
        errors(
            rule("r")
                .modules(any_of(["crate::forms::no_such"]))
                .must_not_use(any_of(["std::fs"]))
        ),
        [(code.clone(), Some("crate::forms::no_such".to_owned()))],
    );
    assert_eq!(
        errors(
            rule("r")
                .modules(any_of([
                    "crate::forms::use_simple",
                    "crate::forms::no_such"
                ]))
                .must_not_use(any_of(["crate::codegen"]))
        ),
        [(code.clone(), Some("crate::forms::no_such".to_owned()))],
    );
    // An exception that matches nothing.
    assert_eq!(
        errors(
            rule("r")
                .modules(refpact::all_except(["crate::gone::**"]))
                .must_not_use(any_of(["std::fs"]))
        ),
        [(code.clone(), Some("crate::gone::**".to_owned()))],
    );
    // A target of this crate that does not exist, beside one that is violated.
    assert_eq!(
        errors(
            rule("r")
                .modules(any_of(["crate::forms::use_simple"]))
                .must_not_use(any_of(["crate::codegen", "crate::codgen"]))
        ),
        [(code.clone(), Some("crate::codgen".to_owned()))],
    );
    // An item that the module does not declare.
    assert_eq!(
        errors(
            rule("r")
                .modules(refpact::all())
                .must_not_use(any_of(["crate::codegen::Genrator"]))
        ),
        [(code, Some("crate::codegen::Genrator".to_owned()))],
    );
    // Exceptions that remove every module.
    assert_eq!(
        errors(
            rule("r")
                .modules(any_of(["crate::codegen"]).except(["crate::**"]))
                .must_not_use(any_of(["std::fs"]))
        ),
        [("no-subjects".to_owned(), None)],
    );
    // An unverified target is not checked for existence.
    let with_unverified = rule("r")
        .modules(any_of(["crate::forms::use_simple"]))
        .must_not_use(any_of(["crate::codegen"]).and_unverified(["crate::codegen::MadeByAMacro"]));
    assert_eq!(with_unverified.evaluate(&PLANTED).unwrap().len(), 1);
    let only_unverified = rule("r")
        .modules(any_of(["crate::forms::use_simple"]))
        .must_not_use(refpact::unverified(["crate::codegen::MadeByAMacro"]));
    assert_eq!(only_unverified.evaluate(&PLANTED).unwrap().len(), 0);
}

#[test]
fn check_panics_with_a_report_for_an_error_and_for_a_violation() {
    let invalid = rule("typo")
        .modules(any_of(["crate::nope"]))
        .must_not_use(any_of(["std::fs"]));
    let message = invalid.verify(&PLANTED).unwrap_err().to_string();
    assert_eq!(
        message,
        "rule `typo` cannot be checked:\n  subject pattern `crate::nope` matches nothing in the analysed code"
    );

    let violated = rule("no codegen here")
        .modules(any_of(["crate::forms::use_simple"]))
        .must_not_use(any_of(["crate::codegen"]));
    let message = violated.verify(&PLANTED).unwrap_err().to_string();
    assert_eq!(
        message,
        "rule `no codegen here` is violated in 1 place(s):\n  \
         src/forms/use_simple.rs:1:21: [no codegen here] crate::forms::use_simple uses crate::codegen::Generator (use)"
    );
    assert!(std::panic::catch_unwind(|| violated.check(&PLANTED)).is_err());
}

// --- Filters ---------------------------------------------------------------------------

/// Test code, candidates and kinds can be left out, and a violation says how the name came.
#[test]
fn the_filters_of_a_rule() {
    let modules = ["crate::forms::test_code::**"];
    assert_eq!(found(&[uses(&modules)]).len(), 3);
    assert_eq!(found(&[uses(&modules).except_in_test_code()]), Vec::new());

    // The index proves a name that a glob import brings in. The syntax alone gives a
    // candidate.
    let modules = ["crate::forms::glob_candidates"];
    assert_eq!(found(&[uses(&modules)]).len(), 2);
    assert_eq!(found(&[uses(&modules).ignoring_candidates()]).len(), 2);
    let note = uses(&modules).evaluate(&PLANTED).unwrap()[0].note.clone();
    assert_eq!(note.as_deref(), Some("through the glob import at line 5"));
    let syntax = Crate::options(FIXTURE).load().unwrap();
    let candidates = uses(&modules).evaluate(&syntax).unwrap();
    let texts: Vec<String> = candidates.iter().map(|found| notation(found).2).collect();
    assert_eq!(
        texts,
        ["expr rand::thread_rng ?", "expr rand::rngs::OsRng ?"]
    );
    assert_eq!(
        candidates[0].note.as_deref(),
        Some("may come through the glob import at line 5")
    );
    let certain = uses(&modules).ignoring_candidates();
    assert_eq!(certain.evaluate(&syntax).unwrap(), Vec::new());

    let modules = ["crate::forms::use_then_name"];
    let texts = |rule: Rule| {
        found(&[rule])
            .into_iter()
            .map(|(_, _, text)| text)
            .collect::<Vec<_>>()
    };
    assert_eq!(texts(uses(&modules)), ["use std::fs", "expr std::fs::read"]);
    assert_eq!(
        texts(uses(&modules).only_kinds([Kind::Use])),
        ["use std::fs"]
    );
    assert_eq!(
        texts(uses(&modules).ignoring_kinds([Kind::Use])),
        ["expr std::fs::read"]
    );
    let violations = uses(&modules).evaluate(&PLANTED).unwrap();
    assert_eq!(
        violations[1].note.as_deref(),
        Some("through the `use` at line 1")
    );

    let modules = ["crate::forms::macro_tokens"];
    assert_eq!(
        texts(uses(&modules).ignoring_kinds([Kind::MacroTokens])),
        ["use std::fs"]
    );
}
