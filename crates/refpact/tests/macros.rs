//! What macros hide from the syntax, and what the index of rust-analyzer shows of it: the
//! method calls and field accesses in the arguments of any macro, and the items that a macro
//! makes from its arguments.

use std::sync::LazyLock;

use refpact::core::What;
use refpact::{any_of, rule, Crate, ErrorCode, Failure};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rust/macros");

static RESOLVED: LazyLock<Crate> =
    LazyLock::new(|| Crate::options(FIXTURE).resolving_members().load().unwrap());
static SYNTAX: LazyLock<Crate> = LazyLock::new(|| Crate::options(FIXTURE).load().unwrap());

/// The method calls and field accesses of a file: `line:column kind target`.
fn calls(code: &Crate, file: &str) -> Vec<String> {
    code.facts()
        .facts
        .iter()
        .filter(|fact| fact.file == file)
        .filter_map(|fact| match &fact.what {
            What::Reference(reference)
                if matches!(reference.kind.as_str(), "method-call" | "field") =>
            {
                Some(format!(
                    "{}:{} {} {}",
                    fact.line,
                    fact.column,
                    reference.kind,
                    reference.target.join("::")
                ))
            }
            _ => None,
        })
        .collect()
}

#[test]
fn a_method_call_and_a_field_in_the_arguments_of_a_macro_are_references() {
    assert_eq!(
        calls(&RESOLVED, "src/api.rs"),
        [
            "6:27 method-call std::fs::File::read_to_end",
            "6:47 method-call core::result::Result::unwrap",
            // `len` of `[u8]`: a method of a type without a name is not resolved.
            "8:26 field crate::Order::id",
        ]
    );
}

#[test]
fn a_rule_sees_a_call_inside_a_macro() {
    let failure = rule("the api does not touch files")
        .modules(any_of(["crate::api"]))
        .must_not_use(any_of(["std::fs"]))
        .verify(&RESOLVED)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `the api does not touch files` is violated in 1 place(s):\n  \
         src/api.rs:6:27: [the api does not touch files] crate::api uses \
         std::fs::File::read_to_end (method-call, of the trait `Read`)"
    );
    // The syntax alone does not see it.
    rule("the api does not touch files")
        .modules(any_of(["crate::api"]))
        .must_not_use(any_of(["std::fs"]))
        .check(&SYNTAX);
}

#[test]
fn an_item_that_a_macro_makes_from_its_arguments_is_a_known_path() {
    let made = rule("the storage does not know the orders")
        .modules(any_of(["crate::storage"]))
        .must_not_use(any_of(["crate::Order", "crate::Order::id"]));
    made.check(&RESOLVED);
    let Err(Failure::Invalid { errors, .. }) = made.verify(&SYNTAX) else {
        panic!("the syntax knows `crate::Order`");
    };
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(errors[0].value.as_deref(), Some("crate::Order"));
}
