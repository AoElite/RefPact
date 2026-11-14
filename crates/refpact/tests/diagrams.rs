//! Diagrams of what a rule sees, in Mermaid and DOT.

use std::sync::LazyLock;

use refpact::{
    all, all_except, any_of, layers, onion_architecture, rule, Crate, DiagramFormat, ErrorCode,
    Failure, Role,
};

static SHOP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod api;\npub mod db;\npub mod util;\n"),
            (
                "src/api.rs",
                "pub fn get() { crate::db::query(); crate::util::log(); }\n",
            ),
            (
                "src/db.rs",
                "pub fn query() { crate::api::get(); crate::util::log(); }\n",
            ),
            ("src/util.rs", "pub fn log() {}\n"),
        ],
    )
    .unwrap()
});

#[test]
fn a_mermaid_diagram_of_the_modules() {
    let diagram = rule("the modules")
        .modules(all_except(["crate"]))
        .must_be_acyclic()
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap();
    assert_eq!(
        diagram,
        "---\n\
         title: \"the modules\"\n\
         ---\n\
         flowchart LR\n    \
             n0[\"crate::api\"]\n    \
             n1[\"crate::db\"]\n    \
             n2[\"crate::util\"]\n    \
             n0 --> n1\n    \
             n0 --> n2\n    \
             n1 --> n0\n    \
             n1 --> n2\n    \
             classDef cycle stroke:#d62728,stroke-width:2px\n    \
             class n0,n1 cycle\n    \
             linkStyle 0,2 stroke:#d62728,stroke-width:2px\n"
    );
}

#[test]
fn a_dot_diagram_of_the_slices() {
    let diagram = rule("the \"parts\"")
        .modules(all())
        .must_be_independent_between(any_of(["crate::api", "crate::util"]))
        .diagram(&SHOP, DiagramFormat::Dot)
        .unwrap();
    assert_eq!(
        diagram,
        "digraph {\n    \
             label=\"the \\\"parts\\\"\";\n    \
             rankdir=LR;\n    \
             node [shape=box];\n    \
             n0 [label=\"crate::api\"];\n    \
             n1 [label=\"crate::util\"];\n    \
             n0 -> n1;\n\
         }\n"
    );
}

#[test]
fn a_diagram_sees_what_the_rule_sees() {
    // The exemption takes the edge from the db to the api away, and with it the cycle.
    let diagram = rule("r")
        .modules(any_of(["crate::api", "crate::db"]))
        .must_not_use(any_of(["std"]))
        .ignoring_use(any_of(["crate::db"]), any_of(["crate::api"]))
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap();
    assert!(diagram.ends_with("n0[\"crate::api\"]\n    n1[\"crate::db\"]\n    n0 --> n1\n"));
}

#[test]
fn a_diagram_of_a_rule_that_cannot_fail_is_an_error() {
    let failure = rule("r")
        .modules(any_of(["crate::web"]))
        .must_be_acyclic()
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap_err();
    let Failure::Invalid { errors, .. } = failure else {
        panic!("not invalid: {failure:?}");
    };
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
}

#[test]
fn a_diagram_of_layers_marks_the_uses_they_forbid() {
    let diagram = layers("the layers")
        .layer("api", any_of(["crate::api"]))
        .layer("db", any_of(["crate::db"]))
        .layer("util", any_of(["crate::util"]))
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap();
    assert_eq!(
        diagram,
        "---\n\
         title: \"the layers\"\n\
         ---\n\
         flowchart TB\n    \
             n0[\"api\"]\n    \
             n1[\"db\"]\n    \
             n2[\"util\"]\n    \
             n0 --> n1\n    \
             n0 --> n2\n    \
             n1 --> n0\n    \
             n1 --> n2\n    \
             linkStyle 2 stroke:#d62728,stroke-width:2px\n"
    );
}

