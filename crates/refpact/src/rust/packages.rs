//! The dependencies that the manifests name: each is a reference from the root of a crate to
//! the crate of a package, so that a rule can say which package may depend on which.
//!
//! Cargo says what the packages are and what each depends on (`cargo metadata`). This module
//! reads that: it is where the members of a workspace, the names of their crates and their
//! root files come from, and what the facts of the dependencies are made of. It runs no
//! program and reads no file.

use std::collections::{HashMap, HashSet, VecDeque};
use std::path::{Path as FilePath, PathBuf};

use crate::core::{Certainty, Fact, FactSet, Path, Property, Reference, What};
use crate::rust::manifest::{self, Table};
use crate::rust::sources::json::Json;
use crate::rust::{cfg, kinds};

/// The kinds of the references that the manifests give.
pub(crate) mod kind {
    /// A dependency of `[dependencies]`.
    pub(crate) const DEPENDENCY: &str = "dependency";
    /// A dependency of `[dev-dependencies]`.
    pub(crate) const DEV_DEPENDENCY: &str = "dev-dependency";
    /// A dependency of `[build-dependencies]`.
    pub(crate) const BUILD_DEPENDENCY: &str = "build-dependency";
    /// A package that a dependency needs, and that the manifest does not name.
    pub(crate) const TRANSITIVE_DEPENDENCY: &str = "transitive-dependency";
    /// All of them.
    pub(crate) const ALL: &[&str] = &[
        DEPENDENCY,
        DEV_DEPENDENCY,
        BUILD_DEPENDENCY,
        TRANSITIVE_DEPENDENCY,
    ];
}

/// The property of a dependency that has the version of its package, such as `1.4.0`.
pub const PROPERTY_VERSION: &str = "version";
/// The property of a dependency that has the licence of its package, as its manifest
/// writes it: `MIT OR Apache-2.0`. A package whose manifest names no licence has none.
pub const PROPERTY_LICENSE: &str = "license";
/// The property of a dependency that says where Cargo takes its package from, as Cargo
/// writes it: `registry+https://github.com/rust-lang/crates.io-index`, or `git+` and the
/// address of a repository. A package of a local path has none.
pub const PROPERTY_SOURCE: &str = "source";

/// The names of the properties of a dependency.
const PROPERTIES: [&str; 3] = [PROPERTY_VERSION, PROPERTY_LICENSE, PROPERTY_SOURCE];

/// The packages of a workspace as Cargo names them, and, when Cargo was asked for them, the
/// packages that a build can need with what each depends on.
#[derive(Debug)]
pub(crate) struct Graph {
    /// The directory of the workspace.
    root: PathBuf,
    packages: Vec<Package>,
}

#[derive(Debug)]
pub(crate) struct Package {
    /// The name of the package, as `[package]` writes it.
    pub name: String,
    /// What Cargo says of the package besides its name: its version, its licence and where
    /// it comes from, each when it has one.
    properties: Vec<Property>,
    /// The name of its library crate, as code writes it. A package without a library has
    /// the name of the package.
    pub krate: String,
    /// Whether the package is a member of the workspace.
    member: bool,
    /// Its manifest file.
    pub manifest: PathBuf,
    /// The root file of its library, or of its program when it has no library.
    pub root: Option<PathBuf>,
    /// The dependencies that the manifest gives another name: the name that the code
    /// writes, and the name of the package.
    renamed: Vec<(String, String)>,
    dependencies: Vec<Dependency>,
}

#[derive(Debug)]
struct Dependency {
    /// The name that the code of the package writes for it.
    name: String,
    /// The place of the package in the graph.
    package: usize,
    /// Each table that names it, with the platform of the table.
    uses: Vec<(Table, Option<String>)>,
}

