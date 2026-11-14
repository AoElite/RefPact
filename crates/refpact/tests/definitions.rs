//! What the index of rust-analyzer says of a path that the syntax resolved: the definition
//! behind a re-export of another crate, and the item behind a name of a glob import.

use std::sync::LazyLock;

use refpact::core::What;
use refpact::{any_of, rule, Crate};

const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../fixtures/rust/definitions"
);

static RESOLVED: LazyLock<Crate> =
    LazyLock::new(|| Crate::options(FIXTURE).resolving_members().load().unwrap());
static SYNTAX: LazyLock<Crate> = LazyLock::new(|| Crate::options(FIXTURE).load().unwrap());

/// The references of a file: `line:column kind target = aliases`, with `?` for a candidate.
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
                for alias in &reference.aliases {
                    text.push_str(&format!(" = {}", alias.join("::")));
                }
                if reference.certainty.as_str() == "candidate" {
                    text.push_str(" ?");
                }
                Some(text)
            }
            What::Marker(_) => None,
        })
        .collect()
}

#[test]
fn a_path_into_another_crate_has_its_definition_as_an_alias() {
    assert_eq!(
        references(&RESOLVED, "src/direct.rs"),
        [
            "1:13 use facade::Motor = engine::Motor",
            "2:16 use std::time::Duration = core::time::Duration",
            "4:18 type facade::Motor = engine::Motor",
            "4:25 type std::time::Duration = core::time::Duration",
            // A re-export under another name.
            "5:17 expr facade::Drive::start = engine::Motor::start",
            "6:13 expr std::time::Duration::from_secs = core::time::Duration::from_secs",
        ]
    );
    // The syntax does not read other crates.
    assert_eq!(
        references(&SYNTAX, "src/direct.rs"),
        [
            "1:13 use facade::Motor",
            "2:16 use std::time::Duration",
            "4:18 type facade::Motor",
            "4:25 type std::time::Duration",
            "5:17 expr facade::Drive::start",
            "6:13 expr std::time::Duration::from_secs",
        ]
    );
}

#[test]
fn a_rule_that_names_the_definition_sees_a_use_through_a_re_export() {
    let no_engine = || {
        rule("the engine is not used")
            .modules(any_of(["crate::direct"]))
            .must_not_use(any_of(["engine"]))
    };
    let failure = no_engine().verify(&RESOLVED).unwrap_err().to_string();
    assert_eq!(
        failure.lines().nth(1),
        Some(concat!(
            "  src/direct.rs:1:13: [the engine is not used] crate::direct uses engine::Motor ",
            "(use, written as facade::Motor)"
        ))
    );
    assert_eq!(failure.lines().count(), 4);
    no_engine().check(&SYNTAX);

    // The path as the code writes it is a use as before, and `core` behind `std` is one.
    let failure = rule("no time")
        .modules(any_of(["crate::direct"]))
        .must_not_use(any_of(["core::time", "facade::Drive"]))
        .verify(&RESOLVED)
        .unwrap_err();
    assert_eq!(failure.to_string().lines().count(), 5);
}

#[test]
fn the_index_proves_the_name_behind_a_glob_import() {
    assert_eq!(
        references(&RESOLVED, "src/globbed.rs"),
        [
            "1:13 use-glob facade",
            "2:23 use-glob std::collections",
            // `facade` gives the items of `engine::parts` again.
            "4:19 type facade::Gear = engine::parts::Gear",
            "5:16 type std::collections::HashMap = std::collections::hash::map::HashMap",
            "5:36 expr std::collections::HashMap::new = std::collections::hash::map::HashMap::new",
            "6:5 expr facade::Gear = engine::parts::Gear",
            "6:10 expr facade::wheel = engine::parts::wheel",
            "6:26 method-call std::collections::hash::map::HashMap::len",
            "10:5 macro crate::bind!",
            // `wheel` at line 11 is a variable that the macro declares.
            // `std` gives the items of `alloc` again.
            "14:21 type std::collections::BTreeMap = alloc::collections::btree::map::BTreeMap",
            "15:5 expr std::collections::BTreeMap::new = alloc::collections::btree::map::BTreeMap::new",
        ]
    );
    assert_eq!(
        references(&SYNTAX, "src/globbed.rs"),
        [
            "1:13 use-glob facade",
            "1:13 use-glob std::collections::facade ?",
            "2:23 use-glob std::collections",
            "4:19 type facade::Gear ?",
            "4:19 type std::collections::Gear ?",
            "5:16 type facade::HashMap ?",
            "5:16 type std::collections::HashMap ?",
            "5:36 expr facade::HashMap::new ?",
            "5:36 expr std::collections::HashMap::new ?",
            "6:5 expr facade::Gear ?",
            "6:5 expr std::collections::Gear ?",
            "6:10 expr facade::wheel ?",
            "6:10 expr std::collections::wheel ?",
            "10:5 macro crate::bind!",
            "11:5 expr facade::wheel ?",
            "11:5 expr std::collections::wheel ?",
            "14:21 type facade::BTreeMap ?",
            "14:21 type std::collections::BTreeMap ?",
            "15:5 expr facade::BTreeMap::new ?",
            "15:5 expr std::collections::BTreeMap::new ?",
        ]
    );
    let notes: Vec<_> = RESOLVED
        .facts()
        .facts
        .iter()
        .filter(|fact| fact.file == "src/globbed.rs" && fact.line == 4)
        .map(|fact| fact.note.clone())
        .collect();
    assert_eq!(
        notes,
        [Some("through the glob import at line 1".to_owned())]
    );
}
