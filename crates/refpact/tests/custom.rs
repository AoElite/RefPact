//! Rules with a check of the user: the subjects, the filters, the exemptions, the errors,
//! the reports and the baselines are those of every rule.

use std::sync::LazyLock;

use refpact::core::{baseline_key, Custom, NoReferences, PathFormat};
use refpact::{
    all, all_except, any_of, rule, Check, Checker, Crate, Detail, ErrorCode, Failure, Role, Scope,
    Setup, Violation,
};

static APP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod a;\npub mod b;\n"),
            (
                "src/a.rs",
                "use std::fs;\n\
                 use std::io;\n\
                 use std::env;\n\
                 \n\
                 #[cfg(test)]\n\
                 mod tests {\n\
                     use std::net;\n\
                 }\n",
            ),
            ("src/b.rs", "use crate::a;\n"),
        ],
    )
    .unwrap()
});

/// One violation for each module with more than `limit` `use` declarations that the rule
/// sees, at the first one.
fn at_most(limit: usize) -> impl Fn(&Scope<'_>) -> Vec<Violation> + Send + Sync + 'static {
    move |scope: &Scope<'_>| {
        let units = &scope.facts().units;
        scope
            .subjects()
            .filter_map(|unit| {
                let uses: Vec<_> = scope
                    .references()
                    .filter(|seen| seen.unit == unit)
                    .collect();
                let first = uses.first()?;
                (uses.len() > limit).then(|| {
                    let message = format!("has {} uses, more than {limit}", uses.len());
                    let detail =
                        Detail::custom_with_key(message, format!("more than {limit} uses"));
                    Violation {
                        unit: units[unit].clone(),
                        ..scope.violation_at(first.fact, detail)
                    }
                })
            })
            .collect()
    }
}

#[test]
fn a_check_of_the_user_finds_violations_and_reports_them() {
    let failure = rule("small modules")
        .modules(all())
        .must_satisfy(at_most(2))
        .because("small modules are easy to read")
        .verify(&APP)
        .unwrap_err();
    let Failure::Violated { violations, .. } = &failure else {
        panic!("{failure}")
    };
    assert_eq!(violations.len(), 1, "{failure}");
    assert_eq!(violations[0].unit.join("::"), "crate::a");
    assert_eq!(violations[0].line, 1);
    assert_eq!(
        failure.to_string(),
        "rule `small modules` is violated in 1 place(s):\n  because small modules are easy to read\n  \
         src/a.rs:1:10: [small modules] crate::a has 3 uses, more than 2"
    );
}

#[test]
fn the_filters_and_the_subjects_apply_to_a_check_of_the_user() {
    // The only use of the test module is test code.
    let passes = rule("no uses")
        .modules(any_of(["crate::a::tests"]))
        .must_satisfy(at_most(0))
        .except_in_test_code();
    assert_eq!(passes.verify(&APP), Ok(()));
    let fails = rule("no uses")
        .modules(any_of(["crate::a::tests"]))
        .must_satisfy(at_most(0));
    assert!(fails.verify(&APP).is_err());
    let others = rule("small modules")
        .modules(all_except(["crate::a", "crate::a::tests"]))
        .must_satisfy(at_most(0));
    let found = others.evaluate(&APP).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].unit.join("::"), "crate::b");
}

#[test]
fn an_exemption_hides_a_reference_from_a_check_of_the_user() {
    let small = || {
        rule("small modules")
            .modules(any_of(["crate::a"]))
            .must_satisfy(at_most(1))
            .except_in_test_code()
    };
    assert!(small().verify(&APP).is_err());
    let exempt = small().ignoring_use(any_of(["crate::a"]), any_of(["std::fs", "std::io"]));
    assert_eq!(exempt.verify(&APP), Ok(()));
}

/// A check with an argument: the targets that it counts.
struct Counts {
    targets: refpact::core::Targets,
}

impl Check for Counts {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        let targets = setup.targets(&self.targets);
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            scope
                .references()
                .filter(|seen| scope.selects(seen.unit))
                .filter(|seen| {
                    scope
                        .owners(seen)
                        .any(|(path, _)| targets.iter().any(|target| target.matches_within(path)))
                })
                .map(|seen| scope.violation_at(seen.fact, Detail::custom("uses a counted path")))
                .collect()
        })
    }
}

