//! Rules about items: what the front end declares, and the rules on top of it.

use std::sync::LazyLock;

use refpact::{
    all, all_except, any_of, rule, Crate, Detail, ErrorCode, Failure, ItemKind, Violation,
    Visibility,
};

static SHOP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            (
                "src/lib.rs",
                "pub mod domain;\n\
                 pub mod storage;\n\
                 mod internal;\n\
                 pub mod prelude {\n\
                     pub use crate::domain::Repository as Repo;\n\
                 }\n",
            ),
            (
                "src/domain.rs",
                "pub trait Repository {}\n\
                 \n\
                 #[derive(Debug, Clone)]\n\
                 #[non_exhaustive]\n\
                 pub struct Order;\n\
                 \n\
                 pub struct MemoryRepository;\n\
                 impl Repository for MemoryRepository {}\n",
            ),
            (
                "src/storage.rs",
                "use crate::prelude::Repo;\n\
                 \n\
                 #[derive(Debug)]\n\
                 pub struct PgRepository;\n\
                 \n\
                 pub(crate) struct Cache;\n\
                 \n\
                 impl Repo for PgRepository {}\n\
                 impl crate::domain::Repository for Cache {}\n\
                 \n\
                 pub(super) fn open() {\n\
                     struct Inner;\n\
                 }\n",
            ),
            (
                "src/internal.rs",
                "pub fn helper() {}\n\
                 pub(self) const LIMIT: u32 = 3;\n\
                 #[macro_export]\n\
                 macro_rules! shout { () => {} }\n\
                 #[cfg(test)]\n\
                 pub fn fixture() {}\n",
            ),
        ],
    )
    .unwrap()
});

/// The items of the violations, joined.
fn items(violations: &[Violation]) -> Vec<String> {
    violations
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Item { item, .. } => item.join("::"),
            other => panic!("not an item: {other:?}"),
        })
        .collect()
}

/// The front end declares the items of each module with kind, visibility and relations.
#[test]
fn the_declarations_of_a_crate() {
    let declared: Vec<String> = SHOP
        .facts()
        .declarations
        .iter()
        .map(|declaration| {
            let mut relations: Vec<String> = declaration
                .relations
                .iter()
                .map(|tie| format!("{} {}", tie.relation, tie.target.join("::")))
                .collect();
            relations.sort();
            format!(
                "{} {} {} {}:{} [{}]{}",
                declaration.path.join("::"),
                declaration.kind,
                declaration.visibility,
                declaration.file,
                declaration.line,
                relations.join(", "),
                if declaration.tags.is_empty() {
                    ""
                } else {
                    " test"
                },
            )
        })
        .collect();
    assert_eq!(
        declared,
        [
            "crate::domain::Repository trait pub src/domain.rs:1 [exported crate::domain::Repository, exported crate::prelude::Repo]",
            "crate::domain::Order struct pub src/domain.rs:5 [attribute non_exhaustive, derives std::clone::Clone, derives std::fmt::Debug, exported crate::domain::Order]",
            "crate::domain::MemoryRepository struct pub src/domain.rs:7 [exported crate::domain::MemoryRepository, implements crate::domain::Repository]",
            "crate::storage::PgRepository struct pub src/storage.rs:4 [derives std::fmt::Debug, exported crate::storage::PgRepository, implements crate::domain::Repository, implements crate::prelude::Repo]",
            "crate::storage::Cache struct pub(crate) src/storage.rs:6 [implements crate::domain::Repository]",
            "crate::storage::open fn pub(restricted) src/storage.rs:11 []",
            "crate::internal::helper fn pub src/internal.rs:1 []",
            "crate::internal::LIMIT const private src/internal.rs:2 []",
            "crate::shout macro pub src/internal.rs:4 [attribute macro_export, exported crate::shout]",
            "crate::internal::fixture fn pub src/internal.rs:6 [attribute cfg] test",
        ]
    );
}

#[test]
fn items_that_implement_a_trait() {
    let repositories = || {
        rule("repositories")
            .items(all())
            .implementing(any_of(["crate::domain::Repository"]))
    };
    // A trait through a re-export counts: `Repo` is `Repository`.
    assert_eq!(
        items(
            &repositories()
                .must_reside_in(any_of(["crate::storage"]))
                .evaluate(&SHOP)
                .unwrap()
        ),
        ["crate::domain::MemoryRepository"]
    );
    assert_eq!(
        items(
            &repositories()
                .must_be_named(["*Repository"])
                .evaluate(&SHOP)
                .unwrap()
        ),
        ["crate::storage::Cache"]
    );
    rule("the order is a value")
        .items(any_of(["crate::domain"]))
        .named(["Order"])
        .must_derive(any_of(["std::fmt::Debug"]))
        .check(&SHOP);
    assert_eq!(
        items(
            &rule("structs derive Debug")
                .items(all())
                .of_kind([ItemKind::Struct])
                .must_derive(any_of(["std::fmt::Debug"]))
                .evaluate(&SHOP)
                .unwrap()
        ),
        ["crate::domain::MemoryRepository", "crate::storage::Cache"]
    );
    assert_eq!(
        items(
            &rule("repositories implement the trait")
                .items(all())
                .named(["*Repository"])
                .of_kind([ItemKind::Struct])
                .must_implement(any_of(["crate::domain::Repository"]))
                .evaluate(&SHOP)
                .unwrap()
        ),
        Vec::<String>::new()
    );
}

