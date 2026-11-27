//! The `cfg` conditions of the code: each is a tag of the facts under it, a rule can leave
//! one out, and a load for one target leaves out what the target switches off.

mod support;

use std::sync::LazyLock;

use refpact::core::What;
use refpact::{any_of, layers, onion_architecture, rule, Crate, DiagramFormat};

const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../fixtures/rust/platforms");

static ALL: LazyLock<Crate> = LazyLock::new(|| Crate::options(FIXTURE).load().unwrap());

/// The references of a file: `line target [tags]`.
fn references(code: &Crate, file: &str) -> Vec<String> {
    let facts = code.facts().facts.iter();
    facts
        .filter(|fact| fact.file == file)
        .filter_map(|fact| match &fact.what {
            What::Reference(reference) => Some(format!(
                "{} {} [{}]",
                fact.line,
                reference.target.join("::"),
                fact.tags.join(", ")
            )),
            What::Marker(_) => None,
        })
        .collect()
}

fn modules(code: &Crate) -> Vec<String> {
    code.modules().collect()
}

fn no_files() -> refpact::Rule {
    rule("no files and no network")
        .modules(any_of(["crate::**"]))
        .must_not_use(any_of(["std::fs", "std::net"]))
}

#[test]
fn each_condition_around_a_fact_is_a_tag() {
    assert_eq!(
        references(&ALL, "src/shared.rs"),
        [
            "1 std::string::String []",
            // A statement under a condition.
            "3 std::env::var [cfg(windows)]",
            "5 std::env::var [cfg(not(windows))]",
            // Each part of `all(..)` is a condition.
            "10 std::io::Result [cfg(unix), cfg(feature = \"tls\")]",
            "10 std::net::TcpStream [cfg(unix), cfg(feature = \"tls\")]",
            "11 std::net::TcpStream::connect [cfg(unix), cfg(feature = \"tls\")]",
            "18 std::time::Instant::now [test, cfg(test)]",
        ]
    );
    // A module that is declared under a condition.
    assert_eq!(
        references(&ALL, "src/win.rs"),
        [
            "1 std::io::Result [cfg(windows)]",
            "1 std::fs::File [cfg(windows)]",
            "2 std::fs::File::open [cfg(windows)]",
        ]
    );
    let secure = ALL.facts().declarations.iter();
    let secure = secure
        .filter(|found| found.name == "secure")
        .collect::<Vec<_>>();
    assert_eq!(secure[0].tags, ["cfg(unix)", "cfg(feature = \"tls\")"]);
}

#[test]
fn a_branch_of_cfg_if_is_under_its_condition_and_not_under_those_before() {
    assert_eq!(
        references(&ALL, "src/lib.rs"),
        [
            "9 cfg_if::cfg_if! []",
            "11 std::path::PathBuf [cfg(target_os = \"macos\")]",
            "12 std::path::PathBuf::from [cfg(target_os = \"macos\")]",
            "15 std::path::PathBuf [cfg(unix), cfg(not(target_os = \"macos\"))]",
            "16 std::path::PathBuf::from [cfg(unix), cfg(not(target_os = \"macos\"))]",
            "19 std::path::PathBuf [cfg(not(target_os = \"macos\")), cfg(not(unix))]",
            "20 std::env::temp_dir [cfg(not(target_os = \"macos\")), cfg(not(unix))]",
        ]
    );
}

#[test]
fn a_rule_checks_all_conditions_and_can_leave_one_out() {
    let failure = no_files().verify(&ALL).unwrap_err().to_string();
    let places: Vec<&str> = failure
        .lines()
        .skip(1)
        .filter_map(|line| line.trim().split(": ").next())
        .collect();
    assert_eq!(
        places,
        [
            "src/shared.rs:10:36",
            "src/shared.rs:11:5",
            "src/win.rs:1:38",
            "src/win.rs:2:5"
        ]
    );
    let failure = no_files().except_under_cfg("windows").verify(&ALL);
    assert_eq!(failure.unwrap_err().to_string().lines().count(), 3);
    // The spaces of the condition do not count.
    no_files()
        .except_under_cfg("windows")
        .except_under_cfg("feature=\"tls\"")
        .check(&ALL);

    // A condition that no code is under is an error.
    let error = no_files().except_under_cfg("target_os = \"redox\"");
    let error = error.verify(&ALL).unwrap_err().to_string();
    assert!(
        error.contains("tag `cfg(target_os = \"redox\")` is not known to this front end"),
        "{error}"
    );
}

