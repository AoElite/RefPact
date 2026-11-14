//! Rules across the crates of a workspace, on `fixtures/rust/workspace`.

use std::sync::LazyLock;

use refpact::{all, any_of, rule, Detail, ErrorCode, LoadError, Rule, Workspace};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rust/workspace");

static SHOP: LazyLock<Workspace> = LazyLock::new(|| {
    Workspace::options(FIXTURE)
        .resolving_members()
        .load()
        .unwrap()
});

/// File, line and path of each violation, with the path as written when it differs.
fn uses(rule: &Rule) -> Vec<(String, u32, String)> {
    rule.evaluate(&SHOP)
        .unwrap()
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Uses {
                target, written, ..
            } => {
                let mut path = target.join("::");
                if let Some(written) = written {
                    path.push_str(&format!(" written={}", written.join("::")));
                }
                (violation.file.clone(), violation.line, path)
            }
            other => panic!("not a use: {other:?}"),
        })
        .collect()
}

fn owned(list: &[(&str, u32, &str)]) -> Vec<(String, u32, String)> {
    list.iter()
        .map(|(file, line, path)| ((*file).to_owned(), *line, (*path).to_owned()))
        .collect()
}

/// The members come from the manifest or can be named; a module is named by its crate.
#[test]
fn the_members_and_modules_of_a_workspace() {
    let members: Vec<(&str, &str)> = SHOP
        .members()
        .iter()
        .map(|member| (member.name.as_str(), member.dir.as_str()))
        .collect();
    // `crates/*` with `exclude`. The name and the root of `shop` are set in `[lib]`, and
    // `shop-domain` is written `shop_domain` in code.
    assert_eq!(
        members,
        [
            ("shop", "crates/app"),
            ("shop_domain", "crates/domain"),
            ("shop_storage", "crates/storage")
        ]
    );

    assert_eq!(
        SHOP.modules().collect::<Vec<_>>(),
        [
            "shop",
            "shop::cart",
            "shop_domain",
            "shop_domain::money",
            "shop_domain::order",
            "shop_domain::tests",
            "shop_storage",
            "shop_storage::db",
            "shop_storage::stored"
        ]
    );

    let two = Workspace::options(FIXTURE)
        .resolving_members()
        .members(["crates/storage", "./crates/domain/"])
        .load()
        .unwrap();
    assert_eq!(
        two.members()
            .iter()
            .map(|member| member.name.as_str())
            .collect::<Vec<_>>(),
        ["shop_storage", "shop_domain"]
    );
    assert_eq!(two.modules().next().as_deref(), Some("shop_storage"));
}

/// Rules across crates: the files are relative to the workspace, a re-export of another
/// crate is followed to the definition, a target in another crate must exist, and each crate
/// can be a slice.
#[test]
fn rules_across_crates() {
    {
        let rule = rule("the domain does not use storage")
            .modules(any_of(["shop_domain::**"]))
            .must_not_use(any_of(["shop_storage"]));
        // The files are relative to the workspace.
        assert_eq!(
            uses(&rule),
            owned(&[
                ("crates/domain/src/lib.rs", 11, "shop_storage::db::Db"),
                (
                    "crates/domain/src/lib.rs",
                    15,
                    "shop_storage::db::Db::default"
                ),
            ])
        );
        rule.except_in_test_code().check(&SHOP);
    }
    {
        let rule = rule("the app does not reach into the order module")
            .modules(any_of(["shop::**"]))
            .must_not_use(any_of(["shop_domain::order"]));
        assert_eq!(
            uses(&rule),
            owned(&[
                (
                    "crates/app/src/cart.rs",
                    1,
                    "shop_domain::order::Order written=shop_domain::Order"
                ),
                (
                    "crates/app/src/cart.rs",
                    3,
                    "shop_domain::order::Order written=shop_domain::Order"
                ),
                // A method call, which rust-analyzer resolves.
                (
                    "crates/app/src/cart.rs",
                    4,
                    "shop_domain::order::Order::total"
                ),
            ])
        );
    }
    {
        let errors = rule("typo")
            .modules(any_of(["shop::**"]))
            .must_not_use(any_of(["shop_domain::ordre"]))
            .evaluate(&SHOP)
            .unwrap_err();
        assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
        assert_eq!(errors[0].value.as_deref(), Some("shop_domain::ordre"));
    }
    {
        let only = |allowed: &[&str]| {
            rule("storage uses the domain and nothing else of the workspace")
                .modules(any_of(["shop_storage::**"]))
                .must_only_use(any_of(allowed))
        };
        only(&["std", "shop_storage", "shop_domain"]).check(&SHOP);
        // Without the root of the domain in the list, the definition behind `shop_domain::Order`
        // still allows it.
        only(&["std", "shop_storage", "shop_domain::order"]).check(&SHOP);
        assert_eq!(
            uses(&only(&["std", "shop_storage", "shop_domain::money"])),
            owned(&[
                ("crates/storage/src/db.rs", 3, "shop_domain::Order"),
                ("crates/storage/src/db.rs", 7, "shop_domain::Order"),
                ("crates/storage/src/db.rs", 11, "shop_domain::order::Order"),
                ("crates/storage/src/stored.rs", 5, "shop_domain::Order"),
            ])
        );
    }
    {
        // Each crate is a slice: the first segment of the path.
        let rule = rule("no cycles between crates")
            .modules(all())
            .must_be_acyclic_between(any_of(["*"]));
        let found = rule.evaluate(&SHOP).unwrap();
        assert_eq!(found.len(), 1);
        match &found[0].detail {
            Detail::Cycle { steps, size } => {
                let text: Vec<String> = steps
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
                    .collect();
                assert_eq!(
                    text,
                    [
                        "shop_domain -> shop_storage at crates/domain/src/lib.rs:11",
                        "shop_storage -> shop_domain at crates/storage/src/db.rs:3"
                    ]
                );
                assert_eq!(*size, 2);
            }
            other => panic!("{other:?}"),
        }
        // The domain uses storage in its tests only, as a dev-dependency.
        rule.except_in_test_code().check(&SHOP);
    }
}

