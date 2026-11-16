//! refpact checks its own crates. The core is the crate `refpact_core`. The crate `refpact`
//! has the Rust front end in `refpact::rust` and the binding, which is the crate root with
//! `refpact::baseline`, `refpact::group`, `refpact::items`,
//! `refpact::layers` and `refpact::onion`. The crate
//! `refpact_wasm` gives the core to other languages.

use std::sync::LazyLock;

use refpact::{all_except, any_of, layers, rule, unsafe_code, Detail, Visibility, Workspace};

static REFPACT: LazyLock<Workspace> = LazyLock::new(|| {
    Workspace::options(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
        .resolving_members()
        .members([
            "crates/refpact-core",
            "crates/refpact",
            "crates/refpact-wasm",
        ])
        .load()
        .unwrap()
});

const ROOTS: [&str; 3] = ["refpact_core", "refpact", "refpact_wasm"];

const OPERATING_SYSTEM: [&str; 6] = [
    "std::process",
    "std::net",
    "std::fs",
    "std::thread",
    "std::time",
    "std::env",
];

const BUILT_IN_CHECKS: [&str; 5] = [
    "refpact_core::lengths",
    "refpact_core::uses",
    "refpact_core::cycles",
    "refpact_core::reach",
    "refpact_core::demands",
];

const BINDING: [&str; 7] = [
    "refpact",
    "refpact::baseline",
    "refpact::diagram",
    "refpact::group",
    "refpact::items",
    "refpact::layers",
    "refpact::onion",
];

#[test]
fn the_core_does_not_know_a_language() {
    rule("the core does not know a language")
        .modules(any_of(["refpact_core::**"]))
        .must_only_use(any_of(["std", "core", "alloc", "refpact_core"]))
        .check(&REFPACT);
}

#[test]
fn the_core_does_not_use_the_operating_system() {
    rule("the core does not use the operating system")
        .modules(any_of(["refpact_core::**"]))
        .must_not_use(any_of(OPERATING_SYSTEM))
        .check(&REFPACT);
}

#[test]
fn patterns_and_the_model_do_not_use_evaluation() {
    rule("patterns and the model do not use evaluation")
        .modules(any_of([
            "refpact_core::pattern::**",
            "refpact_core::model::**",
        ]))
        .must_not_use(any_of(["refpact_core::eval", "refpact_core::report"]))
        .check(&REFPACT);
}

/// The built-in constraints and demands are checks like the checks of a user. They use what
/// the core exports: not the index, not the checks of patterns behind `Setup`, and not the
/// filters behind `Scope`.
#[test]
fn the_built_in_checks_use_what_a_check_of_the_user_can_use() {
    rule("the built-in checks use what a check of the user can use")
        .modules(any_of(BUILT_IN_CHECKS))
        .must_only_use(any_of([
            "std",
            "core",
            "alloc",
            "refpact_core::check",
            "refpact_core::item_check",
            "refpact_core::graph",
            "refpact_core::error",
            "refpact_core::model",
            "refpact_core::pattern",
            "refpact_core::rule",
            "refpact_core::violation",
            "refpact_core::cycles",
        ]))
        .check(&REFPACT);
    // Everything that these modules may use by a path is exported.
    rule("what a check uses is exported")
        .items(any_of([
            "refpact_core::check",
            "refpact_core::item_check",
            "refpact_core::error",
        ]))
        .with_visibility([Visibility::PubCrate, Visibility::PubRestricted])
        .must_not_exist()
        .check(&REFPACT);
}

#[test]
fn only_sources_reads_the_file_system_in_the_front_end() {
    rule("only sources reads the file system in the front end")
        .modules(any_of(["refpact::rust::**"]).except(["refpact::rust::sources::**"]))
        .must_not_use(any_of(
            OPERATING_SYSTEM
                .iter()
                .filter(|path| **path != "std::thread"),
        ))
        .check(&REFPACT);
}

#[test]
fn only_workers_starts_threads() {
    rule("only workers starts threads")
        .modules(all_except(["refpact::rust::workers::**"]))
        // `thread_local!` and its `LocalKey::with` start no thread.
        .must_not_use(any_of([
            "std::thread::spawn",
            "std::thread::scope",
            "std::thread::Builder",
        ]))
        .check(&REFPACT);
}

#[test]
fn name_resolution_does_not_read_syntax() {
    rule("name resolution does not read syntax")
        .modules(any_of(["refpact::rust::scope::**"]))
        .must_not_use(any_of([
            "syn",
            "proc_macro2",
            "refpact::rust::load",
            "refpact::rust::extract",
            "refpact::rust::macros",
            "refpact::rust::sources",
        ]))
        .check(&REFPACT);
}

#[test]
fn the_tables_and_the_macro_parser_stand_alone() {
    rule("the tables and the macro parser stand alone")
        .modules(any_of([
            "refpact::rust::tables::**",
            "refpact::rust::macros::**",
        ]))
        .must_not_use(any_of([
            "refpact::rust::load",
            "refpact::rust::extract",
            "refpact::rust::scope",
            "refpact::rust::sources",
        ]))
        .check(&REFPACT);
}

#[test]
fn the_front_end_uses_only_the_core_and_the_parser() {
    rule("the front end uses only the core and the parser")
        .modules(any_of(["refpact::rust::**"]))
        .must_only_use(any_of([
            "std",
            "syn",
            "proc_macro2",
            "refpact_core",
            "refpact::rust",
        ]))
        .check(&REFPACT);
}

#[test]
fn the_binding_does_not_parse() {
    rule("the binding does not parse")
        .modules(any_of(BINDING))
        .must_not_use(any_of([
            "syn",
            "proc_macro2",
            "std::process",
            "std::net",
            "std::thread",
        ]))
        .check(&REFPACT);
}

#[test]
fn only_the_baseline_and_the_diagram_file_read_and_write_files_in_the_binding() {
    rule("only the baseline and the diagram file read and write files in the binding")
        .modules(any_of([
            "refpact",
            "refpact::group",
            "refpact::items",
            "refpact::layers",
            "refpact::onion",
        ]))
        .must_not_use(any_of(["std::fs", "std::env"]))
        .check(&REFPACT);
}

#[test]
fn the_parts_of_the_core_form_no_cycle() {
    rule("the parts of the core form no cycle")
        .modules(all_except(["refpact_core"]))
        .must_be_acyclic_between(any_of(["refpact_core::*"]))
        .except_in_test_code()
        .check(&REFPACT);
}

#[test]
fn the_parts_are_layers_and_the_core_reaches_no_file() {
    // As a target, `crate` would cover the whole crate, so the binding layer names only its
    // modules below the root. The allow-lists of the core and of the front end keep both away
    // from the items of the root.
    layers("the parts")
        .layer(
            "binding",
            any_of([
                "refpact::baseline",
                "refpact::diagram",
                "refpact::group",
                "refpact::items",
                "refpact::layers",
                "refpact::onion",
            ]),
        )
        .layer("front end", any_of(["refpact::rust::**"]))
        .layer("core", any_of(["refpact_core::**"]))
        .check(&REFPACT);
    rule("the core reaches no file")
        .modules(any_of(["refpact_core::**"]))
        .must_not_reach(any_of(["std::fs", "std::env", "std::process"]))
        .check(&REFPACT);
}

#[test]
fn every_module_is_used_and_reachable_from_the_crate_root() {
    // A test module is a module that nothing uses, and the ready-made rules are for users.
    rule("every module is used")
        .modules(all_except(ROOTS.iter().copied().chain(["**::tests"])))
        .must_be_used()
        .except_in_test_code()
        .check(&REFPACT);
    rule("every module is reachable from the crate root")
        .modules(all_except(["**::tests"]))
        .must_be_reachable_from(any_of(ROOTS))
        .except_in_test_code()
        .check(&REFPACT);
}

#[test]
fn no_unsafe_code() {
    // The WebAssembly interface reads and writes memory that JavaScript hands over.
    rule("no unsafe code outside the WebAssembly interface")
        .modules(all_except(["refpact_wasm::**"]))
        .must_not_contain(unsafe_code())
        .check(&REFPACT);
}

#[test]
fn the_webassembly_interface_uses_only_the_core() {
    rule("the WebAssembly interface uses only the core")
        .modules(any_of(["refpact_wasm::**"]))
        .must_only_use(any_of([
            "std",
            "core",
            "alloc",
            "refpact_core",
            "refpact_wasm",
        ]))
        .check(&REFPACT);
}

/// A rule that passes proves little. These must find the uses that the rules above allow.
#[test]
fn the_rules_above_can_fail() {
    let files = rule("probe")
        .modules(any_of(["refpact::rust::sources"]))
        .must_not_use(any_of(["std::fs"]))
        .evaluate(&REFPACT)
        .unwrap();
    assert!(!files.is_empty());

    let layers = rule("probe")
        .modules(any_of(["refpact_core::eval"]))
        .must_not_use(any_of(["refpact_core::pattern", "refpact_core::model"]))
        .evaluate(&REFPACT)
        .unwrap();
    assert!(!layers.is_empty());

    let built_in = rule("probe")
        .modules(any_of(BUILT_IN_CHECKS))
        .must_not_use(any_of(["refpact_core::check", "refpact_core::graph"]))
        .evaluate(&REFPACT)
        .unwrap();
    assert!(!built_in.is_empty());
    let framework = rule("probe")
        .modules(any_of(["refpact_core::check", "refpact_core::items"]))
        .must_only_use(any_of(["std", "refpact_core::model"]))
        .evaluate(&REFPACT)
        .unwrap();
    assert!(!framework.is_empty());

    let front_end = rule("probe")
        .modules(any_of(["refpact::rust::**"]))
        .must_not_use(any_of(["syn", "refpact_core"]))
        .evaluate(&REFPACT)
        .unwrap();
    assert!(!front_end.is_empty());

    // The groups of the binding reach the file system through the baseline.
    let chain = rule("probe")
        .modules(any_of(["refpact::group"]))
        .must_not_reach(any_of(["std::fs"]))
        .evaluate(&REFPACT)
        .unwrap();
    assert!(
        matches!(&chain[0].detail, Detail::Reaches { steps, end } if !steps.is_empty() && end.unit.join("::") == "refpact::baseline")
    );
}