#[test]
fn a_load_for_a_target_leaves_out_what_the_target_switches_off() {
    assert_eq!(
        modules(&ALL),
        [
            "crate",
            "crate::nix",
            "crate::shared",
            "crate::shared::tests",
            "crate::win"
        ]
    );
    let options = || Crate::options(FIXTURE);
    let linux = options().target("x86_64-unknown-linux-gnu").load().unwrap();
    assert_eq!(
        modules(&linux),
        [
            "crate",
            "crate::nix",
            "crate::shared",
            "crate::shared::tests"
        ]
    );
    assert_eq!(
        references(&linux, "src/shared.rs"),
        [
            "1 std::string::String []",
            "5 std::env::var [cfg(not(windows))]",
            // The target does not decide a feature or `test`.
            "10 std::io::Result [cfg(unix), cfg(feature = \"tls\")]",
            "10 std::net::TcpStream [cfg(unix), cfg(feature = \"tls\")]",
            "11 std::net::TcpStream::connect [cfg(unix), cfg(feature = \"tls\")]",
            "18 std::time::Instant::now [test, cfg(test)]",
        ]
    );
    assert_eq!(
        references(&linux, "src/lib.rs"),
        [
            "9 cfg_if::cfg_if! []",
            "15 std::path::PathBuf [cfg(unix), cfg(not(target_os = \"macos\"))]",
            "16 std::path::PathBuf::from [cfg(unix), cfg(not(target_os = \"macos\"))]",
        ]
    );
    assert_eq!(references(&linux, "src/win.rs"), Vec::<String>::new());
    let declared = |code: &Crate| {
        let found = code.facts().declarations.iter();
        found.map(|found| found.path.join("::")).collect::<Vec<_>>()
    };
    assert!(declared(&ALL).contains(&"crate::win::registry".to_owned()));
    assert!(!declared(&linux).contains(&"crate::win::registry".to_owned()));

    let windows = options().target("x86_64-pc-windows-msvc").load().unwrap();
    assert_eq!(
        modules(&windows),
        [
            "crate",
            "crate::shared",
            "crate::shared::tests",
            "crate::win"
        ]
    );
    assert_eq!(
        references(&windows, "src/shared.rs"),
        [
            "1 std::string::String []",
            "3 std::env::var [cfg(windows)]",
            "18 std::time::Instant::now [test, cfg(test)]"
        ]
    );
    assert_eq!(references(&windows, "src/lib.rs").len(), 3);

    // The target of this machine has one of the two modules.
    let host = options().target("host").load().unwrap();
    assert_eq!(modules(&host).len(), 4);

    let error = options().target("no-such-target").load().unwrap_err();
    assert!(
        error
            .to_string()
            .starts_with("the load needs the `cfg` options of its target"),
        "{error}"
    );
}

#[test]
fn a_load_with_features_leaves_out_the_code_of_the_other_features() {
    let options = || Crate::options(FIXTURE);
    let plain = options().features(Vec::<String>::new()).load().unwrap();
    assert_eq!(references(&plain, "src/shared.rs").len(), 4);
    // The modules of both platforms are still there.
    assert_eq!(modules(&plain).len(), 5);
    let tls = options().features(["tls"]).load().unwrap();
    assert_eq!(references(&tls, "src/shared.rs").len(), 7);

    // A rule for the build of one target with the features that are shipped.
    let shipped = options()
        .target("x86_64-unknown-linux-gnu")
        .features(["tls"]);
    let failure = no_files().verify(&shipped.load().unwrap()).unwrap_err();
    assert_eq!(failure.to_string().lines().count(), 3);
    let shipped = options()
        .target("x86_64-unknown-linux-gnu")
        .features(["gui"]);
    no_files().check(&shipped.load().unwrap());
}

