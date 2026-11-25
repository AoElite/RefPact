//! Loading a crate: the module tree and the errors that stop an analysis.

use refpact::{Crate, LoadError, Workspace};

mod support;
use support::Removed;

fn modules(krate: &Crate) -> Vec<String> {
    krate.modules().collect()
}

/// Modules are found through `mod` declarations: in files, inline, with a `#[path]` that is
/// relative to the file and owns its directory, with a raw identifier, and in a macro whose
/// argument is a list of items. Other crate roots of the package are not modules, and the
/// root can be set.
#[test]
fn the_module_tree() {
    {
        let krate = Crate::from_memory(
            "src/lib.rs",
            &[
                ("src/lib.rs", "mod a; mod b { mod c; mod d { } }"),
                ("src/a.rs", "mod e;"),
                ("src/a/e.rs", ""),
                ("src/b/c.rs", ""),
            ],
        )
        .unwrap();
        assert_eq!(
            modules(&krate),
            [
                "crate",
                "crate::a",
                "crate::a::e",
                "crate::b",
                "crate::b::c",
                "crate::b::d"
            ]
        );
    }
    {
        let krate = Crate::from_memory(
            "src/lib.rs",
            &[
                (
                    "src/lib.rs",
                    r#"#[path = "elsewhere/x.rs"] mod a; mod inline { #[path = "y.rs"] mod b; }"#,
                ),
                ("src/elsewhere/x.rs", "mod below;"),
                ("src/elsewhere/below.rs", ""),
                ("src/inline/y.rs", ""),
            ],
        )
        .unwrap();
        assert_eq!(
            modules(&krate),
            [
                "crate",
                "crate::a",
                "crate::a::below",
                "crate::inline",
                "crate::inline::b"
            ]
        );
    }
    {
        let krate = Crate::from_memory(
            "lib.rs",
            &[
                ("lib.rs", "mod r#async; use crate::r#async::Task;"),
                ("async.rs", "pub struct Task;"),
            ],
        )
        .unwrap();
        assert_eq!(modules(&krate), ["crate", "crate::async"]);
    }
    {
        let files = [
            ("src/lib.rs", "wrapper! { mod a; pub mod b; }"),
            ("src/a.rs", ""),
            ("src/b/mod.rs", ""),
        ];
        assert_eq!(
            modules(&Crate::from_memory("src/lib.rs", &files).unwrap()),
            ["crate", "crate::a", "crate::b"]
        );
    }
    {
        let files = [
            ("src/lib.rs", ""),
            ("src/main.rs", "fn main() {}"),
            ("src/bin/tool.rs", "fn main() {}"),
        ];
        assert_eq!(
            modules(&Crate::from_memory("src/lib.rs", &files).unwrap()),
            ["crate"]
        );
    }
    {
        let fixture = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rust/app");
        let krate = Crate::options(fixture)
            .resolving_members()
            .root("src/interpreter/mod.rs")
            .allow_unreached_files(Vec::<String>::new())
            .load()
            .unwrap();
        assert_eq!(modules(&krate), ["crate", "crate::heap", "crate::env"]);
    }
}

/// A module without a file, a file that does not parse, a missing root, and a file that no
/// `mod` declaration reaches stop the load. Unreached files can be allowed, and an allowance
/// that matches nothing is an error. A `mod` declaration inside a macro whose argument is not
/// a list of items is not seen, so its file is such an unreached file.
#[test]
fn load_errors() {
    {
        let error = Crate::from_memory("src/lib.rs", &[("src/lib.rs", "\nmod gone;")]).unwrap_err();
        assert_eq!(
            error,
            LoadError::ModuleFileNotFound {
                module: "crate::gone".to_owned(),
                declared_in: "src/lib.rs".to_owned(),
                line: 2,
                tried: vec!["src/gone.rs".to_owned(), "src/gone/mod.rs".to_owned()],
            }
        );
    }
    {
        let error = Crate::from_memory("lib.rs", &[("lib.rs", "mod a;"), ("a.rs", "fn broken( {")])
            .unwrap_err();
        assert!(
            matches!(error, LoadError::Parse { ref file, line: 1, .. } if file == "a.rs"),
            "{error:?}"
        );
    }
    {
        let error = Crate::from_memory("src/lib.rs", &[("src/other.rs", "")]).unwrap_err();
        assert_eq!(
            error,
            LoadError::RootNotFound {
                tried: vec!["src/lib.rs".to_owned()]
            }
        );
        let error = Crate::options(concat!(env!("CARGO_MANIFEST_DIR"), "/tests"))
            .resolving_members()
            .load()
            .unwrap_err();
        assert!(matches!(error, LoadError::RootNotFound { .. }));
    }
    {
        let files = [
            ("src/lib.rs", "mod a; my_macro! { modules: [hidden] }"),
            ("src/a.rs", ""),
            ("src/hidden.rs", "use std::fs;"),
        ];
        let error = Crate::from_memory("src/lib.rs", &files).unwrap_err();
        assert_eq!(
            error,
            LoadError::UnreachedFiles {
                files: vec!["src/hidden.rs".to_owned()]
            }
        );
    }
    {
        let dir = std::env::temp_dir().join(format!("refpact-loading-{}", std::process::id()));
        let _removed = Removed(dir.clone());
        std::fs::create_dir_all(dir.join("src/generated")).unwrap();
        std::fs::write(dir.join("src/lib.rs"), "").unwrap();
        std::fs::write(dir.join("src/generated/table.rs"), "").unwrap();

        let unreached = Crate::options(&dir).resolving_members().load().unwrap_err();
        // The directory has no manifest, which rust-analyzer needs.
        let allowed = Crate::options(&dir)
            .allow_unreached_files(["src/generated"])
            .load();
        let stale = Crate::options(&dir)
            .resolving_members()
            .allow_unreached_files(["src/generated", "src/old"])
            .load()
            .unwrap_err();

        assert_eq!(
            unreached,
            LoadError::UnreachedFiles {
                files: vec!["src/generated/table.rs".to_owned()]
            }
        );
        assert!(allowed.is_ok());
        assert_eq!(
            stale,
            LoadError::StaleAllowance {
                prefixes: vec!["src/old".to_owned()]
            }
        );

        // With a manifest, the directory is also a workspace of one package. That package
        // has no directory below the workspace for the error to name.
        let manifest = "[package]\nname = \"alone\"\nversion = \"0.0.0\"\nedition = \"2021\"\n";
        std::fs::write(dir.join("Cargo.toml"), manifest).unwrap();
        let root = Workspace::load(&dir).unwrap_err().to_string();
        let start = "in the crate at the root of the workspace: these files are not reached";
        assert!(root.starts_with(start), "{root}");
    }
}