fn counts(patterns: &[&str]) -> Counts {
    Counts {
        targets: refpact::core::Targets {
            any_of: patterns
                .iter()
                .map(|pattern| refpact::core::split(pattern, "::"))
                .collect(),
            unverified: Vec::new(),
        },
    }
}

#[test]
fn a_check_of_the_user_reads_its_arguments_first() {
    let found = rule("counted")
        .modules(all())
        .must_meet(counts(&["std::fs", "crate::a"]))
        .evaluate(&APP)
        .unwrap();
    let places: Vec<(String, u32)> = found
        .iter()
        .map(|violation| (violation.file.clone(), violation.line))
        .collect();
    assert_eq!(
        places,
        [("src/a.rs".to_owned(), 1), ("src/b.rs".to_owned(), 1)]
    );

    let errors = rule("counted")
        .modules(all())
        .must_meet(counts(&["crate::nowhere"]))
        .evaluate(&APP)
        .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(errors[0].role, Some(Role::Target));
    assert_eq!(errors[0].value.as_deref(), Some("crate::nowhere"));
}

#[test]
fn the_rule_errors_of_every_rule_apply_to_a_check_of_the_user() {
    let errors = rule("small modules")
        .modules(any_of(["crate::nowhere"]))
        .must_satisfy(at_most(2))
        .evaluate(&APP)
        .unwrap_err();
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(errors[0].role, Some(Role::Subject));
}

/// A check about modules as a whole, which reads no references.
struct NotEmpty;

impl Check for NotEmpty {
    fn prepare<'c>(&'c self, _: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        Box::new(NoReferences(|scope: &Scope<'_>| -> Vec<Violation> {
            scope
                .subjects()
                .filter(|&unit| !scope.references().any(|seen| seen.unit == unit))
                .map(|unit| scope.violation_of_unit(unit, Detail::custom("is empty")))
                .collect()
        }))
    }
}

#[test]
fn an_exemption_of_a_check_that_reads_no_references_is_an_error() {
    let errors = rule("not empty")
        .modules(all())
        .must_meet(NotEmpty)
        .ignoring_use(any_of(["crate::b"]), any_of(["crate::a"]))
        .evaluate(&APP)
        .unwrap_err();
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::UnusedExemption);
    let found = rule("not empty")
        .modules(all())
        .must_meet(NotEmpty)
        .evaluate(&APP)
        .unwrap();
    let units: Vec<String> = found
        .iter()
        .map(|violation| violation.unit.join("::"))
        .collect();
    assert_eq!(units, ["crate"]);
    // The root has no fact, so the violation has no file.
    assert_eq!((found[0].file.as_str(), found[0].line), ("", 0));
}

#[test]
fn a_violation_of_a_module_as_a_whole() {
    let found = rule("every module")
        .modules(any_of(["crate::b"]))
        .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
            scope
                .subjects()
                .map(|unit| scope.violation_of_unit(unit, Detail::custom("is here")))
                .collect()
        })
        .evaluate(&APP)
        .unwrap();
    let places: Vec<(String, &str, u32)> = found
        .iter()
        .map(|violation| {
            (
                violation.unit.join("::"),
                violation.file.as_str(),
                violation.line,
            )
        })
        .collect();
    assert_eq!(places, [("crate::b".to_owned(), "src/b.rs", 0)]);
}

#[test]
fn a_baseline_counts_a_violation_of_the_user_by_its_key() {
    let format = PathFormat::plain("::");
    let violation = |detail: Detail| Violation {
        rule: "r".into(),
        unit: vec!["crate".into(), "a".into()],
        file: "src/a.rs".into(),
        line: 1,
        column: 1,
        note: None,
        detail,
    };
    assert_eq!(
        baseline_key(&violation(Detail::custom("has 4 uses")), &format),
        ("crate::a".to_owned(), "custom has 4 uses".to_owned())
    );
    assert_eq!(
        baseline_key(
            &violation(Detail::custom_with_key("has 4 uses", "too\tmany\nuses")),
            &format
        ),
        ("crate::a".to_owned(), "custom too many uses".to_owned())
    );
}

#[test]
fn a_custom_check_equals_only_its_clones() {
    let check = Custom::new(at_most(1));
    assert_eq!(check, check.clone());
    assert_ne!(check, Custom::new(at_most(1)));
    // A rule stays a value that crosses threads and `catch_unwind`.
    fn value<T: Send + Sync + std::panic::RefUnwindSafe + std::panic::UnwindSafe>(_: &T) {}
    value(&rule("r").modules(all()).must_satisfy(at_most(1)));
}
