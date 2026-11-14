//! `expanding_macros`: the code that a derive, an attribute macro and the body of a
//! `macro_rules!` write, which the source does not show.

use std::sync::LazyLock;

use refpact::core::What;
use refpact::{any_of, rule, Crate};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rust/expanded");

static EXPANDED: LazyLock<Crate> =
    LazyLock::new(|| Crate::options(FIXTURE).expanding_macros().load().unwrap());
static PLAIN: LazyLock<Crate> =
    LazyLock::new(|| Crate::options(FIXTURE).resolving_members().load().unwrap());

/// The references of a file: `line:column kind target [tags] (note)`.
fn references(code: &Crate, file: &str) -> Vec<String> {
    let facts = code.facts().facts.iter();
    facts
        .filter(|fact| fact.file == file)
        .filter_map(|fact| match &fact.what {
            What::Reference(reference) => {
                let mut text = format!(
                    "{}:{} {} {}",
                    fact.line,
                    fact.column,
                    reference.kind,
                    reference.target.join("::")
                );
                if !fact.tags.is_empty() {
                    text.push_str(&format!(" [{}]", fact.tags.join(", ")));
                }
                if let Some(note) = &fact.note {
                    text.push_str(&format!(" ({note})"));
                }
                Some(text)
            }
            What::Marker(_) => None,
        })
        .collect()
}

#[test]
fn the_code_that_a_derive_and_an_attribute_write_is_at_the_macro() {
    assert_eq!(
        references(&EXPANDED, "src/model.rs"),
        [
            "1:14 use derives::Stored",
            "3:10 attribute derives::Stored (through the `use` at line 1)",
            "3:10 type crate::model::Invoice [generated] (in the expansion of `Stored`)",
            "3:10 type std::io::Result [generated] (in the expansion of `Stored`)",
            "3:10 type std::vec::Vec [generated] (in the expansion of `Stored`)",
            "3:10 expr std::fs::read [generated] (in the expansion of `Stored`)",
            // A derive of the language is not expanded.
            "3:18 attribute std::fmt::Debug",
        ]
    );
    assert_eq!(
        references(&EXPANDED, "src/service.rs"),
        [
            "1:3 attribute derives::logged",
            "1:12 expr std::io::_print [generated] (in the expansion of `logged`)",
            "1:12 macro std::format_args_nl! [generated] (in the expansion of `logged`)",
            "1:12 expr std::process::id [generated] (in the expansion of `logged`)",
            // The body of the `macro_rules!` that the function calls.
            "1:12 expr std::time::Instant::now [generated] (in the expansion of `logged`)",
            // The expansion gives the function again. What the source has is not added twice.
            "2:23 type crate::model::Invoice",
            "3:20 macro crate::now!",
        ]
    );
    // It is not the default.
    assert_eq!(references(&PLAIN, "src/model.rs").len(), 3);
    assert_eq!(references(&PLAIN, "src/service.rs").len(), 3);
}

#[test]
fn the_body_of_a_macro_rules_is_read_where_the_macro_is_called() {
    assert_eq!(
        references(&EXPANDED, "src/lib.rs"),
        [
            // The definition is read as tokens, as before.
            "9:9 macro-tokens std::time::Instant::now",
            "12:16 use now",
            "15:21 type std::time::Instant",
            "16:5 macro now! (through the `use` at line 12)",
            "16:5 expr std::time::Instant::now [generated] (in the expansion of `now`)",
        ]
    );
}

#[test]
fn a_rule_sees_the_generated_code_and_can_leave_it_out() {
    let no_files = || {
        rule("the model reads no file")
            .modules(any_of(["crate::model"]))
            .must_not_use(any_of(["std::fs"]))
    };
    let failure = no_files().verify(&EXPANDED).unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `the model reads no file` is violated in 1 place(s):\n  \
         src/model.rs:3:10: [the model reads no file] crate::model uses std::fs::read \
         (expr, in the expansion of `Stored`)"
    );
    no_files().ignoring_generated().check(&EXPANDED);
    // Without the expansions the rule passes, and the filter is no error.
    no_files().check(&PLAIN);
    no_files().ignoring_generated().check(&PLAIN);

    // Layers and an onion have the filter too. The top layer has no rule of its own.
    let made = || {
        refpact::layers("the layers")
            .layer("files", any_of(["std::fs"]))
            .layer("model", any_of(["crate::model"]))
    };
    assert!(made().verify(&EXPANDED).is_err());
    made().ignoring_generated().check(&EXPANDED);
    let made = || {
        refpact::onion_architecture("the onion")
            .application_services(any_of(["std::fs"]))
            .domain_models(any_of(["crate::model"]))
    };
    assert!(made().verify(&EXPANDED).is_err());
    made().ignoring_generated().check(&EXPANDED);
}

#[test]
fn an_item_that_a_macro_writes_is_a_declaration() {
    let declared = |code: &Crate| {
        let found = code.facts().declarations.iter();
        found
            .filter(|found| found.file == "src/model.rs")
            .map(|found| format!("{} {} {}", found.line, found.kind, found.path.join("::")))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        declared(&EXPANDED),
        [
            "4 struct crate::model::Invoice",
            "5 field crate::model::Invoice::total",
            "3 method crate::model::Invoice::load",
        ]
    );
    assert_eq!(declared(&PLAIN).len(), 2);
    let load = EXPANDED.facts().declarations.iter();
    let load = load
        .filter(|found| found.name == "load")
        .collect::<Vec<_>>();
    assert_eq!(load[0].tags, ["generated"]);
    // The path is one that a rule can name.
    rule("the service does not load invoices")
        .modules(any_of(["crate::service"]))
        .must_not_use(any_of(["crate::model::Invoice::load"]))
        .check(&EXPANDED);
}

#[test]
fn a_workspace_reads_the_expansions_of_its_members() {
    let workspace = refpact::Workspace::options(FIXTURE)
        .members(["."])
        .expanding_macros()
        .load()
        .unwrap();
    let generated: Vec<String> = references(&workspace, "src/model.rs")
        .into_iter()
        .filter(|text| text.contains("[generated]"))
        .collect();
    assert_eq!(
        generated,
        [
            "3:10 type expanded::model::Invoice [generated] (in the expansion of `Stored`)",
            "3:10 type std::io::Result [generated] (in the expansion of `Stored`)",
            "3:10 type std::vec::Vec [generated] (in the expansion of `Stored`)",
            "3:10 expr std::fs::read [generated] (in the expansion of `Stored`)",
        ]
    );
}