/// A directory that is not a workspace, a member that is not there, and an error in a member.
#[test]
fn workspace_errors() {
    let no_table = Workspace::options(format!("{FIXTURE}/crates/domain"))
        .resolving_members()
        .load()
        .unwrap_err();
    let message = no_table.to_string();
    let start = "Cargo.toml: the directory is not the directory of its workspace, which is ";
    assert!(message.starts_with(start), "{message}");
    assert!(message.ends_with("workspace"), "{message}");

    let no_file = Workspace::options(format!("{FIXTURE}/crates"))
        .resolving_members()
        .load()
        .unwrap_err();
    assert!(matches!(no_file, LoadError::Manifest { .. }), "{no_file}");

    let no_crate = Workspace::options(FIXTURE)
        .resolving_members()
        .members(["crates/nothing"])
        .load()
        .unwrap_err();
    assert_eq!(
        no_crate.to_string(),
        "crates/nothing/Cargo.toml: Cargo names no member of the workspace in this directory"
    );

    let error = Workspace::options(FIXTURE)
        .resolving_members()
        .allow_unreached_files(["crates/domain/src/generated.rs"])
        .load()
        .unwrap_err();
    assert!(
        matches!(error, LoadError::Member { ref dir, .. } if dir == "crates/domain"),
        "{error}"
    );

    let nowhere = Workspace::options(FIXTURE)
        .resolving_members()
        .allow_unreached_files(["tools/x.rs"])
        .load()
        .unwrap_err();
    assert!(
        matches!(nowhere, LoadError::StaleAllowance { .. }),
        "{nowhere}"
    );
}

/// Items in a workspace are named by their crate, and their files are relative to the
/// workspace.
#[test]
fn items_across_crates() {
    let violations = rule("structs derive Debug")
        .items(all())
        .of_kind([refpact::ItemKind::Struct])
        .must_derive(any_of(["std::fmt::Debug"]))
        .evaluate(&SHOP)
        .unwrap();
    let found: Vec<(String, String)> = violations
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Item { item, .. } => (violation.file.clone(), item.join("::")),
            other => panic!("{other:?}"),
        })
        .collect();
    assert!(
        found.contains(&(
            "crates/storage/src/db.rs".to_owned(),
            "shop_storage::db::Db".to_owned()
        )),
        "{found:?}"
    );
    assert!(!found
        .iter()
        .any(|(_, item)| item == "shop_domain::order::Order"));
    rule("the order lives in the domain")
        .items(all())
        .named(["Order"])
        .must_reside_in(any_of(["shop_domain::order"]))
        .check(&SHOP);
    // An `impl` in one member for a type of another, named through a re-export.
    rule("the order is stored")
        .items(any_of(["shop_domain::order"]))
        .named(["Order"])
        .must_implement(any_of(["shop_storage::stored::Stored"]))
        .check(&SHOP);
    rule("the exports of the domain")
        .items(any_of(["shop_domain::**"]))
        .exported()
        .must_be_named(["Order", "Money"])
        .check(&SHOP);
    // The uses inside an item carry the item, with the name of its member.
    let uses = rule("the database touches no file")
        .items(any_of(["shop_storage::db"]))
        .named(["Db"])
        .must_not_use(any_of(["std::fs"]))
        .evaluate(&SHOP)
        .unwrap();
    let items: Vec<String> = uses
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Uses {
                item: Some(item), ..
            } => item.join("::"),
            other => panic!("{other:?}"),
        })
        .collect();
    assert_eq!(items, ["shop_storage::db::Db::save"]);
}
