//! Several crates of one Cargo workspace as one set of facts.
//!
//! In a workspace, a module is named by its crate: `shop_domain::order`, not `crate::order`.
//! So a rule can say that one crate must not use another, or a module of another.

use std::collections::{BTreeMap, HashMap};
use std::ops::Deref;
use std::path::{Path as FilePath, PathBuf};

use crate::core::{Fact, FactSet, Path, What};

use crate::rust::calls::{self, MacroSite, Pending};
use crate::rust::expand;
use crate::rust::items::{self, Impl};
use crate::rust::load::{LoadError, Tree};
use crate::rust::options::Common;
use crate::rust::packages::{self, Graph};
use crate::rust::scip::Index;
use crate::rust::scope::Resolver;
use crate::rust::sources::Expansions;
use crate::rust::sources::{self, Disk, Sources};
use crate::rust::{analyse_tree, cfg, read_index, read_packages, resolve, Crate};

/// The crates of one workspace, analysed together.
///
/// It can be used wherever a [`Crate`] can: rules are checked against it in the same way.
#[derive(Debug, Clone)]
pub struct Workspace {
    code: Crate,
    members: Vec<Member>,
}

/// One crate of a workspace.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    /// The name of the crate as code writes it, such as `shop_domain`.
    pub name: String,
    /// The directory of the crate, relative to the workspace. Empty for the root package.
    pub dir: String,
}

impl Deref for Workspace {
    type Target = Crate;

    fn deref(&self) -> &Crate {
        &self.code
    }
}

impl Workspace {
    /// Analyses the workspace in `dir`. The members are those that Cargo names for it
    /// (`cargo metadata`): the packages of `members` without those of `exclude`, a root
    /// package, and each package of the directory that a member depends on by a path.
    ///
    /// The load reads the syntax only. Method calls and field accesses are references with
    /// [`WorkspaceOptions::resolving_members`].
    ///
    /// In a test of a member crate, pass a path from `env!("CARGO_MANIFEST_DIR")`.
    pub fn load(dir: impl Into<PathBuf>) -> Result<Workspace, LoadError> {
        Workspace::options(dir).load()
    }

    /// Starts a load with options.
    pub fn options(dir: impl Into<PathBuf>) -> WorkspaceOptions {
        WorkspaceOptions {
            dir: dir.into(),
            members: None,
            common: Common::default(),
        }
    }

    /// The crates, in the order of their directories.
    pub fn members(&self) -> &[Member] {
        &self.members
    }

    /// The code of the workspace as one crate.
    pub(crate) fn into_crate(self) -> Crate {
        self.code
    }
}

/// Options for [`Workspace::options`].
#[derive(Debug, Clone)]
pub struct WorkspaceOptions {
    dir: PathBuf,
    members: Option<Vec<String>>,
    common: Common,
}

impl WorkspaceOptions {
    /// Loads only the members in these directories, relative to the workspace. Each must
    /// be a package that Cargo names as a member of the workspace.
    pub fn members<I, S>(mut self, dirs: I) -> WorkspaceOptions
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.members = Some(dirs.into_iter().map(Into::into).collect());
        self
    }

    /// Analyses the workspace.
    pub fn load(self) -> Result<Workspace, LoadError> {
        let plan = Plan::Workspace {
            members: self.members,
        };
        load(self.dir, plan, self.common)
    }
}

option_methods!(WorkspaceOptions, "Workspace", "shop_domain");

/// What a load reads.
pub(crate) enum Plan {
    /// The crate of the directory, with `crate` as the first segment of its paths. `root`
    /// is its root file when the load names one.
    Crate { root: Option<String> },
    /// The members of the workspace of the directory, or those in the directories of
    /// `members`. The first segment of a path is the name of a crate.
    Workspace { members: Option<Vec<String>> },
}

/// One crate that a load reads.
struct Spec {
    /// The first segment of its paths, and its directory relative to the load.
    member: Member,
    /// The files that can be its root, relative to its directory. The first that is there
    /// is the root.
    roots: Vec<String>,
    /// The place of its package in what Cargo names. `None` for a crate without a manifest.
    package: Option<usize>,
}

