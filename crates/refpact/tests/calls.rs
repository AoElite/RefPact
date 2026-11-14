//! The method calls and the field accesses that rust-analyzer resolves, which every load from
//! disk does unless it is `syntax_only`.
//!
//! These tests run `rust-analyzer scip` on the fixture `fixtures/rust/calls`. They fail when
//! rust-analyzer is not installed: `rustup component add rust-analyzer`.

use std::sync::LazyLock;

use refpact::core::What;
use refpact::{all, any_of, rule, Crate, Failure, Kind, LoadError, Workspace};

fn fixture(path: &str) -> String {
    format!(
        "{}/../../fixtures/rust/calls{path}",
        env!("CARGO_MANIFEST_DIR")
    )
}

static SHOP: LazyLock<Workspace> = LazyLock::new(|| {
    Workspace::options(fixture(""))
        .resolving_members()
        .load()
        .unwrap()
});

static DOMAIN: LazyLock<Crate> = LazyLock::new(|| {
    Crate::options(fixture("/domain"))
        .resolving_members()
        .load()
        .unwrap()
});

/// The references of the two kinds in a file: `line:column kind target [aliases] (note)`.
fn resolved(code: &Crate, file: &str) -> Vec<String> {
    let facts = code.facts().facts.iter();
    facts
        .filter(|fact| fact.file == file)
        .filter_map(|fact| {
            let What::Reference(reference) = &fact.what else {
                return None;
            };
            if !matches!(reference.kind.as_str(), "method-call" | "field") {
                return None;
            }
            let mut text = format!(
                "{}:{} {} {}",
                fact.line,
                fact.column,
                reference.kind,
                reference.target.join("::")
            );
            for alias in &reference.aliases {
                text.push_str(&format!(" [{}]", alias.join("::")));
            }
            if let Some(note) = &fact.note {
                text.push_str(&format!(" ({note})"));
            }
            Some(text)
        })
        .collect()
}

#[test]
fn the_calls_and_the_fields_of_a_file_are_references() {
    assert_eq!(
        resolved(&SHOP, "app/src/api.rs"),
        [
            "5:22 method-call shop_app::storage::Pool::load",
            "7:10 method-call std::collections::hash::map::HashMap::insert",
            "7:23 field shop_domain::Order::id",
            "7:33 method-call alloc::vec::Vec::len [std::vec::Vec::len]",
            "9:25 method-call shop_domain::Order::total",
            "10:11 method-call shop_domain::Order::price [shop_domain::Priced::price] (of the trait `Priced`)",
            "10:27 method-call shop_domain::Order::discount",
            "10:50 method-call shop_domain::Priced::doubled",
            "10:61 method-call alloc::vec::Vec::len [std::vec::Vec::len]",
            "16:10 field shop_app::api::Pair::0",
        ]
    );
}

#[test]
fn a_call_inside_a_type_and_through_a_bound_is_resolved() {
    assert_eq!(
        resolved(&DOMAIN, "src/lib.rs"),
        [
            "14:14 field crate::Order::total",
            "26:14 method-call crate::Priced::price",
        ]
    );
    assert_eq!(
        resolved(&DOMAIN, "src/pricing.rs"),
        [
            "5:14 method-call crate::Order::total",
            "11:14 field crate::Order::total",
            "11:27 method-call crate::Order::discount",
            "16:10 method-call crate::Priced::price",
        ]
    );
}

#[test]
fn self_paths_are_resolved_with_the_kind_of_the_path() {
    let found: Vec<String> = DOMAIN
        .facts()
        .facts
        .iter()
        .filter(|fact| fact.file == "src/lib.rs" && fact.line == 18)
        .filter_map(|fact| match &fact.what {
            What::Reference(reference) => Some(format!(
                "{} {}",
                reference.kind,
                reference.target.join("::")
            )),
            What::Marker(_) => None,
        })
        .collect();
    assert_eq!(found, ["expr crate::Order::new"]);
}

