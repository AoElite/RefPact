//! Layers, independent slices, unused modules and chains, on `fixtures/rust/layered`.

use std::sync::LazyLock;

use refpact::{
    all, all_except, any_of, layers, rule, rules, Baseline, Crate, Detail, ErrorCode, Failure,
    Layers, Rule, Violation,
};

/// Removes a temporary file or directory when the test ends, also when it fails.
struct Removed(std::path::PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

static LAYERED: LazyLock<Crate> = LazyLock::new(|| {
    Crate::options(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust/layered"
    ))
    .resolving_members()
    .load()
    .unwrap()
});

fn units(violations: &[Violation]) -> Vec<String> {
    let mut units: Vec<String> = violations
        .iter()
        .map(|violation| violation.unit.join("::"))
        .collect();
    units.sort();
    units
}

/// File, line and path of each use.
fn uses(violations: &[Violation]) -> Vec<(String, u32, String)> {
    violations
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Uses { target, .. } => {
                (violation.file.clone(), violation.line, target.join("::"))
            }
            other => panic!("not a use: {other:?}"),
        })
        .collect()
}

fn place(file: &str, line: u32, path: &str) -> (String, u32, String) {
    (file.to_owned(), line, path.to_owned())
}

// ---------------------------------------------------------------------------------------
// Layers

fn shop() -> Layers {
    layers("shop")
        .layer("api", any_of(["crate::api::**"]))
        .layer("domain", any_of(["crate::domain::**"]))
        .layer("storage", any_of(["crate::storage::**"]))
}