/// The roots of a crate that no manifest describes.
fn usual_roots() -> Vec<String> {
    vec!["src/lib.rs".to_owned(), "src/main.rs".to_owned()]
}

/// A path with `/` between its parts.
fn slashes(path: &FilePath) -> String {
    let parts = path.components();
    let parts: Vec<_> = parts
        .map(|part| part.as_os_str().to_string_lossy())
        .collect();
    parts.join("/")
}

/// The crates of a plan. `graph` is what Cargo says about the directory; without it, the
/// directory is one crate without a manifest.
fn specs(disk: &Disk, plan: Plan, graph: Option<&Graph>) -> Result<Vec<Spec>, LoadError> {
    let wrong = |file: String, message: String| LoadError::Manifest { file, message };
    let top = || "Cargo.toml".to_owned();
    let Some(graph) = graph else {
        let root = match plan {
            Plan::Crate { root } => root,
            Plan::Workspace { .. } => None,
        };
        return Ok(vec![Spec {
            member: Member {
                name: "crate".to_owned(),
                dir: String::new(),
            },
            roots: root.map_or_else(usual_roots, |root| vec![root]),
            package: None,
        }]);
    };

    let real = sources::real;
    let base = real(&disk.base);
    // The members below the directory of the load, by their directories.
    let mut inside: Vec<(String, usize)> = Vec::new();
    let mut outside: Vec<&str> = Vec::new();
    for (place, package) in graph.members() {
        let dir = real(package.manifest.parent().unwrap_or(FilePath::new("")));
        match dir.strip_prefix(&base) {
            Ok(rest) => inside.push((slashes(rest), place)),
            Err(_) => outside.push(&package.name),
        }
    }
    inside.sort();
    let spec = |name: &str, dir: &str, place: usize, root: Option<String>| {
        let package = &graph.packages()[place];
        let file = sources::join(dir, "Cargo.toml");
        let roots = match (root, &package.root) {
            (Some(root), _) => vec![root],
            (None, Some(root)) => {
                let below = package
                    .manifest
                    .parent()
                    .and_then(|dir| root.strip_prefix(dir).ok());
                let below = below.ok_or_else(|| {
                    let message =
                        "the root file of the crate is outside the directory of the package";
                    wrong(file.clone(), message.to_owned())
                })?;
                vec![slashes(below)]
            }
            (None, None) => usual_roots(),
        };
        Ok(Spec {
            member: Member {
                name: name.to_owned(),
                dir: dir.to_owned(),
            },
            roots,
            package: Some(place),
        })
    };
    match plan {
        Plan::Crate { root } => {
            let own = inside.iter().find(|(dir, _)| dir.is_empty());
            let (_, place) = own.ok_or_else(|| {
                let message = "Cargo names no package in this directory. Is it a workspace? Then load it with `Workspace::load`";
                wrong(top(), message.to_owned())
            })?;
            Ok(vec![spec("crate", "", *place, root)?])
        }
        Plan::Workspace { members } => {
            let root = real(graph.root());
            if root != base {
                let message = format!(
                    "the directory is not the directory of its workspace, which is {}",
                    root.display()
                );
                return Err(wrong(top(), message));
            }
            let named = |place: usize| graph.packages()[place].krate.as_str();
            match members {
                None => {
                    if let Some(name) = outside.first() {
                        let message = format!(
                            "the member `{name}` is outside the directory of the workspace, so its files have no path in it"
                        );
                        return Err(wrong(top(), message));
                    }
                    let all = inside.iter();
                    all.map(|(dir, place)| spec(named(*place), dir, *place, None))
                        .collect()
                }
                Some(dirs) => {
                    let one = |dir: &String| {
                        let dir = sources::join("", dir);
                        let found = inside.iter().find(|(known, _)| *known == dir);
                        let (_, place) = found.ok_or_else(|| {
                            let message =
                                "Cargo names no member of the workspace in this directory";
                            wrong(sources::join(&dir, "Cargo.toml"), message.to_owned())
                        })?;
                        spec(named(*place), &dir, *place, None)
                    };
                    dirs.iter().map(one).collect()
                }
            }
        }
    }
}