#[test]
fn the_index_resolves_the_calls_of_a_load_for_a_target() {
    let linux = Crate::options(FIXTURE)
        .resolving_members()
        .target("host")
        .load()
        .unwrap();
    let kinds = linux.facts().kinds.clone();
    assert!(kinds.contains(&"method-call".to_owned()));
    assert_eq!(modules(&linux).len(), 4);
}

#[test]
fn layers_and_an_onion_can_leave_a_condition_out() {
    let shop = Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod api;\npub mod db;\n"),
            ("src/api.rs", "pub fn get() {}\n"),
            (
                "src/db.rs",
                "#[cfg(windows)]\npub fn query() { crate::api::get(); }\n",
            ),
        ],
    )
    .unwrap();
    let made = || {
        layers("the layers")
            .layer("api", any_of(["crate::api"]))
            .layer("db", any_of(["crate::db"]))
    };
    assert!(made().verify(&shop).is_err());
    made().except_under_cfg("windows").check(&shop);
    let red = "linkStyle 0 stroke:#d62728";
    let drawn = made().diagram(&shop, DiagramFormat::Mermaid).unwrap();
    assert!(drawn.contains(red), "{drawn}");
    let drawn = made().except_under_cfg("windows");
    let drawn = drawn.diagram(&shop, DiagramFormat::Mermaid).unwrap();
    assert!(!drawn.contains(red), "{drawn}");

    let made = || {
        onion_architecture("the onion")
            .application_services(any_of(["crate::api"]))
            .domain_models(any_of(["crate::db"]))
    };
    assert!(made().verify(&shop).is_err());
    made().except_under_cfg("windows").check(&shop);
}

#[test]
fn a_reference_follows_each_enabled_named_reexport() {
    let dir = std::env::temp_dir().join(format!("refpact-cfg-reexports-{}", std::process::id()));
    let _remove = support::Removed(dir.clone());
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("Cargo.toml"), "[package]\nname='switch'\nversion='0.0.0'\nedition='2021'\n[features]\nfast=[]\n[workspace]\n").unwrap();
    let slow = "#[cfg(not(feature = \"fast\"))] pub use crate::slow::Driver;";
    let fast = "#[cfg(feature = \"fast\")] pub use crate::fast::Driver;";
    std::fs::write(
        dir.join("src/main.rs"),
        "pub mod binary_client { pub fn use_driver(_: switch::facade::Driver) {} } fn main() {}",
    )
    .unwrap();
    let law = |client| {
        rule("client avoids fast")
            .modules(any_of([client]))
            .must_not_use(any_of(["crate::fast"]))
    };
    for (first, second) in [(slow, fast), (fast, slow)] {
        std::fs::write(
            dir.join("src/lib.rs"),
            format!(
                "
pub mod slow {{ pub struct Driver; }}
pub mod fast {{ pub struct Driver; }}
pub mod facade {{ {first} {second} }}
pub mod client {{ pub fn use_driver(_: crate::facade::Driver) {{}}
    #[cfg(not(feature = \"fast\"))] pub fn slow_only(_: crate::facade::Driver) {{}} }}
"
            ),
        )
        .unwrap();
        let options = || Crate::options(&dir);
        for (label, app, expected) in [
            ("all", options().load().unwrap(), 1),
            ("fast", options().features(["fast"]).load().unwrap(), 1),
            (
                "slow",
                options().features(Vec::<String>::new()).load().unwrap(),
                0,
            ),
        ] {
            for client in ["crate::client", "crate::binary_client"] {
                assert_eq!(
                    law(client).evaluate(&app).unwrap().len(),
                    expected,
                    "{label}, {client}: {first}"
                );
            }
        }
    }
}