/// A layer must not use any layer above it, not only the next one. Test code can be left
/// out, a baseline is accepted, and layers that cannot fail are an error.
#[test]
fn layer_rules() {
    match shop().verify(&LAYERED) {
        Err(Failure::Several(failures)) => {
            let found: Vec<_> = failures
                .iter()
                .map(|failure| match failure {
                    Failure::Violated {
                        rule, violations, ..
                    } => (rule.as_str(), uses(violations)),
                    other => panic!("{other:?}"),
                })
                .collect();
            // The storage is two layers below the api.
            assert_eq!(
                found,
                vec![
                    (
                        "shop: domain does not use the layers above it",
                        vec![place("src/domain/price.rs", 3, "crate::api::http::ROUTE")]
                    ),
                    (
                        "shop: storage does not use the layers above it",
                        vec![place("src/storage/memory.rs", 7, "crate::api::http::ROUTE")]
                    ),
                ]
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(shop().rules().unwrap().len(), 2);
    // Without the api, the rule of the storage passes.
    layers("no api")
        .layer("domain", any_of(["crate::domain::**"]))
        .layer("storage", any_of(["crate::storage::**"]))
        .check(&LAYERED);

    match shop().except_in_test_code().verify(&LAYERED) {
        Err(Failure::Violated {
            rule, violations, ..
        }) => {
            assert_eq!(rule, "shop: domain does not use the layers above it");
            assert_eq!(violations.len(), 1);
        }
        other => panic!("{other:?}"),
    }

    let file = std::env::temp_dir().join(format!("refpact-{}-layers.baseline", std::process::id()));
    let _removed = Removed(file.clone());
    std::fs::write(
        &file,
        "shop: domain does not use the layers above it\tcrate::domain::price\tuses expr crate::api::http::ROUTE\t1\n\
         shop: storage does not use the layers above it\tcrate::storage::memory::tests\tuses expr crate::api::http::ROUTE\t1\n",
    )
    .unwrap();
    shop().check_with_baseline(&LAYERED, &Baseline::file(&file));

    let one = layers("one")
        .layer("api", any_of(["crate::api::**"]))
        .verify(&LAYERED);
    assert!(matches!(one, Err(Failure::Malformed { .. })), "{one:?}");
    assert!(one
        .unwrap_err()
        .to_string()
        .contains("fewer than two layers"));
    let typo = layers("typo")
        .layer("api", any_of(["crate::api::**"]))
        .layer("domain", any_of(["crate::domian::**"]));
    match typo.verify(&LAYERED) {
        Err(Failure::Invalid { errors, .. }) => {
            assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing)
        }
        other => panic!("{other:?}"),
    }
}

/// Layers and an onion have every filter of a rule, for all their rules.
#[test]
fn layers_and_an_onion_have_the_filters_of_a_rule() {
    use refpact::{onion_architecture, Kind};

    // Both violations are paths in expressions.
    shop().only_kinds([Kind::Use]).check(&LAYERED);
    shop().ignoring_kinds([Kind::Expr]).check(&LAYERED);
    assert!(shop().only_kinds([Kind::Expr]).verify(&LAYERED).is_err());
    assert!(shop().ignoring_candidates().verify(&LAYERED).is_err());
    // A warning reports and passes. A reason is in the report of each rule.
    shop().as_warning().check(&LAYERED);
    let report = shop()
        .because("the api is the outside")
        .verify(&LAYERED)
        .unwrap_err()
        .to_string();
    assert_eq!(report.matches("the api is the outside").count(), 2);

    // One known use is left out. The rule of the other layer still fails.
    let exempt = shop().ignoring_use(
        any_of(["crate::domain::price"]),
        any_of(["crate::api::http::ROUTE"]),
    );
    match exempt.verify(&LAYERED) {
        Err(Failure::Violated { rule, .. }) => {
            assert_eq!(rule, "shop: storage does not use the layers above it")
        }
        other => panic!("{other:?}"),
    }

    // The rule about the modules in a layer looks at no reference, so a filter of the
    // references is not a filter of it: it does not fail for a tag that it does not need.
    let covered = shop()
        .every_module_in_a_layer(all_except(["crate"]))
        .except_in_test_code()
        .ignoring_kinds([Kind::Expr])
        .as_warning();
    let made = covered.rules().unwrap();
    assert_eq!(made.len(), 3);
    assert!(made[2].as_core().ignore_tags.is_empty());
    assert_eq!(made[0].as_core().ignore_tags, ["test"]);
    assert!(made
        .iter()
        .all(|rule| rule.as_core().severity == refpact::core::Severity::Warning));

    let onion = || {
        onion_architecture("the onion")
            .domain_models(any_of(["crate::storage::**"]))
            .application_services(any_of(["crate::domain::**"]))
            .adapter("web", any_of(["crate::api::**"]))
    };
    assert!(onion().verify(&LAYERED).is_err());
    onion().ignoring_kinds([Kind::Expr]).check(&LAYERED);
    onion().as_warning().check(&LAYERED);
    let diagram = |layers: Layers| {
        layers
            .diagram(&LAYERED, refpact::DiagramFormat::Mermaid)
            .unwrap()
    };
    // The filters decide what a diagram draws too.
    assert!(diagram(shop()).contains("stroke:#d62728"));
    assert!(!diagram(shop().ignoring_kinds([Kind::Expr])).contains("stroke:#d62728"));
}

/// The names of the rules that fail, and the violations of each.
fn failed(failure: Result<(), Failure>) -> Vec<(String, Vec<Violation>)> {
    let failures = match failure {
        Ok(()) => Vec::new(),
        Err(Failure::Several(failures)) => failures,
        Err(failure) => vec![failure],
    };
    failures
        .into_iter()
        .map(|failure| match failure {
            Failure::Violated {
                rule, violations, ..
            } => (rule, violations),
            other => panic!("{other}"),
        })
        .collect()
}

/// Strict layers, and layers that may use or be used by some layers only.
#[test]
fn layer_options() {
    // The api uses the domain, two layers below it.
    let three = || {
        layers("three")
            .layer("api", any_of(["crate::api::**"]))
            .layer("features", any_of(["crate::features::**"]))
            .layer("domain", any_of(["crate::domain::**"]))
    };
    let strict = failed(three().strict().verify(&LAYERED));
    let names: Vec<&str> = strict.iter().map(|(rule, _)| rule.as_str()).collect();
    assert_eq!(
        names,
        [
            "three: api uses only the layers it may use",
            "three: domain does not use the layers above it",
        ]
    );
    let api = uses(&strict[0].1);
    assert!(api.contains(&place("src/api/http.rs", 7, "crate::domain::price::route")));
    assert!(api.iter().all(|(file, ..)| file == "src/api/http.rs"));
    assert_eq!(three().strict().rules().unwrap().len(), 3);
    assert_eq!(three().rules().unwrap().len(), 2);

    // The api may use only the features.
    let only = failed(
        three()
            .may_only_use("api", ["features"])
            .may_only_use("domain", [])
            .except_in_test_code()
            .verify(&LAYERED),
    );
    assert_eq!(only.len(), 2);
    assert_eq!(only[0].0, "three: api uses only the layers it may use");
    assert_eq!(uses(&only[0].1), api);

    // Only the domain may use the storage; the tools use it too.
    let storage = || {
        layers("four")
            .layer("api", any_of(["crate::api::**"]))
            .layer("tools", any_of(["crate::tools"]))
            .layer("domain", any_of(["crate::domain::**"]))
            .layer("storage", any_of(["crate::storage::**"]))
            .except_in_test_code()
    };
    let used_by = failed(
        storage()
            .may_only_be_used_by("storage", ["domain"])
            .verify(&LAYERED),
    );
    let names: Vec<&str> = used_by.iter().map(|(rule, _)| rule.as_str()).collect();
    assert_eq!(
        names,
        [
            "four: tools uses only the layers it may use",
            "four: domain does not use the layers above it",
        ]
    );
    assert_eq!(
        uses(&used_by[0].1),
        [place("src/tools.rs", 3, "crate::storage::disk::read")]
    );
    // The tools are allowed as well.
    let both = failed(
        storage()
            .may_only_be_used_by("storage", ["tools", "domain"])
            .verify(&LAYERED),
    );
    assert_eq!(both.len(), 1, "{both:?}");
}

/// Every module must be in a layer, and layers that name what does not exist, or what the
/// order forbids anyway, are an error.
#[test]
fn layers_that_cover_the_crate_and_malformed_layers() {
    let covered = failed(
        shop()
            .except_in_test_code()
            .every_module_in_a_layer(all_except(["crate"]))
            .verify(&LAYERED),
    );
    let (rule, violations) = covered.last().unwrap();
    assert_eq!(rule, "shop: every module is in a layer");
    assert_eq!(
        units(violations),
        [
            "crate::dead",
            "crate::dead::inner",
            "crate::features",
            "crate::features::cart",
            "crate::features::pay",
            "crate::features::shared",
            "crate::tools",
        ]
    );
    assert!(violations
        .iter()
        .all(|violation| violation.detail == Detail::Outside && violation.line == 0));
    // Each violation has the file of its module, also for a module without a reference.
    let files: Vec<&str> = violations.iter().map(|found| found.file.as_str()).collect();
    assert_eq!(
        files,
        [
            "src/dead/mod.rs",
            "src/dead/inner.rs",
            "src/features/mod.rs",
            "src/features/cart.rs",
            "src/features/pay.rs",
            "src/features/shared.rs",
            "src/tools.rs",
        ]
    );
    // With a layer for the rest, every module is in a layer.
    let all_in = failed(
        shop()
            .layer(
                "rest",
                any_of(["crate::dead::**", "crate::features::**", "crate::tools"]),
            )
            .every_module_in_a_layer(all_except(["crate"]))
            .verify(&LAYERED),
    );
    assert!(!all_in.is_empty());
    assert!(all_in
        .iter()
        .all(|(rule, _)| rule != "shop: every module is in a layer"));

    let malformed = |layers: Layers| match layers.verify(&LAYERED) {
        Err(Failure::Malformed { rule, message }) => {
            assert_eq!(rule, "shop");
            message
        }
        other => panic!("{other:?}"),
    };
    let message = malformed(
        shop()
            .may_only_use("api", ["domian", "api"])
            .may_only_use("api", [])
            .may_only_be_used_by("storage", ["storage"])
            .may_only_be_used_by("db", []),
    );
    assert_eq!(
        message,
        "`domian` is not a layer\n  \
         `api` may only use `api`, which the order of the layers forbids\n  \
         `api` has two lists of the layers it may use\n  \
         `storage` may only be used by `storage`, which the order of the layers forbids\n  \
         `db` is not a layer"
    );
    let message = malformed(shop().strict().may_only_use("api", ["storage"]));
    assert!(
        message.contains("`api` may only use `storage`"),
        "{message}"
    );
    let message = malformed(shop().may_only_be_used_by("api", ["domain"]));
    assert!(
        message.contains("`api` may only be used by `domain`"),
        "{message}"
    );
    let message = malformed(
        shop()
            .may_only_be_used_by("domain", [])
            .may_only_be_used_by("domain", ["api"]),
    );
    assert_eq!(
        message,
        "`domain` has two lists of the layers that may use it"
    );
    let message = malformed(shop().layer("api", any_of(["crate::tools"])));
    assert_eq!(message, "two layers have the name `api`");
}

/// Only the domain may use the storage: every other module that uses it breaks the rule.
#[test]
fn only_some_modules_may_use_a_module() {
    let storage = rule("only the domain uses the storage")
        .modules(any_of(["crate::storage::**"]))
        .must_only_be_used_by(any_of(["crate::domain::**"]));
    assert_eq!(
        uses(&storage.evaluate(&LAYERED).unwrap()),
        [
            place("src/dead/inner.rs", 2, "crate::storage::memory::read"),
            place("src/tools.rs", 3, "crate::storage::disk::read"),
        ]
    );
    rule("only the domain and the tools use the storage")
        .modules(any_of(["crate::storage::**"]))
        .must_only_be_used_by(any_of([
            "crate::domain::**",
            "crate::tools",
            "crate::dead::**",
        ]))
        .check(&LAYERED);
    match rule("typo")
        .modules(any_of(["crate::storage::**"]))
        .must_only_be_used_by(any_of(["crate::domian::**"]))
        .verify(&LAYERED)
    {
        Err(Failure::Invalid { errors, .. }) => {
            assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing)
        }
        other => panic!("{other:?}"),
    }
}

/// An exemption lets one known use pass, also as an edge of a cycle, and an exemption that
/// lets nothing pass is an error.
#[test]
fn exemptions() {
    let domain = || {
        rule("the domain does not use the api")
            .modules(any_of(["crate::domain::**"]))
            .must_not_use(any_of(["crate::api"]))
    };
    assert_eq!(domain().evaluate(&LAYERED).unwrap().len(), 1);
    domain()
        .ignoring_use(
            any_of(["crate::domain::price"]),
            any_of(["crate::api::http::ROUTE"]),
        )
        .check(&LAYERED);

    // The modules of an exemption are a selection, as the modules of a rule are.
    domain()
        .ignoring_use(all(), any_of(["crate::api::http::ROUTE"]))
        .check(&LAYERED);
    let excepted = domain()
        .ignoring_use(
            any_of(["crate::domain::**"]).except(["crate::domain::price"]),
            any_of(["crate::api"]),
        )
        .verify(&LAYERED)
        .unwrap_err();
    assert!(excepted.to_string().contains(
        "exemption `crate::domain::** except crate::domain::price -> crate::api` lets no reference pass"
    ));

    let cycles = || {
        rule("the parts form no cycle")
            .modules(all())
            .must_be_acyclic_between(any_of(["crate::*"]))
            .except_in_test_code()
    };
    assert_eq!(cycles().evaluate(&LAYERED).unwrap().len(), 1);
    cycles()
        .ignoring_use(any_of(["crate::domain::price"]), any_of(["crate::api"]))
        .check(&LAYERED);

    let unused = domain()
        .ignoring_use(any_of(["crate::domain::price"]), any_of(["crate::api"]))
        .ignoring_use(any_of(["crate::domain::order"]), any_of(["crate::api"]));
    match unused.verify(&LAYERED) {
        Err(failure @ Failure::Invalid { .. }) => {
            let Failure::Invalid { errors, .. } = &failure else {
                unreachable!()
            };
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].code, ErrorCode::UnusedExemption);
            assert_eq!(
                errors[0].value.as_deref(),
                Some("crate::domain::order -> crate::api")
            );
            assert!(failure
                .to_string()
                .contains("exemption `crate::domain::order -> crate::api` lets no reference pass"));
        }
        other => panic!("{other:?}"),
    }
    match domain()
        .ignoring_use(any_of(["crate::domian"]), any_of(["crate::api::nothing"]))
        .verify(&LAYERED)
    {
        Err(Failure::Invalid { errors, .. }) => {
            let codes: Vec<_> = errors.iter().map(|error| error.code).collect();
            assert_eq!(
                codes,
                [
                    ErrorCode::PatternMatchesNothing,
                    ErrorCode::PatternMatchesNothing
                ]
            );
        }
        other => panic!("{other:?}"),
    }
}