impl Graph {
    /// Reads what `cargo metadata --format-version 1` writes.
    pub fn read(text: &str) -> Result<Graph, String> {
        let unreadable = || "`cargo metadata` wrote what cannot be read".to_owned();
        let json = Json::parse(text).ok_or_else(unreadable)?;
        let list = |json: &Json, key: &str| -> Vec<Json> {
            json.get(key).map(Json::list).unwrap_or_default().to_vec()
        };
        let text_of = |json: &Json, key: &str| -> Option<String> {
            json.get(key).and_then(Json::text).map(str::to_owned)
        };
        let members: HashSet<String> = list(&json, "workspace_members")
            .iter()
            .filter_map(|id| id.text().map(str::to_owned))
            .collect();

        let mut places: HashMap<String, usize> = HashMap::new();
        let mut packages = Vec::new();
        for package in list(&json, "packages") {
            let id = text_of(&package, "id").ok_or_else(unreadable)?;
            let name = text_of(&package, "name").ok_or_else(unreadable)?;
            let targets = list(&package, "targets");
            let is = |target: &Json, wanted: fn(&str) -> bool| {
                let kinds = list(target, "kind");
                let mut kinds = kinds.iter().filter_map(Json::text);
                kinds.any(wanted)
            };
            let library = targets
                .iter()
                .find(|target| is(target, |kind| kind.ends_with("lib") || kind == "proc-macro"));
            // Without a library, the program with the name of the package, or the first.
            let programs = || {
                targets
                    .iter()
                    .filter(|target| is(target, |kind| kind == "bin"))
            };
            let program = programs()
                .find(|target| text_of(target, "name").as_deref() == Some(name.as_str()))
                .or_else(|| programs().next());
            let krate = library.and_then(|target| text_of(target, "name"));
            let root = library
                .or(program)
                .and_then(|target| text_of(target, "src_path"));
            let manifest = text_of(&package, "manifest_path").ok_or_else(unreadable)?;
            let renamed = list(&package, "dependencies")
                .into_iter()
                .filter_map(|named| {
                    let written = text_of(&named, "rename")?.replace('-', "_");
                    Some((written, text_of(&named, "name")?))
                });
            let properties = PROPERTIES.iter().filter_map(|name| {
                let value = text_of(&package, name)?;
                Some(Property {
                    name: (*name).to_owned(),
                    value,
                })
            });
            packages.push(Package {
                properties: properties.collect(),
                renamed: renamed.collect(),
                krate: krate.unwrap_or_else(|| name.clone()).replace('-', "_"),
                name,
                member: members.contains(&id),
                manifest: PathBuf::from(manifest),
                root: root.map(PathBuf::from),
                dependencies: Vec::new(),
            });
            places.insert(id, packages.len() - 1);
        }

        // Without the dependencies (`--no-deps`), Cargo gives no nodes.
        let nodes = json.get("resolve").map(|resolve| list(resolve, "nodes"));
        for node in nodes.unwrap_or_default() {
            let place = text_of(&node, "id").and_then(|id| places.get(&id).copied());
            let place = place.ok_or_else(unreadable)?;
            for dependency in list(&node, "deps") {
                let name = text_of(&dependency, "name").unwrap_or_default();
                let package = text_of(&dependency, "pkg").and_then(|id| places.get(&id).copied());
                // A dependency without a library has no name in the code.
                let (Some(package), false) = (package, name.is_empty()) else {
                    continue;
                };
                let uses = list(&dependency, "dep_kinds");
                let uses = uses.iter().map(|used| {
                    let table = match used.get("kind").and_then(Json::text) {
                        Some("dev") => Table::Dev,
                        Some("build") => Table::Build,
                        _ => Table::Normal,
                    };
                    (table, text_of(used, "target"))
                });
                packages[place].dependencies.push(Dependency {
                    name,
                    package,
                    uses: uses.collect(),
                });
            }
        }
        let root = text_of(&json, "workspace_root").ok_or_else(unreadable)?;
        Ok(Graph {
            root: PathBuf::from(root),
            packages,
        })
    }

    /// The directory of the workspace.
    pub fn root(&self) -> &FilePath {
        &self.root
    }

    /// All packages, by their places in the graph.
    pub fn packages(&self) -> &[Package] {
        &self.packages
    }

    /// For each dependency that the manifest of a package gives another name: the name that
    /// the code writes, and the name of the crate. The crate of a package that is no member
    /// has the name of the package.
    pub fn renamed(&self, place: usize) -> Vec<(String, String)> {
        let crate_of = |package: &String| {
            let member = self.members().find(|(_, member)| member.name == *package);
            member.map_or_else(
                || package.replace('-', "_"),
                |(_, member)| member.krate.clone(),
            )
        };
        let renamed = self.packages[place].renamed.iter();
        renamed
            .map(|(written, package)| (written.clone(), crate_of(package)))
            .collect()
    }

    /// The members of the workspace, each with its place in the graph.
    pub fn members(&self) -> impl Iterator<Item = (usize, &Package)> {
        let packages = self.packages.iter().enumerate();
        packages.filter(|(_, package)| package.member)
    }
}