/// The targets of the references of one file, with the module that each one is in.
fn targets(krate: &Crate, file: &str) -> Vec<String> {
    krate
        .facts()
        .facts
        .iter()
        .filter(|fact| fact.file == file)
        .filter_map(|fact| match &fact.what {
            refpact::core::What::Reference(reference) => Some(format!(
                "{} -> {}",
                fact.unit.join("::"),
                reference.target.join("::")
            )),
            _ => None,
        })
        .collect()
}

// The file is parsed one time. Its `use` is resolved in each module that names the file.
#[test]
fn one_file_of_two_modules_resolves_its_uses_in_each_module() {
    let krate = Crate::from_memory(
        "src/lib.rs",
        &[
            (
                "src/lib.rs",
                r#"mod a { pub mod dep { pub struct Item; } #[path = "../shared.rs"] mod shared; }
                   mod b { pub mod dep { pub struct Item; } #[path = "../shared.rs"] mod shared; }"#,
            ),
            ("src/shared.rs", "use super::dep::Item;\npub fn make() -> Item { Item }\n"),
        ],
    )
    .unwrap();
    let found = targets(&krate, "src/shared.rs");
    assert!(
        found.contains(&"crate::a::shared -> crate::a::dep::Item".to_owned()),
        "{found:?}"
    );
    assert!(
        found.contains(&"crate::b::shared -> crate::b::dep::Item".to_owned()),
        "{found:?}"
    );
    assert!(
        found
            .iter()
            .all(|line| line.starts_with("crate::a::shared -> crate::a::")
                || line.starts_with("crate::b::shared -> crate::b::")),
        "{found:?}"
    );
}

// Several threads parse the files. The result must not depend on which thread is first.
#[test]
fn the_facts_are_the_same_each_time_and_in_the_order_of_the_files() {
    let mut files: Vec<(String, String)> = vec![(
        "src/lib.rs".to_owned(),
        (0..40)
            .map(|index| format!("pub mod m{index};\n"))
            .collect::<String>(),
    )];
    for index in 0..40 {
        let other = (index + 1) % 40;
        // Each file is a little larger than the one before, so the threads end out of order.
        let filler: String = (0..index * 4)
            .map(|line| format!("pub fn f{line}() -> u32 {{ {line} }}\n"))
            .collect();
        files.push((
            format!("src/m{index}.rs"),
            format!("use crate::m{other}::Thing as Other;\npub struct Thing;\n{filler}pub fn other() -> Other {{ Other }}\n"),
        ));
    }
    let borrowed: Vec<(&str, &str)> = files
        .iter()
        .map(|(path, text)| (path.as_str(), text.as_str()))
        .collect();
    let first = Crate::from_memory("src/lib.rs", &borrowed).unwrap();

    let order: Vec<&str> = first
        .facts()
        .facts
        .iter()
        .map(|fact| fact.file.as_str())
        .collect();
    let mut by_tree: Vec<String> = Vec::new();
    for file in &order {
        if by_tree.last().map(String::as_str) != Some(*file) {
            by_tree.push((*file).to_owned());
        }
    }
    let expected: Vec<String> = (0..40).map(|index| format!("src/m{index}.rs")).collect();
    assert_eq!(by_tree, expected);

    for _ in 0..5 {
        assert_eq!(
            Crate::from_memory("src/lib.rs", &borrowed).unwrap().facts(),
            first.facts()
        );
    }
}