/// A group reports every rule that fails, in the order of the rules. A group without rules
/// cannot fail, and is an error.
#[test]
fn rule_groups() {
    let domain = rule("the domain does not use the api")
        .modules(any_of(["crate::domain::**"]))
        .must_not_use(any_of(["crate::api"]));
    let storage = rule("the storage does not use the domain")
        .modules(any_of(["crate::storage::**"]))
        .must_not_use(any_of(["crate::domain"]));
    let shop = rules("shop", [domain.clone(), storage.clone(), features(all())]);
    match shop.verify(&LAYERED) {
        Err(Failure::Several(failures)) => {
            let failed: Vec<&str> = failures
                .iter()
                .map(|failure| match failure {
                    Failure::Violated { rule, .. } => rule.as_str(),
                    other => panic!("{other:?}"),
                })
                .collect();
            assert_eq!(failed, ["the domain does not use the api", "features"]);
        }
        other => panic!("{other:?}"),
    }

    rules("passes", [storage.clone()]).check(&LAYERED);
    match rules("one fails", [domain, storage]).verify(&LAYERED) {
        Err(Failure::Violated { rule, .. }) => assert_eq!(rule, "the domain does not use the api"),
        other => panic!("{other:?}"),
    }

    let empty = rules("empty", Vec::<Rule>::new()).verify(&LAYERED);
    assert!(matches!(empty, Err(Failure::Malformed { .. })), "{empty:?}");
    assert!(empty.unwrap_err().to_string().contains("no rules"));
}

