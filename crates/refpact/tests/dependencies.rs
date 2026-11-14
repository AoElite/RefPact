//! The dependencies that the manifests name, which a load reads with `reading_dependencies`:
//! each is a reference from the root module of a crate to the crate of a package.

use std::sync::LazyLock;

use std::collections::{BTreeSet, HashMap};

use refpact::core::What;
use refpact::{
    all, any_of, layers, rule, Crate, Detail, Kind, LoadError, Rule, Scope, Violation, Workspace,
    PROPERTY_LICENSE, PROPERTY_SOURCE, PROPERTY_VERSION,
};

fn fixture(path: &str) -> String {
    format!(
        "{}/../../fixtures/rust/packages{path}",
        env!("CARGO_MANIFEST_DIR")
    )
}

fn options() -> refpact::WorkspaceOptions {
    Workspace::options(fixture("")).reading_dependencies()
}

static SHOP: LazyLock<Workspace> = LazyLock::new(|| options().load().unwrap());

/// The references of a manifest: `line kind target [aliases] {tags} (note)`.
fn named(code: &Crate, file: &str) -> Vec<String> {
    let facts = code.facts().facts.iter();
    facts
        .filter(|fact| fact.file == file)
        .filter_map(|fact| {
            let What::Reference(reference) = &fact.what else {
                return None;
            };
            let mut text = format!(
                "{} {} {}",
                fact.line,
                reference.kind,
                reference.target.join("::")
            );
            assert_eq!((fact.column, &fact.item), (1, &None));
            assert_eq!(reference.certainty.as_str(), "exact");
            for alias in &reference.aliases {
                text.push_str(&format!(" [{}]", alias.join("::")));
            }
            if !fact.tags.is_empty() {
                text.push_str(&format!(" {{{}}}", fact.tags.join(", ")));
            }
            if let Some(note) = &fact.note {
                text.push_str(&format!(" ({note})"));
            }
            Some(text)
        })
        .collect()
}

#[test]
fn each_dependency_of_a_manifest_is_a_reference_of_the_root_module() {
    assert_eq!(
        named(&SHOP, "api/Cargo.toml"),
        [
            "21 build-dependency cfg_if",
            "14 dependency derives",
            "12 dependency facade",
            "25 dependency getrandom {cfg(windows)}",
            "17 dev-dependency logkit {test}",
            // A renamed dependency has the name that the code writes and that of the crate.
            "13 dependency random [rand] (the package `rand`)",
            // The cli needs the api and the facade, which are no packages that the api needs.
            "19 dev-dependency shop_cli {test}",
            "11 dependency shop_domain",
            // What the dependencies need, at the line of the dependency.
            "12 transitive-dependency engine (through `facade`)",
            // What the cli needs, the tests of the api need, under the conditions of the cli.
            "19 transitive-dependency tooling {test, cfg(unix)} (through `shop_cli`)",
            "11 transitive-dependency rand_core (through `shop_domain`)",
            // Another version of the package that the facade needs: another package.
            "19 transitive-dependency engine {test, cfg(unix)} (through `shop_cli`, `tooling`)",
            // The manifest names it for Windows. The tests need it on Unix too.
            "19 transitive-dependency getrandom {test, cfg(unix)} (through `shop_cli`, `tooling`)",
        ]
    );
    assert_eq!(
        named(&SHOP, "domain/Cargo.toml"),
        [
            "11 dev-dependency facade {test}",
            // A dependency of two tables is at the line of each.
            "8 dependency rand_core",
            "12 dev-dependency rand_core {test}",
            // Only a dependency for the tests needs it.
            "11 transitive-dependency engine {test} (through `facade`)",
        ]
    );
    assert_eq!(
        named(&SHOP, "cli/Cargo.toml"),
        [
            "12 dependency facade {cfg(unix)}",
            // The manifest names it, so it is not also a package that the api needs.
            "9 dependency rand_core",
            "8 dependency shop_api",
            // The manifest names the package; the code writes the name of its library.
            "13 dependency tooling {cfg(unix)}",
            // Also what the build script of a dependency needs.
            "8 transitive-dependency cfg_if (through `shop_api`)",
            "8 transitive-dependency derives (through `shop_api`)",
            // The manifest names it for one platform. The api needs it on all.
            "8 transitive-dependency facade (through `shop_api`)",
            // On one platform only. What the tests of another package need, `logkit`, is not
            // needed.
            "8 transitive-dependency getrandom {cfg(windows)} (through `shop_api`)",
            "8 transitive-dependency rand (through `shop_api`)",
            "8 transitive-dependency shop_domain (through `shop_api`)",
            "13 transitive-dependency engine {cfg(unix)} (through `tooling`)",
            // A second way to a package, on which another platform needs it.
            "13 transitive-dependency getrandom {cfg(unix)} (through `tooling`)",
            // The packages that are further away come after the nearer ones.
            "8 transitive-dependency engine (through `shop_api`, `facade`)",
        ]
    );
    let units = SHOP.facts().facts.iter();
    let units: Vec<String> = units
        .filter(|fact| fact.file == "api/Cargo.toml")
        .map(|fact| fact.unit.join("::"))
        .collect();
    assert!(units.iter().all(|unit| unit == "shop_api"), "{units:?}");
}