#[test]
fn a_diagram_of_strict_layers_in_dot() {
    let diagram = layers("the layers")
        .layer("api", any_of(["crate::api"]))
        .layer("db", any_of(["crate::db"]))
        .layer("util", any_of(["crate::util"]))
        .strict()
        .diagram(&SHOP, DiagramFormat::Dot)
        .unwrap();
    assert_eq!(
        diagram,
        "digraph {\n    \
             label=\"the layers\";\n    \
             rankdir=TB;\n    \
             node [shape=box];\n    \
             n0 [label=\"api\"];\n    \
             n1 [label=\"db\"];\n    \
             n2 [label=\"util\"];\n    \
             n0 -> n1;\n    \
             n0 -> n2 [color=\"#d62728\"];\n    \
             n1 -> n0 [color=\"#d62728\"];\n    \
             n1 -> n2;\n\
         }\n"
    );
}

#[test]
fn a_module_is_in_the_first_layer_that_matches_it() {
    // `crate::api` is in both layers, so it is in `web`; `crate::util` is in none.
    let diagram = layers("the layers")
        .layer("web", any_of(["crate::api"]))
        .layer("core", any_of(["crate::api", "crate::db"]))
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap();
    assert!(diagram.ends_with(
        "n0[\"web\"]\n    n1[\"core\"]\n    n0 --> n1\n    n1 --> n0\n    \
         linkStyle 1 stroke:#d62728,stroke-width:2px\n"
    ));
}

#[test]
fn a_diagram_of_an_onion_is_that_of_its_layers() {
    let onion = onion_architecture("the onion")
        .domain_models(any_of(["crate::util"]))
        .adapter("web", any_of(["crate::api"]))
        .adapter("storage", any_of(["crate::db"]));
    assert_eq!(
        onion.diagram(&SHOP, DiagramFormat::Mermaid).unwrap(),
        onion
            .as_layers()
            .diagram(&SHOP, DiagramFormat::Mermaid)
            .unwrap()
    );
    let diagram = onion.diagram(&SHOP, DiagramFormat::Mermaid).unwrap();
    // The adapters must not use each other.
    assert!(diagram.ends_with("linkStyle 0,2 stroke:#d62728,stroke-width:2px\n"));
}

#[test]
fn a_layer_that_matches_nothing_is_an_error() {
    let failure = layers("the layers")
        .layer("api", any_of(["crate::api"]))
        .layer("web", any_of(["crate::web"]))
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap_err();
    let Failure::Invalid { rule, errors } = failure else {
        panic!("not invalid: {failure:?}");
    };
    assert_eq!(rule, "the layers");
    assert_eq!(errors.len(), 1);
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(errors[0].role, Some(Role::Layer));
}

#[test]
fn malformed_layers_have_no_diagram() {
    let failure = layers("the layers")
        .layer("api", any_of(["crate::api"]))
        .diagram(&SHOP, DiagramFormat::Mermaid)
        .unwrap_err();
    assert!(matches!(failure, Failure::Malformed { .. }), "{failure:?}");
}

#[test]
fn a_diagram_of_layers_leaves_out_test_code_when_they_do() {
    let krate = Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod api;\npub mod db;\n"),
            ("src/api.rs", "pub fn get() {}\n"),
            (
                "src/db.rs",
                "pub fn query() {}\n#[cfg(test)]\nmod tests {\n    fn t() { crate::api::get(); }\n}\n",
            ),
        ],
    )
    .unwrap();
    let made = layers("the layers")
        .layer("api", any_of(["crate::api"]))
        .layer("db", any_of(["crate::db::**"]));
    let all_code = made.diagram(&krate, DiagramFormat::Mermaid).unwrap();
    assert!(all_code.contains("n1 --> n0"), "{all_code}");
    let without_tests = made
        .except_in_test_code()
        .diagram(&krate, DiagramFormat::Mermaid)
        .unwrap();
    assert!(!without_tests.contains("-->"), "{without_tests}");
}