/// A set of rules of the user: one rule for each part, or a failure when no part is given.
struct KeepsAwayFrom {
    parts: &'static [&'static str],
    target: &'static str,
}

impl refpact::RuleSet for KeepsAwayFrom {
    fn name(&self) -> &str {
        "the parts keep away"
    }

    fn rules(&self) -> Result<Vec<Rule>, Failure> {
        if self.target.is_empty() {
            return Err(Failure::Malformed {
                rule: self.name().to_owned(),
                message: "no target is given".to_owned(),
            });
        }
        let rule_of = |part: &&str| {
            rule(format!("{part} does not use {}", self.target))
                .modules(any_of([format!("crate::{part}::**")]))
                .must_not_use(any_of([self.target]))
                .except_in_test_code()
        };
        Ok(self.parts.iter().map(rule_of).collect())
    }
}

/// A type of the user that gives rules is checked as the groups of this crate are.
#[test]
fn a_rule_set_of_the_user() {
    use refpact::RuleSet;

    let set = |parts, target| KeepsAwayFrom { parts, target };
    set(&["storage"], "crate::api").check(&LAYERED);
    match set(&["storage", "domain"], "crate::api").verify(&LAYERED) {
        Err(Failure::Violated { rule, .. }) => assert_eq!(rule, "domain does not use crate::api"),
        other => panic!("{other:?}"),
    }
    match set(&["domain", "api"], "crate").verify(&LAYERED) {
        Err(Failure::Several(failures)) => assert_eq!(failures.len(), 2),
        other => panic!("{other:?}"),
    }
    // A set without rules cannot fail, and the failure of the set itself comes through.
    let empty = set(&[], "crate::api").verify(&LAYERED).unwrap_err();
    assert_eq!(
        empty.to_string(),
        "rule `the parts keep away` cannot be checked:\n  the group has no rules, so it cannot fail"
    );
    let malformed = set(&["domain"], "").verify(&LAYERED).unwrap_err();
    assert!(malformed.to_string().ends_with("no target is given"));

    // With a baseline, the violations of each rule of the set are accepted.
    let file =
        std::env::temp_dir().join(format!("refpact-rule-set-{}.baseline", std::process::id()));
    let _removed = Removed(file.clone());
    let baseline = Baseline::file(&file);
    let failing = set(&["storage", "domain"], "crate::api");
    for made in failing.rules().unwrap() {
        made.write_baseline(&LAYERED, &baseline).unwrap();
    }
    assert_eq!(failing.verify_with_baseline(&LAYERED, &baseline), Ok(()));
    failing.check_with_baseline(&LAYERED, &baseline);
    assert!(set(&["api"], "crate::domain")
        .verify_with_baseline(&LAYERED, &baseline)
        .is_err());

    // The layers and the groups are rule sets.
    fn name_of(set: &impl RuleSet) -> &str {
        set.name()
    }
    assert_eq!(name_of(&shop()), "shop");
    assert_eq!(name_of(&rules("group", Vec::<Rule>::new())), "group");
    assert_eq!(
        RuleSet::rules(&shop()).unwrap().len(),
        shop().rules().unwrap().len()
    );
}