/// The places of the violations of a rule: `file:line target`.
fn violations(rule: Rule, code: &Crate) -> Vec<String> {
    let found = rule.evaluate(code).unwrap();
    found
        .iter()
        .map(|violation| match &violation.detail {
            refpact::Detail::Uses { target, .. } => {
                format!(
                    "{}:{} {}",
                    violation.file,
                    violation.line,
                    target.join("::")
                )
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

#[test]
fn a_rule_says_which_package_may_depend_on_which() {
    let without = |packages: &[&str]| {
        rule("built without")
            .modules(any_of(["shop_cli", "shop_api", "shop_domain"]))
            .must_not_use(any_of(packages.iter().copied()))
    };
    // Directly or through other packages.
    assert_eq!(
        violations(without(&["engine"]), &SHOP),
        [
            "api/Cargo.toml:12 engine",
            "api/Cargo.toml:19 engine",
            "cli/Cargo.toml:13 engine",
            "cli/Cargo.toml:8 engine",
            "domain/Cargo.toml:11 engine"
        ]
    );
    assert_eq!(
        violations(without(&["engine"]).except_in_test_code(), &SHOP),
        [
            "api/Cargo.toml:12 engine",
            "cli/Cargo.toml:13 engine",
            "cli/Cargo.toml:8 engine"
        ]
    );
    assert_eq!(
        violations(without(&["facade"]).only_kinds([Kind::Dependency]), &SHOP),
        ["api/Cargo.toml:12 facade", "cli/Cargo.toml:12 facade"]
    );
    assert_eq!(
        violations(
            without(&["facade"]).only_kinds([Kind::DevDependency]),
            &SHOP
        ),
        ["domain/Cargo.toml:11 facade"]
    );
    assert_eq!(
        violations(
            without(&["cfg_if"]).only_kinds([Kind::BuildDependency]),
            &SHOP
        ),
        ["api/Cargo.toml:21 cfg_if"]
    );
    assert_eq!(
        violations(
            without(&["facade"]).only_kinds([Kind::TransitiveDependency]),
            &SHOP
        ),
        ["cli/Cargo.toml:8 facade"]
    );
    assert_eq!(
        violations(
            without(&["rand_core"]).only_kinds([Kind::TransitiveDependency]),
            &SHOP
        ),
        ["api/Cargo.toml:11 rand_core"]
    );
    // The name of the package and the name that the code writes are both found.
    assert_eq!(
        violations(without(&["rand"]).only_kinds([Kind::Dependency]), &SHOP),
        ["api/Cargo.toml:13 rand"]
    );
    // The code of the api writes the name two times.
    assert_eq!(violations(without(&["random"]), &SHOP).len(), 3);
    // A dependency for one platform is under its condition. The cli needs the package on
    // two ways: through the api on Windows, and through its tools on Unix.
    assert_eq!(
        violations(without(&["getrandom"]).except_in_test_code(), &SHOP),
        [
            "api/Cargo.toml:25 getrandom",
            "cli/Cargo.toml:8 getrandom",
            "cli/Cargo.toml:13 getrandom"
        ]
    );
    let other = without(&["getrandom"])
        .except_in_test_code()
        .except_under_cfg("windows");
    assert_eq!(violations(other, &SHOP), ["cli/Cargo.toml:13 getrandom"]);
    let neither = without(&["getrandom"])
        .except_under_cfg("windows")
        .except_under_cfg("unix");
    assert_eq!(violations(neither, &SHOP), Vec::<String>::new());

    let failure = without(&["rand_core"])
        .only_kinds([Kind::TransitiveDependency])
        .verify(&SHOP)
        .unwrap_err();
    assert_eq!(
        failure.to_string().lines().nth(1),
        Some(
            "  api/Cargo.toml:11:1: [built without] shop_api uses rand_core \
             (transitive-dependency, through `shop_domain`)"
        )
    );
}

#[test]
fn a_dependency_on_a_member_is_a_use_of_the_member() {
    // No code of the domain uses the api, and its manifest does not name it.
    let made = || {
        layers("the shop")
            .layer("cli", any_of(["shop_cli"]))
            .layer("domain", any_of(["shop_domain"]))
            .layer("api", any_of(["shop_api"]))
    };
    let failure = made().verify(&SHOP).unwrap_err().to_string();
    assert!(
        failure.contains("api/Cargo.toml:11:1: [the shop: api does not use the layers above it] shop_api uses shop_domain (dependency)"),
        "{failure}"
    );
    layers("the shop")
        .layer("cli", any_of(["shop_cli"]))
        .layer("api", any_of(["shop_api"]))
        .layer("domain", any_of(["shop_domain"]))
        // The tests of the api need the cli.
        .except_in_test_code()
        .check(&SHOP);
}

#[test]
fn a_load_for_a_target_leaves_out_the_dependencies_of_other_platforms() {
    let linux = options().target("x86_64-unknown-linux-gnu").load().unwrap();
    let on = |code: &Crate| {
        let found = named(code, "api/Cargo.toml").into_iter();
        let cli = named(code, "cli/Cargo.toml").into_iter();
        found
            .chain(cli)
            .filter(|text| text.contains("getrandom"))
            .count()
    };
    // On Linux, only the tools of the cli need it, and so do the tests of the api.
    assert_eq!((on(&SHOP), on(&linux)), (4, 2));
    let windows = options().target("x86_64-pc-windows-msvc").load().unwrap();
    assert_eq!(on(&windows), 2);
    // The cli names the facade for Unix, and needs it on all platforms through the api.
    let facade = |code: &Crate| {
        let found = named(code, "cli/Cargo.toml").into_iter();
        found
            .filter(|text| text.contains(" facade"))
            .collect::<Vec<_>>()
    };
    assert_eq!(facade(&linux).len(), 2);
    assert_eq!(
        facade(&windows),
        ["8 transitive-dependency facade (through `shop_api`)"]
    );
}

#[test]
fn the_dependencies_are_read_only_when_the_load_says_so() {
    let plain = Workspace::options(fixture("")).load().unwrap();
    assert_eq!(named(&plain, "api/Cargo.toml"), Vec::<String>::new());
    let failure = rule("built without")
        .modules(any_of(["shop_api"]))
        .must_not_use(any_of(["facade"]))
        .only_kinds([Kind::Dependency])
        .verify(&plain)
        .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("kind `dependency` is not known to this front end"),
        "{failure}"
    );
    // Neither is a property of a dependency.
    let failure = rule("licensed")
        .modules(all())
        .must_not_use(any_of(["facade"]))
        .reading_properties([PROPERTY_LICENSE])
        .verify(&plain)
        .unwrap_err();
    assert!(
        failure
            .to_string()
            .contains("property `license` is not known to this front end"),
        "{failure}"
    );
    // The other facts are the same.
    let code = |code: &Crate| {
        let facts = code.facts().facts.iter();
        facts
            .filter(|fact| !fact.file.ends_with("Cargo.toml"))
            .cloned()
            .collect::<Vec<_>>()
    };
    assert_eq!(code(&plain), code(&SHOP));
}

#[test]
fn one_crate_names_its_dependencies_from_its_root_module() {
    let api = Crate::options(fixture("/api"))
        .reading_dependencies()
        .load()
        .unwrap();
    assert_eq!(named(&api, "Cargo.toml"), named(&SHOP, "api/Cargo.toml"));
    let units = api.facts().facts.iter();
    let mut units = units.filter(|fact| fact.file == "Cargo.toml");
    assert!(units.all(|fact| fact.unit == ["crate"]));
    rule("built without the engine")
        .modules(any_of(["crate"]))
        .must_not_use(any_of(["logkit"]))
        .except_in_test_code()
        .check(&api);
}

/// The properties of the dependencies of a manifest: `line target name=value ..`.
fn properties(code: &Crate, file: &str) -> Vec<String> {
    let facts = code.facts().facts.iter();
    facts
        .filter(|fact| fact.file == file)
        .filter_map(|fact| {
            let What::Reference(reference) = &fact.what else {
                return None;
            };
            let mut text = format!("{} {}", fact.line, reference.target.join("::"));
            for one in &reference.properties {
                text.push_str(&format!(" {}={}", one.name, one.value));
            }
            Some(text)
        })
        .collect()
}

/// Each dependency says what Cargo knows of its package: the version, and the licence when
/// the manifest of the package names one. A package of a local path has no source.
#[test]
fn a_dependency_has_the_version_and_the_licence_of_its_package() {
    assert_eq!(
        properties(&SHOP, "cli/Cargo.toml"),
        [
            "12 facade version=0.0.0 license=MIT",
            "9 rand_core version=0.0.0 license=MIT OR Apache-2.0",
            "8 shop_api version=0.0.0",
            "13 tooling version=0.0.0",
            "8 cfg_if version=0.0.0 license=MIT",
            "8 derives version=0.0.0",
            "8 facade version=0.0.0 license=MIT",
            "8 getrandom version=0.0.0",
            "8 rand version=0.0.0 license=MIT OR Apache-2.0",
            "8 shop_domain version=0.0.0",
            // Two versions of one package are two packages.
            "13 engine version=1.0.0 license=MIT",
            "13 getrandom version=0.0.0",
            "8 engine version=0.0.0 license=GPL-3.0-only",
        ]
    );
    // A renamed dependency has the properties of the package behind the name.
    assert!(properties(&SHOP, "api/Cargo.toml")
        .contains(&"13 random version=0.0.0 license=MIT OR Apache-2.0".to_owned()));
    let source = |fact: &refpact::core::Fact| match &fact.what {
        What::Reference(reference) => reference.property(PROPERTY_SOURCE).is_some(),
        What::Marker(_) => false,
    };
    assert!(!SHOP.facts().facts.iter().any(source));
    // What the code names has no properties.
    let code = SHOP.facts().facts.iter();
    let code = code.filter(|fact| !fact.file.ends_with("Cargo.toml"));
    assert!(code.clone().count() > 0);
    for fact in code {
        if let What::Reference(reference) = &fact.what {
            assert!(reference.properties.is_empty(), "{fact:?}");
        }
    }
}

/// The places and the messages of the violations of a check: `file:line message (note)`.
fn found(rule: Rule, code: &Crate) -> Vec<String> {
    let found = rule.evaluate(code).unwrap();
    found
        .iter()
        .map(|violation| match &violation.detail {
            Detail::Custom { message, .. } => {
                let note = violation.note.as_deref().unwrap_or("named");
                format!("{}:{} {message} ({note})", violation.file, violation.line)
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect()
}

/// A check of the user reads the licence of each package: the example of `docs/rust.md`.
#[test]
fn a_check_accepts_some_licences_only() {
    let accepted = |licences: &'static [&'static str]| {
        rule("every package has a licence that we accept")
            .modules(all())
            .must_satisfy(move |scope: &Scope<'_>| -> Vec<Violation> {
                scope
                    .references()
                    .filter(|seen| scope.selects(seen.unit))
                    // A member of the workspace is no package of others.
                    .filter(|seen| scope.owners(seen).all(|(_, owner)| owner.is_none()))
                    .filter_map(|seen| {
                        let licence = seen.reference.property(PROPERTY_LICENSE);
                        let accepted = licence.is_some_and(|name| licences.contains(&name));
                        (!accepted).then(|| {
                            let message = format!(
                                "is built with {}, which has {}",
                                seen.reference.target.join("::"),
                                licence.map_or("no licence".to_owned(), |name| format!(
                                    "the licence {name}"
                                ))
                            );
                            scope.violation_at(seen.fact, Detail::custom(message))
                        })
                    })
                    .collect()
            })
            .reading_properties([PROPERTY_LICENSE])
            .only_kinds([Kind::Dependency, Kind::TransitiveDependency])
            .except_in_test_code()
    };
    assert_eq!(
        found(accepted(&["MIT", "MIT OR Apache-2.0"]), &SHOP),
        [
            "api/Cargo.toml:14 is built with derives, which has no licence (named)",
            "api/Cargo.toml:25 is built with getrandom, which has no licence (named)",
            "api/Cargo.toml:12 is built with engine, which has the licence GPL-3.0-only (through `facade`)",
            "cli/Cargo.toml:8 is built with derives, which has no licence (through `shop_api`)",
            "cli/Cargo.toml:8 is built with getrandom, which has no licence (through `shop_api`)",
            "cli/Cargo.toml:13 is built with getrandom, which has no licence (through `tooling`)",
            "cli/Cargo.toml:8 is built with engine, which has the licence GPL-3.0-only (through `shop_api`, `facade`)",
            "tools/Cargo.toml:14 is built with getrandom, which has no licence (named)",
        ]
    );
    let all = found(
        accepted(&["MIT", "MIT OR Apache-2.0", "GPL-3.0-only"]),
        &SHOP,
    );
    assert!(
        all.iter().all(|text| text.contains("no licence")),
        "{all:?}"
    );
    // A property that no dependency can have is an error, not a check that finds nothing.
    let misspelled = accepted(&["MIT"]).reading_properties(["licence"]);
    let failure = misspelled.verify(&SHOP).unwrap_err().to_string();
    assert!(
        failure.contains("property `licence` is not known to this front end"),
        "{failure}"
    );
}

/// A check of the user compares the versions of each package: the example of
/// `docs/rust.md`.
#[test]
fn a_check_finds_a_package_in_two_versions() {
    let one_version = rule("a build has one version of each package")
        .modules(all())
        .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
            let mut versions: HashMap<&[String], BTreeSet<&str>> = HashMap::new();
            for seen in scope.references() {
                if let Some(version) = seen.reference.property(PROPERTY_VERSION) {
                    let of_package = versions.entry(&seen.reference.target).or_default();
                    of_package.insert(version);
                }
            }
            scope
                .references()
                .filter(|seen| scope.selects(seen.unit))
                .filter_map(|seen| {
                    let all = versions.get(seen.reference.target.as_slice())?;
                    let version = seen.reference.property(PROPERTY_VERSION)?;
                    let name = seen.reference.target.join("::");
                    (all.len() > 1).then(|| {
                        let message = format!("is built with {name} {version}, one of {all:?}");
                        let key = format!("is built with several versions of {name}");
                        scope.violation_at(seen.fact, Detail::custom_with_key(message, key))
                    })
                })
                .collect()
        })
        .reading_properties([PROPERTY_VERSION]);
    assert_eq!(
        found(one_version.clone(), &SHOP),
        [
            "api/Cargo.toml:12 is built with engine 0.0.0, one of {\"0.0.0\", \"1.0.0\"} (through `facade`)",
            "api/Cargo.toml:19 is built with engine 1.0.0, one of {\"0.0.0\", \"1.0.0\"} (through `shop_cli`, `tooling`)",
            "cli/Cargo.toml:13 is built with engine 1.0.0, one of {\"0.0.0\", \"1.0.0\"} (through `tooling`)",
            "cli/Cargo.toml:8 is built with engine 0.0.0, one of {\"0.0.0\", \"1.0.0\"} (through `shop_api`, `facade`)",
            "domain/Cargo.toml:11 is built with engine 0.0.0, one of {\"0.0.0\", \"1.0.0\"} (through `facade`)",
            "tools/Cargo.toml:16 is built with engine 1.0.0, one of {\"0.0.0\", \"1.0.0\"} (named)",
        ]
    );
    // The filters of the rule say which packages count: the manifests name one version.
    let named = one_version.only_kinds([Kind::Dependency]);
    assert_eq!(found(named, &SHOP), Vec::<String>::new());
}

/// The dependencies are those of a build with the features of the load.
#[test]
fn the_features_of_a_load_say_which_optional_dependencies_there_are() {
    let load = |features: &[&str]| {
        Crate::options(fixture("/api"))
            .reading_dependencies()
            .features(features.iter().copied())
            .load()
    };
    let derives = |code: &Crate| {
        let found = named(code, "Cargo.toml").into_iter();
        found.filter(|text| text.contains("derives")).count()
    };
    // Without a list of features, the build has all of them.
    assert_eq!(derives(&load(&["derive"]).unwrap()), 1);
    assert_eq!(derives(&load(&[]).unwrap()), 0);
    assert_eq!(named(&load(&[]).unwrap(), "Cargo.toml").len(), 12);
    // In a workspace, the features are those of all members.
    let shop = options().features(["derive"]).load().unwrap();
    assert!(named(&shop, "cli/Cargo.toml")
        .contains(&"8 transitive-dependency derives (through `shop_api`)".to_owned()));
    let none = options().features(Vec::<String>::new()).load().unwrap();
    assert_eq!(
        named(&none, "cli/Cargo.toml").len() + 1,
        named(&shop, "cli/Cargo.toml").len()
    );
    // Cargo knows the features. One that no manifest declares is an error.
    let error = load(&["no-such-feature"]).unwrap_err();
    assert!(
        matches!(&error, LoadError::Manifest { message, .. } if message.contains("no-such-feature")),
        "{error}"
    );
}

/// Cargo says what the members are, what their crates are named and where their roots are.
#[test]
fn the_members_and_their_crates_are_those_that_cargo_names() {
    // No `members` entry names the tools. The cli depends on them by a path.
    let names: Vec<&str> = SHOP
        .members()
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(names, ["shop_api", "shop_cli", "shop_domain", "tooling"]);
    let dirs: Vec<&str> = SHOP
        .members()
        .iter()
        .map(|member| member.dir.as_str())
        .collect();
    assert_eq!(dirs, ["api", "cli", "domain", "tools"]);
    // The library of the tools has a name and a root file of its own.
    assert!(SHOP.modules().any(|module| module == "tooling::seed"));
    rule("the cli uses its tools")
        .modules(any_of(["shop_api", "shop_domain"]))
        .must_not_use(any_of(["tooling::seed::seed"]))
        .check(&SHOP);
    // One crate has the same root.
    let tools = Crate::options(fixture("/tools")).load().unwrap();
    let modules: Vec<String> = tools.modules().collect();
    assert_eq!(modules, ["crate", "crate::seed"]);
    // A path of a dependency that the manifest renames is a path of the crate of the package.
    let plain = Workspace::options(fixture("")).load().unwrap();
    let found = rule("the api has no random numbers")
        .modules(any_of(["shop_api"]))
        .must_not_use(any_of(["rand::rngs"]))
        .evaluate(&plain)
        .unwrap();
    let places: Vec<(u32, Option<String>)> = found
        .iter()
        .map(|violation| match &violation.detail {
            refpact::Detail::Uses { written, .. } => {
                (violation.line, written.as_ref().map(|path| path.join("::")))
            }
            other => panic!("unexpected {other:?}"),
        })
        .collect();
    let written = Some("random::rngs::StdRng".to_owned());
    assert_eq!(places, [(6, written.clone()), (7, written)]);
    // A subset is named by its directories.
    let two = Workspace::options(fixture(""))
        .members(["tools", "api"])
        .load()
        .unwrap();
    let names: Vec<&str> = two
        .members()
        .iter()
        .map(|member| member.name.as_str())
        .collect();
    assert_eq!(names, ["tooling", "shop_api"]);
    // The directory of a workspace is no crate.
    let error = Crate::options(fixture("")).load().unwrap_err();
    assert!(
        error
            .to_string()
            .contains("Cargo names no package in this directory"),
        "{error}"
    );
}

#[test]
fn a_directory_that_cargo_cannot_read_is_a_load_error() {
    let dir = std::env::temp_dir().join(format!("refpact-packages-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    std::fs::write(dir.join("src/lib.rs"), "pub fn f() {}\n").unwrap();
    let load = || Crate::options(&dir).reading_dependencies().load();
    let error = load().unwrap_err();
    assert!(
        matches!(&error, LoadError::Manifest { message, .. } if message.contains("metadata` failed")),
        "{error}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}