/// The places of the violations of a rule: `file:line target`.
fn violations(rule: refpact::Rule, code: &Crate) -> Vec<String> {
    match rule.verify(code) {
        Ok(()) => Vec::new(),
        Err(Failure::Violated { violations, .. }) => violations
            .iter()
            .map(|violation| match &violation.detail {
                refpact::Detail::Uses { target, .. } => {
                    format!(
                        "{}:{} {}",
                        violation.file,
                        violation.line,
                        target.join("::")
                    )
                }
                other => panic!("unexpected {other:?}"),
            })
            .collect(),
        Err(other) => panic!("{other}"),
    }
}

#[test]
fn a_rule_about_uses_sees_the_method_calls() {
    // The path of the type is in the code. The call of the method is not.
    let found = violations(
        rule("the storage does not read files to their end")
            .modules(any_of(["shop_app::storage"]))
            .must_not_use(any_of(["std::fs::File::read_to_end"])),
        &SHOP,
    );
    assert_eq!(found, ["app/src/storage.rs:9 std::fs::File::read_to_end"]);

    let found = violations(
        rule("the api does not ask an order for its total")
            .modules(any_of(["shop_app::api"]))
            .must_not_use(any_of(["shop_domain::Order::total"]))
            .only_kinds([Kind::MethodCall]),
        &SHOP,
    );
    assert_eq!(found, ["app/src/api.rs:9 shop_domain::Order::total"]);
}

#[test]
fn a_call_of_a_trait_method_is_a_use_of_the_trait() {
    let found = violations(
        rule("the api does not price orders")
            .modules(any_of(["shop_app::api"]))
            .must_not_use(any_of(["shop_domain::Priced::price"]))
            .only_kinds([Kind::MethodCall]),
        &SHOP,
    );
    assert_eq!(found, ["app/src/api.rs:10 shop_domain::Priced::price"]);
}

#[test]
fn a_method_of_the_standard_library_is_found_under_std() {
    let found = violations(
        rule("no code asks a vector for its length")
            .modules(any_of(["shop_app::api"]))
            .must_not_use(any_of(["std::vec::Vec::len"]))
            .only_kinds([Kind::MethodCall]),
        &SHOP,
    );
    assert_eq!(
        found,
        [
            "app/src/api.rs:7 std::vec::Vec::len",
            "app/src/api.rs:10 std::vec::Vec::len"
        ]
    );
}

#[test]
fn the_filters_of_a_rule_apply_to_the_calls() {
    let rule = |name: &str| {
        rule(name)
            .modules(all())
            .must_not_use(any_of(["shop_app::storage::Pool::load"]))
            .only_kinds([Kind::MethodCall])
    };
    assert_eq!(
        violations(rule("r"), &SHOP),
        [
            "app/src/api.rs:5 shop_app::storage::Pool::load",
            "app/src/storage.rs:28 shop_app::storage::Pool::load"
        ]
    );
    assert_eq!(
        violations(rule("r").except_in_test_code(), &SHOP),
        ["app/src/api.rs:5 shop_app::storage::Pool::load"]
    );
}

#[test]
fn the_code_of_a_macro_body_is_not_resolved() {
    // `read_all!(path)` calls `std::fs::read`, which the index does not say.
    let found = violations(
        rule("r")
            .modules(any_of(["shop_app::storage"]))
            .must_not_use(any_of(["std::fs::read"]))
            .only_kinds([Kind::MethodCall, Kind::Expr]),
        &SHOP,
    );
    assert_eq!(found, Vec::<String>::new());
}