#[test]
fn visibility_attributes_and_test_code() {
    let public = || {
        rule("nothing in internal is pub")
            .items(any_of(["crate::internal"]))
            .with_visibility([Visibility::Pub])
            .must_not_exist()
    };
    assert_eq!(
        items(&public().evaluate(&SHOP).unwrap()),
        [
            "crate::internal::helper",
            "crate::shout",
            "crate::internal::fixture"
        ]
    );
    assert_eq!(
        items(&public().except_in_test_code().evaluate(&SHOP).unwrap()),
        ["crate::internal::helper", "crate::shout"]
    );
    rule("public enums and structs of the domain are non-exhaustive")
        .items(any_of(["crate::domain"]))
        .named(["Order"])
        .must_have_attribute(any_of(["non_exhaustive"]))
        .check(&SHOP);
    assert_eq!(
        items(
            &rule("deriving Clone")
                .items(all_except(["crate::domain"]))
                .deriving(any_of(["std::clone::Clone"]))
                .must_not_exist()
                .evaluate(&SHOP)
                .unwrap()
        ),
        Vec::<String>::new()
    );
    assert_eq!(
        items(
            &rule("macros")
                .items(all())
                .with_attribute(any_of(["macro_export"]))
                .of_kind([ItemKind::Macro, ItemKind::Fn])
                .with_visibility([Visibility::Pub, Visibility::Private])
                .must_not_exist()
                .evaluate(&SHOP)
                .unwrap()
        ),
        ["crate::shout"]
    );
}

/// A filter whose pattern matches nothing is an error, and the report names the item.
#[test]
fn errors_and_reports() {
    match rule("typo")
        .items(all())
        .implementing(any_of(["crate::domain::Repositry"]))
        .named(["Nothing*"])
        .must_not_exist()
        .verify(&SHOP)
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
    let failure = rule("caches live in the domain")
        .items(all())
        .named(["Cache"])
        .must_reside_in(any_of(["crate::domain"]))
        .verify(&SHOP)
        .unwrap_err()
        .to_string();
    assert!(
        failure.contains(
            "src/storage.rs:6:19: [caches live in the domain] crate::storage declares struct crate::storage::Cache"
        ),
        "{failure}"
    );
}

/// An item is exported when other crates can name it: through `pub mod` and `pub use` from
/// the crate root.
#[test]
fn exported_items() {
    let code = Crate::from_memory(
        "src/lib.rs",
        &[
            (
                "src/lib.rs",
                "pub mod api;\n\
                 mod engine;\n\
                 pub(crate) mod tools;\n\
                 pub use engine::Engine;\n\
                 pub use engine::parts as engine_parts;\n\
                 pub use tools::*;\n\
                 use engine::Hidden;\n\
                 use secret::*;\n\
                 mod secret {\n\
                     pub fn key() {}\n\
                 }\n\
                 #[cfg(not(feature = \"extra\"))]\n\
                 mod extra;\n\
                 #[cfg(feature = \"extra\")]\n\
                 pub mod extra;\n",
            ),
            ("src/extra.rs", "pub fn more() {}\n"),
            (
                "src/api.rs",
                "pub struct Request;\n\
                 pub(crate) struct Token;\n\
                 pub use self::inner::*;\n\
                 mod inner {\n\
                     pub fn handle() {}\n\
                     pub use super::*;\n\
                 }\n",
            ),
            (
                "src/engine.rs",
                "pub struct Engine;\n\
                 pub struct Hidden;\n\
                 pub mod parts {\n\
                     pub struct Gear;\n\
                 }\n",
            ),
            ("src/tools.rs", "pub fn hammer() {}\nfn saw() {}\n"),
        ],
    )
    .unwrap();
    let exported: Vec<String> = code
        .facts()
        .declarations
        .iter()
        .map(|declaration| {
            let at: Vec<String> = declaration
                .relations
                .iter()
                .filter(|tie| tie.relation == "exported")
                .map(|tie| tie.target.join("::"))
                .collect();
            format!("{} [{}]", declaration.path.join("::"), at.join(", "))
        })
        .collect();
    assert_eq!(
        exported,
        [
            "crate::secret::key []",
            "crate::api::Request [crate::api::Request]",
            "crate::api::Token []",
            "crate::api::inner::handle [crate::api::handle]",
            "crate::engine::Engine [crate::Engine]",
            "crate::engine::Hidden []",
            "crate::engine::parts::Gear [crate::engine_parts::Gear]",
            "crate::tools::hammer [crate::hammer]",
            "crate::tools::saw []",
            "crate::extra::more [crate::extra::more]",
        ]
    );
    assert_eq!(
        items(
            &rule("pub items are exported")
                .items(all())
                .with_visibility([Visibility::Pub])
                .must_be_exported()
                .evaluate(&code)
                .unwrap()
        ),
        ["crate::secret::key", "crate::engine::Hidden"]
    );
    assert_eq!(
        items(
            &rule("the engine exports nothing")
                .items(any_of(["crate::engine::**"]))
                .exported()
                .must_not_exist()
                .evaluate(&code)
                .unwrap()
        ),
        ["crate::engine::Engine", "crate::engine::parts::Gear"]
    );
}
