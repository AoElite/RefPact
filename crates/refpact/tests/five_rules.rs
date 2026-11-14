//! Five example rules, on the clean fixture.
//! This file is the test code that a project writes.

use std::sync::LazyLock;

use refpact::{all_except, any_of, rule, unsafe_code, Crate};

static APP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::options(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust/app"
    ))
    .resolving_members()
    .load()
    .unwrap()
});

#[test]
fn frontend_passes_do_not_use_the_back_ends() {
    rule("frontend passes do not use the back ends")
        .modules(any_of(["crate::frontend::passes::**"]))
        .must_not_use(any_of([
            "crate::codegen",
            "crate::interpreter",
            "crate::execution",
            "crate::service",
            "crate::cache",
        ]))
        .check(&APP);
}

#[test]
fn the_interpreter_does_not_use_the_code_generator() {
    rule("the interpreter does not use the code generator")
        .modules(any_of(["crate::interpreter::**"]))
        .must_not_use(any_of(["crate::codegen"]))
        .check(&APP);
}

#[test]
fn the_operating_system_is_used_in_the_platform_layer_only() {
    rule("the operating system is used in the platform layer only")
        .modules(all_except(["crate::platform::**"]))
        .must_not_use(any_of([
            "std::process",
            "std::net",
            "std::fs",
            "std::thread",
            "std::time",
            "std::env",
        ]))
        .except_in_test_code()
        .check(&APP);
}

#[test]
fn randomness_from_the_system_is_used_in_one_module_only() {
    rule("randomness from the system is used in one module only")
        .modules(all_except(["crate::platform::entropy"]))
        .must_not_use(any_of([
            "getrandom",
            "rand::thread_rng",
            "rand::rng",
            "rand::rngs::OsRng",
            "rand::rngs::ThreadRng",
            "rand_core::OsRng",
        ]))
        .check(&APP);
}

#[test]
fn no_unsafe_code_outside_the_code_generator() {
    rule("no unsafe code outside the code generator")
        .modules(any_of([
            "crate::frontend::**",
            "crate::interpreter::**",
            "crate::cache",
            "crate::execution",
            "crate::service",
        ]))
        .must_not_contain(unsafe_code())
        .check(&APP);
}