// ---------------------------------------------------------------------------------------
// Independent slices

fn features(modules: impl Into<refpact::Modules>) -> Rule {
    rule("features")
        .modules(modules)
        .must_be_independent_between(any_of(["crate::features::*"]))
}

/// No slice may use another slice; a part that is not selected is shared.
#[test]
fn independent_slices() {
    assert_eq!(
        uses(&features(all()).evaluate(&LAYERED).unwrap()),
        vec![
            place("src/features/cart.rs", 2, "crate::features::shared::round"),
            place("src/features/cart.rs", 2, "crate::features::pay::fee"),
            place("src/features/pay.rs", 2, "crate::features::shared::round"),
        ]
    );
    let shared = features(all_except(["crate::features::shared"]));
    assert_eq!(
        uses(&shared.evaluate(&LAYERED).unwrap()),
        vec![place(
            "src/features/cart.rs",
            2,
            "crate::features::pay::fee"
        )]
    );
    let one = features(any_of(["crate::features::cart"]))
        .evaluate(&LAYERED)
        .unwrap_err();
    assert_eq!(one[0].code, ErrorCode::TooFewNodes);
}

// ---------------------------------------------------------------------------------------
// Unused modules

/// A module that nothing outside it uses is reported, without a line.
#[test]
fn unused_modules() {
    let found = rule("used")
        .modules(all_except(["crate"]))
        .must_be_used()
        .evaluate(&LAYERED)
        .unwrap();
    // `dead::inner` is used by `dead`, and `api` is used through `api::http`.
    assert_eq!(
        units(&found),
        [
            "crate::dead",
            "crate::storage::memory::tests",
            "crate::tools"
        ]
    );
    assert!(found
        .iter()
        .all(|violation| violation.detail == Detail::Unused && violation.line == 0));
    let failure = rule("used")
        .modules(any_of(["crate::tools", "crate::api"]))
        .must_be_used()
        .verify(&LAYERED)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `used` is violated in 1 place(s):\n  src/tools.rs: [used] crate::tools is not used from outside"
    );
}

