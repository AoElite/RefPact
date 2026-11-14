//! The rules about lengths: the lines of modules and of items, as the front end counts them.

use std::sync::LazyLock;

use refpact::{all, any_of, rule, Crate, Detail, Failure, ItemKind, Rule};

static SHOP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            // 4 lines. The inline module has 3 of them.
            (
                "src/lib.rs",
                "pub mod api;\nmod small {\n    pub fn f() {}\n}\n",
            ),
            (
                // 22 lines.
                "src/api.rs",
                "/// Serves.\n\
                 #[inline]\n\
                 pub fn serve(\n\
                     port: u16,\n\
                 ) -> u16 {\n\
                     let a = port;\n\
                     a\n\
                 }\n\
                 \n\
                 pub struct Config {\n\
                     pub port: u16,\n\
                     pub name: Vec<\n\
                         u8,\n\
                     >,\n\
                 }\n\
                 pub struct Unit;\n\
                 pub const LIMIT: u16 =\n\
                     1;\n\
                 impl Config {\n\
                     pub fn port(&self) -> u16 {\n\
                         self.port\n\
                     }\n\
                 }\n\
                 pub trait Named {\n\
                     fn name(&self) -> String;\n\
                     fn long(&self) -> String {\n\
                         String::new()\n\
                     }\n\
                 }\n\
                 macro_rules! twice {\n\
                     ($e:expr) => {\n\
                         $e\n\
                     };\n\
                 }\n",
            ),
        ],
    )
    .unwrap()
});

/// The lines that a rule reports, each with the module or the item.
fn lengths(rule: Rule) -> Vec<(String, u32)> {
    let Err(Failure::Violated { violations, .. }) = rule.verify(&SHOP) else {
        panic!("the rule is violated");
    };
    violations
        .into_iter()
        .map(|violation| match violation.detail {
            Detail::TooLong { item, lines, .. } => {
                (item.unwrap_or(violation.unit).join("::"), lines)
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn the_lines_of_a_module_are_those_of_its_file_or_of_its_declaration() {
    let found = lengths(rule("r").modules(all()).must_not_exceed_lines(0));
    assert_eq!(
        found,
        [
            ("crate".to_owned(), 4),
            ("crate::api".to_owned(), 34),
            ("crate::small".to_owned(), 3),
        ]
    );
}

#[test]
fn a_module_at_the_limit_passes() {
    rule("r")
        .modules(any_of(["crate::api"]))
        .must_not_exceed_lines(34)
        .check(&SHOP);
    let failure = rule("r")
        .modules(any_of(["crate::api"]))
        .must_not_exceed_lines(33)
        .verify(&SHOP)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `r` is violated in 1 place(s):\n  \
         src/api.rs: [r] crate::api has 34 lines, and the limit is 33"
    );
}

#[test]
fn the_lines_of_an_item_run_from_its_name_to_its_last_token() {
    let found = lengths(
        rule("r")
            .items(any_of(["crate::api"]))
            .must_not_exceed_lines(0),
    );
    assert_eq!(
        found,
        [
            ("crate::api::serve".to_owned(), 6),
            ("crate::api::Config".to_owned(), 6),
            ("crate::api::Unit".to_owned(), 1),
            ("crate::api::LIMIT".to_owned(), 2),
            ("crate::api::Named".to_owned(), 6),
            ("crate::api::twice".to_owned(), 5),
        ]
    );
}

#[test]
fn a_field_ends_at_its_comma_and_a_method_at_its_last_token() {
    let found = lengths(
        rule("r")
            .items(any_of(["crate::api"]))
            .of_kind([ItemKind::Field, ItemKind::Method])
            .must_not_exceed_lines(0),
    );
    assert_eq!(
        found,
        [
            ("crate::api::Config::port".to_owned(), 1),
            ("crate::api::Config::name".to_owned(), 3),
            ("crate::api::Config::port".to_owned(), 3),
            ("crate::api::Named::name".to_owned(), 1),
            ("crate::api::Named::long".to_owned(), 3),
        ]
    );
}

#[test]
fn a_rule_about_long_functions_reports_the_function_at_its_name() {
    let failure = rule("functions are short")
        .items(all())
        .of_kind([ItemKind::Fn, ItemKind::Method])
        .must_not_exceed_lines(5)
        .verify(&SHOP)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `functions are short` is violated in 1 place(s):\n  \
         src/api.rs:3:8: [functions are short] crate::api declares fn crate::api::serve, \
         which has 6 lines, and the limit is 5"
    );
}