#[test]
fn a_rule_about_calls_is_an_error_for_code_without_them() {
    let plain = Workspace::options(fixture("")).load().unwrap();
    let failure = rule("the api does not ask an order for its total")
        .modules(any_of(["shop_app::api"]))
        .must_not_use(any_of(["shop_domain::Order"]))
        .only_kinds([Kind::MethodCall])
        .verify(&plain)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `the api does not ask an order for its total` cannot be checked:\n  \
         kind `method-call` is not known to this front end"
    );
    // And the facts of the other kinds are those of a load without the index. The index adds
    // the definition behind a path to its aliases, proves the name behind a glob import, and
    // leaves out a reading of a name that is wrong.
    let other = |code: &Crate| -> Vec<_> {
        let facts = code.facts().facts.iter();
        facts
            .filter(|fact| match &fact.what {
                What::Reference(reference) => {
                    !matches!(reference.kind.as_str(), "method-call" | "field")
                        && !(fact.file == "domain/src/lib.rs" && fact.line == 18)
                        && reference.certainty.as_str() == "exact"
                }
                What::Marker(_) => true,
            })
            .map(|fact| {
                let mut fact = fact.clone();
                if let What::Reference(reference) = &mut fact.what {
                    reference.aliases.clear();
                }
                fact.note = None;
                fact
            })
            .collect()
    };
    let (resolved, syntax) = (other(&SHOP), other(&plain));
    let proved = |fact: &&refpact::core::Fact| !syntax.contains(fact);
    let proved: Vec<_> = resolved.iter().filter(proved).collect();
    assert_eq!(proved.len(), 1, "{proved:#?}");
    assert_eq!(
        (proved[0].file.as_str(), proved[0].line),
        ("app/src/api.rs", 6)
    );
    assert!(syntax.iter().all(|fact| resolved.contains(fact)));
}

#[test]
fn a_directory_that_cargo_cannot_read_is_a_load_error() {
    let dir = std::env::temp_dir().join(format!("refpact-calls-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    let error = Crate::options(&dir).resolving_members().load().unwrap_err();
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(matches!(error, LoadError::Index { .. }), "{error}");
}

#[test]
fn a_call_of_a_trait_can_count_as_a_call_of_each_implementation() {
    let followed = Crate::options(fixture("/domain"))
        .following_implementations()
        .load()
        .unwrap();
    assert_eq!(
        resolved(&followed, "src/pricing.rs"),
        [
            "5:14 method-call crate::Order::total",
            "11:14 field crate::Order::total",
            "11:27 method-call crate::Order::discount",
            "16:10 method-call crate::Priced::price",
            "16:10 method-call crate::Order::price (may be called through `crate::Priced::price`)",
        ]
    );
    let candidates = followed
        .facts()
        .facts
        .iter()
        .filter(|fact| match &fact.what {
            What::Reference(reference) => reference.certainty.as_str() == "candidate",
            What::Marker(_) => false,
        });
    assert_eq!(candidates.count(), 2);
    // It is not the default: the code that calls a trait does not use what implements it.
    assert_eq!(resolved(&DOMAIN, "src/pricing.rs").len(), 4);

    // In a workspace, the implementation can be in another crate than the call.
    let shop = Workspace::options(fixture(""))
        .following_implementations()
        .load()
        .unwrap();
    let through: Vec<String> = resolved(&shop, "app/src/api.rs")
        .into_iter()
        .filter(|text| text.contains("may be called"))
        .collect();
    assert_eq!(through, Vec::<String>::new());
    let through: Vec<String> = resolved(&shop, "domain/src/pricing.rs")
        .into_iter()
        .filter(|text| text.contains("may be called"))
        .collect();
    assert_eq!(
        through,
        ["16:10 method-call shop_domain::Order::price (may be called through `shop_domain::Priced::price`)"]
    );
    let calls = rule("the pricing calls no order")
        .modules(any_of(["shop_domain::pricing"]))
        .must_not_use(any_of(["shop_domain::Order::price"]));
    assert_eq!(calls.clone().evaluate(&shop).unwrap().len(), 1);
    assert_eq!(
        calls.ignoring_candidates().evaluate(&shop).unwrap().len(),
        0
    );
}