/// One way on which a member needs a package.
#[derive(Debug, Clone)]
struct Way {
    /// Only the tests need the package.
    dev: bool,
    /// The conditions of the platforms on the way. All of them must hold.
    conditions: Vec<String>,
    /// The line of the dependency of the manifest that the way starts with.
    line: u32,
    /// The crates on the way, without the package itself.
    through: Vec<String>,
    /// The manifest names the package for this way.
    named: bool,
}

impl Way {
    /// Whether this way is there whenever `other` is: a build that needs the package on the
    /// other way needs it on this way too.
    fn covers(&self, other: &Way) -> bool {
        let conditions = &mut self.conditions.iter();
        (!self.dev || other.dev) && conditions.all(|one| other.conditions.contains(one))
    }
}

/// The dependencies of a member as references from its root module `unit`, at the lines of
/// its manifest: the file `file` with the text `manifest`. `own` is the place of the member
/// in the graph.
///
/// A dependency that the manifest names has the kind of its table, and the name that the
/// code writes for it; a package with another name is an alias. A dependency for the tests
/// has the tag of test code, and one for a platform has the tags of its condition.
///
/// Each reference has the properties of its package: its version, its licence and where it
/// comes from.
///
/// A package that the manifest does not name, and that a dependency needs, is a reference
/// of the kind `transitive-dependency`, at the line of the dependency that needs it, with
/// the packages on the way in its note. It has the tag of test code when only the
/// dependencies for the tests need it, and the tags of the conditions of the way. A package
/// that is needed on two ways, of which neither is there whenever the other is, such as one
/// for Windows and one for Unix, is one reference for each way.
pub(crate) fn facts(
    graph: &Graph,
    own: usize,
    unit: &Path,
    file: &str,
    manifest: &str,
) -> Vec<Fact> {
    let packages = &graph.packages;
    let lines = manifest::dependency_lines(manifest);
    // The manifest names a dependency by the name that the code writes, or by the name of
    // its package when the library of the package has another name.
    let line = |table: Table, dependency: &Dependency| {
        let package = packages[dependency.package].name.replace('-', "_");
        let named =
            |(_, key, _): &&(Table, String, u32)| *key == dependency.name || *key == package;
        let exact = lines.iter().filter(named).find(|(at, ..)| *at == table);
        let any = exact.or_else(|| lines.iter().find(named));
        any.map_or(1, |(_, _, line)| *line)
    };
    let fact = |kind: &str, package: usize, target: &str, line: u32, tags: Vec<String>| Fact {
        unit: unit.clone(),
        file: file.to_owned(),
        line,
        column: 1,
        tags,
        note: None,
        item: None,
        what: What::Reference(Reference {
            target: vec![target.to_owned()],
            aliases: Vec::new(),
            kind: kind.to_owned(),
            certainty: Certainty::Exact,
            properties: packages[package].properties.clone(),
        }),
    };
    let test = || kinds::TAG_TEST.to_owned();
    let conditions = |target: &Option<String>| -> Vec<String> {
        target
            .as_deref()
            .map(cfg::tags_of_target)
            .unwrap_or_default()
    };

    // The ways on which each package is needed, each with its number in the order in which
    // the search found them. No way of a package covers another.
    let mut ways: HashMap<usize, Vec<(usize, Way)>> = HashMap::new();
    let mut count = 0;
    let mut queue: VecDeque<(usize, Way)> = VecDeque::new();
    let mut offer = |package: usize, way: Way, queue: &mut VecDeque<(usize, Way)>| {
        let known = ways.entry(package).or_default();
        if known.iter().any(|(_, other)| other.covers(&way)) {
            return;
        }
        known.retain(|(_, other)| !way.covers(other));
        known.push((count, way.clone()));
        count += 1;
        queue.push_back((package, way));
    };

    let mut made = Vec::new();
    for dependency in &packages[own].dependencies {
        let package = &packages[dependency.package];
        for (table, target) in &dependency.uses {
            let (kind, mut tags) = match table {
                Table::Normal => (kind::DEPENDENCY, Vec::new()),
                Table::Dev => (kind::DEV_DEPENDENCY, vec![test()]),
                Table::Build => (kind::BUILD_DEPENDENCY, Vec::new()),
            };
            let conditions = conditions(target);
            tags.extend(conditions.iter().cloned());
            let line = line(*table, dependency);
            let mut fact = fact(kind, dependency.package, &dependency.name, line, tags);
            if package.krate != dependency.name {
                if let What::Reference(reference) = &mut fact.what {
                    reference.aliases.push(vec![package.krate.clone()]);
                }
                fact.note = Some(format!("the package `{}`", package.name));
            }
            made.push(fact);
            let way = Way {
                dev: *table == Table::Dev,
                conditions,
                line,
                through: Vec::new(),
                named: true,
            };
            offer(dependency.package, way, &mut queue);
        }
    }

    // What the dependencies need, on the shortest way first. The dependencies that another
    // package has for its own tests are not needed.
    while let Some((at, way)) = queue.pop_front() {
        let mut through = way.through.clone();
        through.push(packages[at].krate.clone());
        for dependency in &packages[at].dependencies {
            if dependency.package == own {
                continue;
            }
            for (table, target) in &dependency.uses {
                if *table == Table::Dev {
                    continue;
                }
                let mut all = way.conditions.clone();
                for condition in conditions(target) {
                    if !all.contains(&condition) {
                        all.push(condition);
                    }
                }
                let next = Way {
                    dev: way.dev,
                    conditions: all,
                    line: way.line,
                    through: through.clone(),
                    named: false,
                };
                offer(dependency.package, next, &mut queue);
            }
        }
    }
    // A way that the manifest names is a dependency above, and no transitive one.
    let mut found: Vec<(usize, usize, &Way)> = Vec::new();
    for (package, known) in &ways {
        let transitive = known.iter().filter(|(_, way)| !way.named);
        found.extend(transitive.map(|(number, way)| (*number, *package, way)));
    }
    found.sort_by_key(|(number, ..)| *number);
    for (_, package, way) in found {
        let mut tags = if way.dev { vec![test()] } else { Vec::new() };
        tags.extend(way.conditions.iter().cloned());
        let krate = &packages[package].krate;
        let mut fact = fact(kind::TRANSITIVE_DEPENDENCY, package, krate, way.line, tags);
        let through = way.through.iter().map(|name| format!("`{name}`"));
        let through: Vec<String> = through.collect();
        fact.note = Some(format!("through {}", through.join(", ")));
        made.push(fact);
    }
    made
}

