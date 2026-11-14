//! `REFPACT_BASELINE=update`. This file holds one test, because the variable belongs to the
//! whole process and other tests must not see it.

use refpact::{any_of, rule, Baseline, Crate};

/// Removes a temporary file or directory when the test ends, also when it fails.
struct Removed(std::path::PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

#[test]
fn the_environment_variable_makes_the_check_write_the_baseline() {
    let planted = Crate::options(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust/planted"
    ))
    .resolving_members()
    .load()
    .unwrap();
    let file = std::env::temp_dir().join(format!("refpact-{}-update.baseline", std::process::id()));
    let _removed = Removed(file.clone());
    let _ = std::fs::remove_file(&file);
    let baseline = Baseline::file(&file);
    let leaf = rule("strict does not use leaf")
        .modules(any_of(["crate::rings::strict"]))
        .must_not_use(any_of(["crate::rings::leaf"]));

    // Any other value does not write.
    std::env::set_var("REFPACT_BASELINE", "yes");
    assert!(leaf.verify_with_baseline(&planted, &baseline).is_err());
    assert!(!file.exists());

    std::env::set_var("REFPACT_BASELINE", "update");
    leaf.check_with_baseline(&planted, &baseline);
    // The plain check does not read the variable or the file.
    assert!(leaf.verify(&planted).is_err());
    std::env::remove_var("REFPACT_BASELINE");

    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        text.lines().filter(|line| !line.starts_with('#')).count(),
        2
    );
    leaf.check_with_baseline(&planted, &baseline);
}
