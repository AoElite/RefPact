//! Fields and methods, the uses inside items, the ready-made rules, the onion architecture
//! and the reasons of rules.

use std::sync::LazyLock;

use refpact::{
    all, any_of, onion_architecture, rule, rules, unsafe_code, Crate, Detail, Failure, ItemKind,
    Violation, Visibility,
};

static SHOP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod api;\npub mod db;\npub mod domain;\n"),
            (
                "src/db.rs",
                "pub struct Pool;\n\
                 pub struct Id(pub u64);\n\
                 pub struct Error;\n\
                 \n\
                 impl crate::domain::Order {\n\
                     pub fn save(&self, pool: &Pool) -> Result<(), Error> {\n\
                         let _ = Pool;\n\
                         Ok(())\n\
                     }\n\
                 }\n",
            ),
            (
                "src/domain.rs",
                "use crate::db::Id;\n\
                 \n\
                 #[derive(Debug)]\n\
                 pub struct Order {\n\
                     pub id: Id,\n\
                     total: u64,\n\
                 }\n\
                 \n\
                 impl Order {\n\
                     pub fn new(id: Id) -> Order {\n\
                         Order { id, total: 0 }\n\
                     }\n\
                     fn total(&self) -> u64 {\n\
                         self.total\n\
                     }\n\
                 }\n\
                 \n\
                 pub trait Priced {\n\
                     fn price(&self) -> crate::db::Error;\n\
                 }\n\
                 \n\
                 impl std::fmt::Display for Order {\n\
                     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {\n\
                         write!(f, \"order\")\n\
                     }\n\
                 }\n\
                 \n\
                 pub fn load<T>(value: T) -> Result<T, crate::db::Error> {\n\
                     todo!()\n\
                 }\n\
                 \n\
                 #[cfg(test)]\n\
                 mod tests {\n\
                     #[test]\n\
                     fn prints() {\n\
                         println!(\"test output\");\n\
                     }\n\
                 }\n",
            ),
            (
                "src/api.rs",
                "pub fn handle() {\n\
                     println!(\"handling\");\n\
                     std::process::exit(1);\n\
                 }\n\
                 \n\
                 pub struct Pair(pub crate::db::Id, u8);\n",
            ),
        ],
    )
    .unwrap()
});

/// The places of the violations: `file:line item`.
fn places(violations: &[Violation]) -> Vec<String> {
    violations
        .iter()
        .map(|violation| {
            let item = match &violation.detail {
                Detail::Item { item, .. } => item.join("::"),
                Detail::Uses {
                    item: Some(item),
                    target,
                    ..
                } => format!("{} uses {}", item.join("::"), target.join("::")),
                Detail::Uses { target, .. } => {
                    format!("{} uses {}", violation.unit.join("::"), target.join("::"))
                }
                other => panic!("unexpected: {other:?}"),
            };
            format!("{}:{} {item}", violation.file, violation.line)
        })
        .collect()
}

#[test]
fn fields_and_methods_are_declared() {
    let declared: Vec<String> = SHOP
        .facts()
        .declarations
        .iter()
        .filter(|declaration| ["field", "method"].contains(&declaration.kind.as_str()))
        .map(|declaration| {
            let relations: Vec<String> = declaration
                .relations
                .iter()
                .map(|tie| format!("{} {}", tie.relation, tie.target.join("::")))
                .collect();
            format!(
                "{} {} {} {}:{} [{}]",
                declaration.path.join("::"),
                declaration.kind,
                declaration.visibility,
                declaration.file,
                declaration.line,
                relations.join(", "),
            )
        })
        .collect();
    assert_eq!(
        declared,
        [
            "crate::api::Pair::0 field pub src/api.rs:6 [has-type crate::db::Id]",
            "crate::api::Pair::1 field private src/api.rs:6 []",
            "crate::db::Id::0 field pub src/db.rs:2 []",
            "crate::domain::Order::save method pub src/db.rs:6 [takes crate::db::Pool, returns std::result::Result, returns crate::db::Error]",
            "crate::domain::Order::id field pub src/domain.rs:5 [has-type crate::db::Id]",
            "crate::domain::Order::total field private src/domain.rs:6 []",
            "crate::domain::Order::new method pub src/domain.rs:10 [takes crate::db::Id, returns crate::domain::Order]",
            "crate::domain::Order::total method private src/domain.rs:13 []",
            "crate::domain::Priced::price method pub src/domain.rs:19 [returns crate::db::Error]",
        ]
    );
}