/// Adds the facts of the dependencies, their kinds and the names of their properties to a
/// fact set.
pub(crate) fn add(facts: &mut FactSet, found: Vec<Fact>) {
    for kind in kind::ALL {
        if !facts.kinds.iter().any(|known| known == kind) {
            facts.kinds.push((*kind).to_owned());
        }
    }
    facts.properties = PROPERTIES.iter().map(|name| (*name).to_owned()).collect();
    facts.facts.extend(found);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// What Cargo says of a package is on each reference to it, and what Cargo does not say
    /// is not: a package of a local path has no source, and one without a licence has none.
    #[test]
    fn a_dependency_has_what_cargo_says_of_its_package() {
        let package = |name: &str, more: &str| {
            format!(
                r#"{{"id": "{name}", "name": "{name}", "version": "1.4.0", "targets": [],
                "manifest_path": "/w/{name}/Cargo.toml", "dependencies": [], {more}}}"#
            )
        };
        let registry = "registry+https://github.com/rust-lang/crates.io-index";
        let packages = [
            package("app", r#""license": null, "source": null"#),
            package(
                "serde",
                &format!(r#""license": "MIT OR Apache-2.0", "source": "{registry}""#),
            ),
            package("local", r#""license": null, "source": null"#),
        ];
        let dependency = |name: &str| {
            format!(
                r#"{{"name": "{name}", "pkg": "{name}", "dep_kinds": [{{"kind": null, "target": null}}]}}"#
            )
        };
        let text = format!(
            r#"{{"packages": [{}], "workspace_members": ["app"], "workspace_root": "/w",
            "resolve": {{"nodes": [{{"id": "app", "deps": [{}, {}]}}]}}}}"#,
            packages.join(", "),
            dependency("serde"),
            dependency("local")
        );
        let graph = Graph::read(&text).unwrap();
        let unit = vec!["app".to_owned()];
        let found = facts(&graph, 0, &unit, "Cargo.toml", "");
        let found: Vec<Vec<(&str, &str)>> = found
            .iter()
            .map(|fact| match &fact.what {
                What::Reference(reference) => {
                    let pairs = reference.properties.iter();
                    pairs
                        .map(|one| (one.name.as_str(), one.value.as_str()))
                        .collect()
                }
                What::Marker(_) => Vec::new(),
            })
            .collect();
        assert_eq!(
            found,
            [
                vec![
                    (PROPERTY_VERSION, "1.4.0"),
                    (PROPERTY_LICENSE, "MIT OR Apache-2.0"),
                    (PROPERTY_SOURCE, registry)
                ],
                vec![(PROPERTY_VERSION, "1.4.0")],
            ]
        );
    }
}
