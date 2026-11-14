//! Warnings: rules whose violations are reported and do not fail the check.

use std::sync::LazyLock;

use refpact::{any_of, rule, rules, Baseline, Crate, Failure};

static SHOP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod api;\npub mod db;\n"),
            ("src/api.rs", "pub fn get() { crate::db::query(); }\n"),
            ("src/db.rs", "pub fn query() {}\n"),
        ],
    )
    .unwrap()
});

fn api_uses_db() -> refpact::Rule {
    rule("the api does not use the db")
        .modules(any_of(["crate::api"]))
        .must_not_use(any_of(["crate::db"]))
}

/// Removes a file when it goes out of scope.
struct Removed(std::path::PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
    }
}

#[test]
fn a_warning_passes_with_its_violations() {
    assert!(matches!(
        api_uses_db().verify(&SHOP),
        Err(Failure::Violated { .. })
    ));
    let warning = api_uses_db().as_warning();
    assert_eq!(warning.verify(&SHOP), Ok(()));
    warning.check(&SHOP);
    // The evaluation does not change.
    assert_eq!(warning.evaluate(&SHOP).unwrap().len(), 1);
    // A set of rules fails only on its errors.
    let set = rules("set", [api_uses_db().as_warning()]);
    assert_eq!(set.verify(&SHOP), Ok(()));
    let mixed = rules("set", [api_uses_db().as_warning(), api_uses_db()]);
    assert!(mixed.verify(&SHOP).is_err());
}

#[test]
fn a_warning_that_cannot_fail_is_an_error() {
    let invalid = rule("r")
        .modules(any_of(["crate::web"]))
        .must_not_use(any_of(["crate::db"]))
        .as_warning();
    assert!(matches!(
        invalid.verify(&SHOP),
        Err(Failure::Invalid { .. })
    ));
}

#[test]
fn a_warning_with_a_baseline() {
    let file =
        std::env::temp_dir().join(format!("refpact-warning-{}.baseline", std::process::id()));
    let _removed = Removed(file.clone());
    let baseline = Baseline::file(&file);
    // Violations that the baseline does not accept do not fail a warning.
    std::fs::write(&file, "").unwrap();
    let warning = api_uses_db().as_warning();
    assert_eq!(warning.verify_with_baseline(&SHOP, &baseline), Ok(()));
    // A stale entry still fails.
    warning.write_baseline(&SHOP, &baseline).unwrap();
    let gone = rule("the api does not use the db")
        .modules(any_of(["crate::db"]))
        .must_not_use(any_of(["crate::api"]))
        .as_warning();
    assert!(matches!(
        gone.verify_with_baseline(&SHOP, &baseline),
        Err(Failure::StaleBaseline { .. })
    ));
}