// ---------------------------------------------------------------------------------------
// Chains

/// A chain is reported with its steps; a direct use is a chain without steps; a chain can
/// lead to a module.
#[test]
fn chains() {
    let found = rule("the api does not reach the file system")
        .modules(any_of(["crate::api::**"]))
        .must_not_reach(any_of(["std::fs"]))
        .verify(&LAYERED)
        .unwrap_err();
    assert_eq!(
        found.to_string(),
        "rule `the api does not reach the file system` is violated in 1 place(s):\n  \
         src/api/http.rs:1:27: [the api does not reach the file system] crate::api::http reaches std::fs::read in 3 step(s):\n      \
         crate::api::http -> crate::domain::order  at src/api/http.rs:1:27, uses crate::domain::order::Order (use)\n      \
         crate::domain::order -> crate::storage::disk  at src/domain/order.rs:1:21, uses crate::storage::disk (use)\n      \
         crate::storage::disk uses std::fs::read  at src/storage/disk.rs:2:13 (expr)"
    );

    let direct = rule("r")
        .modules(any_of(["crate::storage::disk"]))
        .must_not_reach(any_of(["std::fs"]))
        .evaluate(&LAYERED)
        .unwrap();
    match &direct[0].detail {
        Detail::Reaches { steps, end } => {
            assert!(steps.is_empty());
            assert_eq!(
                (end.unit.join("::"), end.line),
                ("crate::storage::disk".to_owned(), 2)
            );
        }
        other => panic!("{other:?}"),
    }
    assert_eq!(
        (direct[0].file.as_str(), direct[0].line),
        ("src/storage/disk.rs", 2)
    );

    let to_module = rule("r")
        .modules(any_of(["crate"]))
        .must_not_reach(any_of(["crate::storage::disk"]))
        .evaluate(&LAYERED)
        .unwrap();
    match &to_module[0].detail {
        Detail::Reaches { steps, end } => {
            let path: Vec<String> = steps.iter().map(|step| step.to.join("::")).collect();
            assert_eq!(path, ["crate::api::http", "crate::domain::order"]);
            assert_eq!(end.target.join("::"), "crate::storage::disk");
        }
        other => panic!("{other:?}"),
    }
}