/// A selection holds the members only when its kinds name them.
#[test]
fn members_are_selected_by_kind() {
    let default = rule("r")
        .items(any_of(["crate::domain"]))
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        places(&default),
        [
            "src/domain.rs:4 crate::domain::Order",
            "src/domain.rs:18 crate::domain::Priced",
            "src/domain.rs:28 crate::domain::load",
        ]
    );
    let public_fields = rule("r")
        .items(all())
        .of_kind([ItemKind::Field])
        .with_visibility([Visibility::Pub])
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        places(&public_fields),
        [
            "src/api.rs:6 crate::api::Pair::0",
            "src/db.rs:2 crate::db::Id::0",
            "src/domain.rs:5 crate::domain::Order::id",
        ]
    );
}

#[test]
fn signatures_and_field_types() {
    let returns = rule("r")
        .items(all())
        .of_kind([ItemKind::Fn, ItemKind::Method])
        .with_visibility([Visibility::Pub])
        .returning(any_of(["crate::db::Error"]))
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        places(&returns),
        [
            "src/db.rs:6 crate::domain::Order::save",
            "src/domain.rs:19 crate::domain::Priced::price",
            "src/domain.rs:28 crate::domain::load",
        ]
    );
    let takes = rule("r")
        .items(all())
        .of_kind([ItemKind::Method])
        .taking(any_of(["crate::db"]))
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        places(&takes),
        [
            "src/db.rs:6 crate::domain::Order::save",
            "src/domain.rs:10 crate::domain::Order::new",
        ]
    );
    let typed = rule("r")
        .items(all())
        .of_kind([ItemKind::Field])
        .of_type(any_of(["crate::db::Id"]))
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        places(&typed),
        [
            "src/api.rs:6 crate::api::Pair::0",
            "src/domain.rs:5 crate::domain::Order::id",
        ]
    );
}

/// The code of an item is its fields, its signatures and bodies, and its `impl` blocks,
/// also in other modules. A `use` at the top of a module is in no item.
#[test]
fn the_uses_inside_items() {
    let order = rule("orders know no database")
        .items(all())
        .named(["Order"])
        .must_not_use(any_of(["crate::db"]))
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        places(&order),
        [
            "src/db.rs:6 crate::domain::Order::save uses crate::db::Pool",
            "src/db.rs:6 crate::domain::Order::save uses crate::db::Error",
            "src/db.rs:7 crate::domain::Order::save uses crate::db::Pool",
            "src/domain.rs:5 crate::domain::Order::id uses crate::db::Id",
            "src/domain.rs:10 crate::domain::Order::new uses crate::db::Id",
        ]
    );
    // A baseline counts the use by its item.
    let key = refpact::core::baseline_key(&order[0], &refpact::core::PathFormat::plain("::"));
    assert_eq!(
        key,
        (
            "crate::domain::Order::save".to_owned(),
            "uses type crate::db::Pool".to_owned()
        )
    );
    let only = rule("functions use only the domain")
        .items(any_of(["crate::domain"]))
        .of_kind([ItemKind::Fn])
        .must_only_use(any_of(["std"]))
        .evaluate(&SHOP)
        .unwrap();
    // `T` is a parameter of `load`, and `Result` is in `std`.
    assert_eq!(
        places(&only),
        ["src/domain.rs:28 crate::domain::load uses crate::db::Error"]
    );
}

#[test]
fn the_reason_of_a_rule_is_in_the_report() {
    let failure = rule("orders know no database")
        .items(all())
        .named(["Order"])
        .must_not_use(any_of(["crate::db::Pool"]))
        .because("the domain must run without a database")
        .verify(&SHOP)
        .unwrap_err();
    assert!(matches!(
        &failure,
        Failure::Violated {
            reason: Some(_),
            ..
        }
    ));
    assert_eq!(
        failure.to_string(),
        "rule `orders know no database` is violated in 2 place(s):\n  \
         because the domain must run without a database\n  \
         src/db.rs:6:27: [orders know no database] crate::domain::Order::save uses crate::db::Pool (type)\n  \
         src/db.rs:7:9: [orders know no database] crate::domain::Order::save uses crate::db::Pool (expr)"
    );
}