/// Reads the crates of a plan in a directory and makes one set of facts of them.
pub(crate) fn load(dir: PathBuf, plan: Plan, mut common: Common) -> Result<Workspace, LoadError> {
    resolve(&mut common.settings)?;
    let disk = Disk { base: dir.clone() };
    let single = matches!(plan, Plan::Crate { .. });
    // A directory without a manifest is one crate. Cargo has nothing to say about it.
    let without_manifest = single && disk.read("Cargo.toml").is_none() && !common.dependencies;
    let graph = match without_manifest {
        true => None,
        false => Some(read_packages(&disk, &common)?),
    };
    let specs = specs(&disk, plan, graph.as_ref())?;
    if specs.is_empty() {
        return Err(LoadError::Manifest {
            file: "Cargo.toml".to_owned(),
            message: "the workspace has no members".to_owned(),
        });
    }

    // The implementations and the expansions are found with the index too.
    let calls = common.members || common.implementations || common.expansions;
    let mut dependencies: Vec<Fact> = Vec::new();
    let mut loaded: Vec<Loaded> = Vec::new();
    // The first segment of the paths of each package, by the name of the package.
    let mut packages: HashMap<String, String> = HashMap::new();
    let mut allowed_used = vec![false; common.allowed.len()];
    for spec in specs {
        let Member { name, dir } = &spec.member;
        let manifest_file = sources::join(dir, "Cargo.toml");
        if loaded.iter().any(|(member, ..)| member.name == *name) {
            return Err(LoadError::Manifest {
                file: manifest_file,
                message: format!("a second crate with the name `{name}`"),
            });
        }

        // Allowances are relative to the load; the crate wants them relative to itself.
        let mut allowed = Vec::new();
        for (index, entry) in common.allowed.iter().enumerate() {
            let entry = sources::join("", entry);
            let inside = if dir.is_empty() {
                Some(entry.as_str())
            } else {
                entry
                    .strip_prefix(dir.as_str())
                    .and_then(|rest| rest.strip_prefix('/'))
            };
            if let Some(inside) = inside {
                allowed.push(inside.to_owned());
                allowed_used[index] = true;
            }
        }

        let member_disk = Disk {
            base: disk.base.join(dir),
        };
        // An error of the one crate of a load names no member.
        let in_member = |error: LoadError| match single {
            true => error,
            false => LoadError::Member {
                dir: dir.clone(),
                error: Box::new(error),
            },
        };
        let root = spec
            .roots
            .iter()
            .find(|path| member_disk.read(path).is_some())
            .ok_or_else(|| {
                in_member(LoadError::RootNotFound {
                    tried: spec.roots.clone(),
                })
            })?;
        let (tree, mut facts, impls, pending) =
            analyse_tree(&member_disk, root, &allowed, calls, &common.settings)
                .map_err(in_member)?;
        if let (Some(graph), Some(place)) = (&graph, spec.package) {
            add_crates_of_renamed(&mut facts, &graph.renamed(place));
            if common.dependencies {
                let text = disk.read(&manifest_file).unwrap_or_default();
                let unit = vec![name.clone()];
                dependencies.extend(packages::facts(graph, place, &unit, &manifest_file, &text));
            }
            packages.insert(graph.packages()[place].name.clone(), name.clone());
        }
        loaded.push((spec.member, tree, facts, impls, pending));
    }
    let unused: Vec<String> = common
        .allowed
        .iter()
        .zip(&allowed_used)
        .filter(|(_, used)| !**used)
        .map(|(entry, _)| entry.clone())
        .collect();
    if !unused.is_empty() {
        return Err(LoadError::StaleAllowance { prefixes: unused });
    }

    let index = match calls {
        true => {
            let mut files = Vec::new();
            for (member, tree, ..) in &loaded {
                files.push(sources::join(&member.dir, "Cargo.toml"));
                let sources = tree.files.iter();
                files.extend(sources.map(|file| sources::join(&member.dir, &file.path)));
            }
            // The macros of each member, at their places in the directory of the load.
            let mut expansions = Expansions::new();
            if common.expansions {
                let places: Vec<_> = loaded
                    .iter()
                    .flat_map(|(member, .., pending)| {
                        let places = pending.macros.iter().map(MacroSite::place);
                        places.map(|(file, line, column)| {
                            (sources::join(&member.dir, &file), line, column)
                        })
                    })
                    .collect();
                expansions = sources::expansions(&disk, &files, &places)
                    .map_err(|message| LoadError::Index { message })?;
            }
            Some((read_index(&disk, files)?, packages, expansions))
        }
        false => None,
    };
    let dependencies = common.dependencies.then_some(dependencies);
    Ok(merge(
        loaded,
        index,
        dependencies,
        &common.settings,
        common.implementations,
    ))
}