// ---------------------------------------------------------------------------------------
// Reachable from the roots

/// A module that no root reaches is reported. A rule that cannot fail is an error.
#[test]
fn reachable_from_the_roots() {
    let live = rule("live")
        .modules(all_except(["**::tests"]))
        .must_be_reachable_from(any_of(["crate"]));
    let found = live.evaluate(&LAYERED).unwrap();
    // `dead::inner` is used, but only by `dead`, which nothing reaches.
    assert_eq!(
        units(&found),
        [
            "crate::dead",
            "crate::dead::inner",
            "crate::storage::memory",
            "crate::tools"
        ]
    );
    assert!(found
        .iter()
        .all(|violation| violation.detail == Detail::Unreachable));

    let only_roots = rule("r")
        .modules(any_of(["crate"]))
        .must_be_reachable_from(any_of(["crate"]))
        .evaluate(&LAYERED);
    assert_eq!(only_roots.unwrap_err()[0].code, ErrorCode::NoSubjects);
    let typo = rule("r")
        .modules(all())
        .must_be_reachable_from(any_of(["crate::mian"]))
        .evaluate(&LAYERED)
        .unwrap_err();
    assert_eq!(
        (typo[0].code, typo[0].value.as_deref()),
        (ErrorCode::PatternMatchesNothing, Some("crate::mian"))
    );
}

#[test]
fn the_new_violations_have_baseline_keys() {
    let file = std::env::temp_dir().join(format!("refpact-{}-graph.baseline", std::process::id()));
    let _removed = Removed(file.clone());
    let _ = std::fs::remove_file(&file);
    let baseline = Baseline::file(&file);
    let used = rule("used")
        .modules(any_of(["crate::tools", "crate::api"]))
        .must_be_used();
    let reach = rule("reach")
        .modules(any_of(["crate::api::**"]))
        .must_not_reach(any_of(["std::fs"]));
    let live = rule("live")
        .modules(any_of(["crate::tools", "crate::api"]))
        .must_be_reachable_from(any_of(["crate"]));
    for rule in [&used, &reach, &live] {
        rule.write_baseline(&LAYERED, &baseline).unwrap();
        rule.check_with_baseline(&LAYERED, &baseline);
    }
    let text = std::fs::read_to_string(&file).unwrap();
    let lines: Vec<&str> = text.lines().filter(|line| !line.starts_with('#')).collect();
    assert_eq!(
        lines,
        [
            "live\tcrate::tools\tunreachable\t1",
            "reach\tcrate::api::http\treaches std::fs::read\t1",
            "used\tcrate::tools\tunused\t1",
        ]
    );
}