/// The conventions of the code are ordinary rules about macros, functions and markers: the
/// example of `docs/rust.md`.
#[test]
fn the_conventions_of_the_code_are_rules() {
    let conventions = rules(
        "the conventions of the code",
        [
            rule("no printing")
                .modules(all())
                .must_not_use(any_of(["std::print!", "std::println!", "std::dbg!"]))
                .except_in_test_code(),
            rule("no unfinished code")
                .modules(all())
                .must_not_use(any_of(["std::todo!", "std::unimplemented!"]))
                .except_in_test_code()
                .because("`todo!` and `unimplemented!` panic when the code runs"),
            rule("no exit of the process")
                .modules(all())
                .must_not_use(any_of(["std::process::exit", "std::process::abort"]))
                .except_in_test_code(),
        ],
    );
    let failure = conventions.verify(&SHOP).unwrap_err();
    let Failure::Several(failures) = &failure else {
        panic!("{failure}")
    };
    let found: Vec<String> = failures
        .iter()
        .map(|failure| match failure {
            Failure::Violated {
                rule, violations, ..
            } => format!("{rule}: {}", places(violations).join(", ")),
            other => panic!("{other}"),
        })
        .collect();
    // The `println!` of the test module does not count.
    assert_eq!(
        found,
        [
            "no printing: src/api.rs:2 crate::api uses std::println!",
            "no unfinished code: src/domain.rs:29 crate::domain uses std::todo!",
            "no exit of the process: src/api.rs:3 crate::api uses std::process::exit",
        ]
    );
    assert!(failure
        .to_string()
        .contains("  because `todo!` and `unimplemented!` panic when the code runs\n"));
    rule("no unsafe code")
        .modules(all())
        .must_not_contain(unsafe_code())
        .check(&SHOP);
}

#[test]
fn the_onion_architecture() {
    let app = Crate::from_memory(
        "src/lib.rs",
        &[
            (
                "src/lib.rs",
                "pub mod model;
pub mod service;
pub mod application;
pub mod web;
pub mod store;
",
            ),
            (
                "src/model.rs",
                "pub struct Order;
pub fn total() { crate::service::price(); }
",
            ),
            (
                "src/service.rs",
                "pub fn price() { let _ = crate::model::Order; }
",
            ),
            (
                "src/application.rs",
                "pub fn checkout() { crate::service::price(); }
",
            ),
            (
                "src/web.rs",
                "pub fn get() { crate::application::checkout(); crate::store::save(); }
",
            ),
            (
                "src/store.rs",
                "pub fn save() { let _ = crate::model::Order; crate::web::get(); }
",
            ),
        ],
    )
    .unwrap();
    let onion = onion_architecture("the shop")
        .domain_models(any_of(["crate::model"]))
        .domain_services(any_of(["crate::service"]))
        .application_services(any_of(["crate::application"]))
        .adapter("web", any_of(["crate::web"]))
        .adapter("store", any_of(["crate::store"]))
        .because("the domain is the centre");
    let names: Vec<String> = onion
        .rules()
        .unwrap()
        .iter()
        .map(|made| made.as_core().name.clone())
        .collect();
    assert_eq!(
        names,
        [
            "the shop: adapter web uses only the layers it may use",
            "the shop: adapter store does not use the layers above it",
            "the shop: application services does not use the layers above it",
            "the shop: domain services does not use the layers above it",
            "the shop: domain models does not use the layers above it",
        ]
    );
    // The adapters use each other, and the models use a service.
    assert_eq!(
        onion.verify(&app).unwrap_err().to_string(),
        "rule `the shop: adapter web uses only the layers it may use` is violated in 1 place(s):\n  \
         because the domain is the centre\n  \
         src/web.rs:1:48: [the shop: adapter web uses only the layers it may use] crate::web uses crate::store::save (expr)\n\
         rule `the shop: adapter store does not use the layers above it` is violated in 1 place(s):\n  \
         because the domain is the centre\n  \
         src/store.rs:1:46: [the shop: adapter store does not use the layers above it] crate::store uses crate::web::get (expr)\n\
         rule `the shop: domain models does not use the layers above it` is violated in 1 place(s):\n  \
         because the domain is the centre\n  \
         src/model.rs:2:18: [the shop: domain models does not use the layers above it] crate::model uses crate::service::price (expr)"
    );
    let too_small = onion_architecture("one part").domain_models(any_of(["crate::model"]));
    assert!(matches!(
        too_small.verify(&app),
        Err(Failure::Malformed { .. })
    ));
}