/// A path that starts with the name that a manifest gives a dependency
/// (`store = { package = "shop-storage" }`) is also a path of the crate of the package:
/// `store::Db` is a use of `shop_storage::Db`. `renamed` has each such name with the name
/// of the crate.
fn add_crates_of_renamed(facts: &mut FactSet, renamed: &[(String, String)]) {
    if renamed.is_empty() {
        return;
    }
    for fact in &mut facts.facts {
        let What::Reference(reference) = &mut fact.what else {
            continue;
        };
        let known: Vec<Path> = std::iter::once(&reference.target)
            .chain(&reference.aliases)
            .cloned()
            .collect();
        for mut path in known {
            let found = renamed
                .iter()
                .find(|(written, _)| path.first() == Some(written));
            let Some((_, krate)) = found else { continue };
            path[0].clone_from(krate);
            if path != reference.target && !reference.aliases.contains(&path) {
                reference.aliases.push(path);
            }
        }
    }
}

/// One analysed member: its module tree, its facts and its `impl` blocks.
type Loaded = (Member, Tree, FactSet, Vec<Impl>, Pending);

/// Makes one fact set of the crates. `crate` becomes the name of each crate, files get the
/// directory of their crate, and a path into another member gets the definitions behind its
/// re-exports as aliases. An `impl` in one member for a type of another is attached to the
/// type.
///
/// With an index and the crates of its packages, the method calls and the field accesses
/// that the index resolves are references too.
fn merge(
    mut loaded: Vec<Loaded>,
    index: Option<(Index, HashMap<String, String>, Expansions)>,
    dependencies: Option<Vec<Fact>>,
    settings: &cfg::Settings,
    implementations: bool,
) -> Workspace {
    let mut pending = Pending::default();
    let found: Vec<Pending> = loaded
        .iter_mut()
        .map(|(.., pending)| std::mem::take(pending))
        .collect();
    let macros: Vec<Vec<MacroSite>> = found.iter().map(|found| found.macros.clone()).collect();
    let names: BTreeMap<&str, usize> = loaded
        .iter()
        .enumerate()
        .map(|(index, (member, ..))| (member.name.as_str(), index))
        .collect();
    let rename = |path: &mut Path, name: &str| {
        if path.first().map(String::as_str) == Some("crate") {
            path[0] = name.to_owned();
        }
    };
    // The definitions behind a path that starts with the name of another member.
    let behind = |path: &Path, own: usize| -> Vec<Path> {
        let Some(&other) = path.first().and_then(|head| names.get(head.as_str())) else {
            return Vec::new();
        };
        if other == own {
            return Vec::new();
        }
        let mut inside = path.clone();
        inside[0] = "crate".to_owned();
        let mut aliases = Resolver {
            modules: &loaded[other].1.modules,
        }
        .aliases(&inside);
        for alias in &mut aliases {
            rename(alias, &loaded[other].0.name);
        }
        aliases
    };

    let mut merged = FactSet {
        separator: "::".to_owned(),
        ..FactSet::default()
    };
    // Each path of an `impl`, in the names of the workspace, with the definitions behind it.
    let mut impls: Vec<Impl> = Vec::new();
    let widen = |paths: &[Path], member: &str, index: usize| {
        let mut wide: Vec<Path> = Vec::new();
        for path in paths {
            let mut path = path.clone();
            rename(&mut path, member);
            for found in std::iter::once(path.clone()).chain(behind(&path, index)) {
                if !wide.contains(&found) {
                    wide.push(found);
                }
            }
        }
        wide
    };
    for (index, (member, _, _, own, _)) in loaded.iter().enumerate() {
        for found in own {
            impls.push(Impl {
                types: widen(&found.types, &member.name, index),
                traits: widen(&found.traits, &member.name, index),
            });
        }
    }

    for ((index, (member, _, facts, ..)), found) in loaded.iter().enumerate().zip(found) {
        let renamed = |path: &Path| {
            let mut path = path.clone();
            rename(&mut path, &member.name);
            path
        };
        // What the index resolves, in the names and the files of the workspace.
        let before = merged.facts.len();
        for (position, mut site) in found.sites {
            site.unit = renamed(&site.unit);
            site.item = site.item.as_ref().map(renamed);
            site.file = sources::join(&member.dir, &site.file);
            pending.sites.push((before + position, site));
        }
        for ((file, line, column), end) in found.ends {
            let place = (sources::join(&member.dir, &file), line, column);
            pending.ends.push((place, end));
        }
        for mut function in found.impl_fns {
            function.file = sources::join(&member.dir, &function.file);
            function.path = renamed(&function.path);
            function.traits = function.traits.iter().map(renamed).collect();
            pending.impl_fns.push(function);
        }
        merged.units.extend(facts.units.iter().map(renamed));
        merged.unit_lines.extend(&facts.unit_lines);
        merged.items.extend(facts.items.iter().map(renamed));
        merged.kinds.clone_from(&facts.kinds);
        merged.markers.clone_from(&facts.markers);
        for tag in &facts.tags {
            if !merged.tags.contains(tag) {
                merged.tags.push(tag.clone());
            }
        }
        merged.item_kinds.clone_from(&facts.item_kinds);
        merged.visibilities.clone_from(&facts.visibilities);
        merged.relations.clone_from(&facts.relations);
        merged.modifiers.clone_from(&facts.modifiers);
        for declaration in &facts.declarations {
            let mut declaration = declaration.clone();
            declaration.path = renamed(&declaration.path);
            declaration.unit = renamed(&declaration.unit);
            declaration.file = sources::join(&member.dir, &declaration.file);
            for tie in &mut declaration.relations {
                tie.target = renamed(&tie.target);
            }
            merged.declarations.push(declaration);
        }
        for fact in &facts.facts {
            let mut fact = fact.clone();
            fact.unit = renamed(&fact.unit);
            fact.item = fact.item.as_ref().map(renamed);
            fact.file = sources::join(&member.dir, &fact.file);
            if let What::Reference(reference) = &mut fact.what {
                reference.target = renamed(&reference.target);
                reference.aliases = reference.aliases.iter().map(renamed).collect();
                let known: Vec<Path> = std::iter::once(&reference.target)
                    .chain(&reference.aliases)
                    .cloned()
                    .collect();
                for path in &known {
                    for alias in behind(path, index) {
                        if alias != reference.target && !reference.aliases.contains(&alias) {
                            reference.aliases.push(alias);
                        }
                    }
                }
            }
            merged.facts.push(fact);
        }
    }
    items::attach_impls(&mut merged.declarations, &impls);
    if let Some((index, packages, expansions)) = &index {
        calls::add(&mut merged, pending, index, packages, implementations);
        for ((member, tree, ..), macros) in loaded.iter().zip(&macros) {
            let names = Some((member.name.as_str(), member.dir.as_str()));
            expand::add(&mut merged, &tree.modules, macros, expansions, names);
        }
    }
    if let Some(found) = dependencies {
        packages::add(&mut merged, found);
    }
    cfg::finish(&mut merged, settings);
    let members = loaded.into_iter().map(|(member, ..)| member).collect();
    Workspace {
        code: Crate::new(merged),
        members,
    }
}
