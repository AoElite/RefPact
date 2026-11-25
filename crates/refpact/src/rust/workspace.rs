//! Several crates of one Cargo workspace as one set of facts.
//!
//! In a workspace, a module is named by its crate: `shop_domain::order`, not `crate::order`.
//! So a rule can say that one crate must not use another, or a module of another.

use std::collections::{BTreeMap, HashMap, HashSet};
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
use crate::rust::workers;
use crate::rust::{analyse_tree, cfg, check_unreached, read_index, read_packages, resolve, Crate};

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
    /// The root files of the other programs of its package. Each is a crate of its own for
    /// the compiler; the load reads them as more code of this crate.
    programs: Vec<String>,
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
            programs: Vec::new(),
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
        let below = |root: &PathBuf| {
            let manifest_dir = package.manifest.parent();
            let below = manifest_dir.and_then(|dir| root.strip_prefix(dir).ok());
            below.map(slashes).ok_or_else(|| {
                let message = "the root file of the crate is outside the directory of the package";
                wrong(file.clone(), message.to_owned())
            })
        };
        // A load that names its root reads that crate only.
        let (roots, programs) = match (root, &package.root) {
            (Some(root), _) => (vec![root], Vec::new()),
            (None, Some(root)) => {
                let programs = package.programs.iter().map(below);
                (vec![below(root)?], programs.collect::<Result<_, _>>()?)
            }
            (None, None) => (usual_roots(), Vec::new()),
        };
        Ok(Spec {
            member: Member {
                name: name.to_owned(),
                dir: dir.to_owned(),
            },
            roots,
            programs,
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
    // What each member needs for its analysis, or why it has none. The errors keep the order
    // of the members.
    let mut planned: Vec<(Spec, Planned)> = Vec::new();
    for spec in specs {
        let Member { name, dir } = &spec.member;
        let manifest_file = sources::join(dir, "Cargo.toml");
        if planned.iter().any(|(other, _)| other.member.name == *name) {
            let error = LoadError::Manifest {
                file: manifest_file,
                message: format!("a second crate with the name `{name}`"),
            };
            planned.push((spec, Err(error)));
            break;
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
        let root = spec
            .roots
            .iter()
            .find(|path| member_disk.read(path).is_some())
            .cloned()
            .ok_or_else(|| LoadError::RootNotFound {
                tried: spec.roots.clone(),
            });
        let plan = root.map(|root| {
            let roots = std::iter::once(root).chain(spec.programs.iter().cloned());
            (member_disk, roots.collect(), allowed)
        });
        planned.push((spec, plan));
    }

    // Each root of each member, by the place of the member and that of the root.
    let roots: Vec<(usize, usize)> = planned
        .iter()
        .enumerate()
        .flat_map(|(place, (_, plan))| {
            let count = plan.as_ref().map_or(0, |(_, roots, _)| roots.len());
            (0..count).map(move |root| (place, root))
        })
        .collect();
    // The crates are analysed at the same time. Each analysis has the threads of its own.
    let analyse = |job: usize| {
        let (place, root) = roots[job];
        let (_, plan) = &planned[place];
        let (member_disk, roots, _) = plan.as_ref().map_err(Clone::clone)?;
        analyse_tree(member_disk, &roots[root], calls, &common.settings)
    };
    let mut analysed: Vec<Option<Result<Analysed, LoadError>>> =
        roots.iter().map(|_| None).collect();
    workers::with_pool(
        || |job: usize| (job, analyse(job)),
        |pool| {
            let mut waiting = 0;
            for job in 0..roots.len() {
                match pool.give(job) {
                    Ok(()) => waiting += 1,
                    Err(job) => analysed[job] = Some(analyse(job)),
                }
            }
            for _ in 0..waiting {
                let (job, found) = pool.reply();
                analysed[job] = Some(found);
            }
        },
    );

    let mut analysed = analysed.into_iter();
    for (spec, plan) in planned {
        let Member { name, dir } = &spec.member;
        // An error of the one crate of a load names no member.
        let in_member = |error: LoadError| match single {
            true => error,
            false => LoadError::Member {
                dir: dir.clone(),
                error: Box::new(error),
            },
        };
        let (member_disk, roots, allowed) = plan.map_err(in_member)?;
        let mut found: Vec<Analysed> = Vec::new();
        for reply in analysed.by_ref().take(roots.len()) {
            let reply = reply.unwrap_or_else(|| unreachable!("each root has one reply"));
            found.push(reply.map_err(in_member)?);
        }
        let trees: Vec<&Tree> = found.iter().map(|(tree, ..)| tree).collect();
        check_unreached(&member_disk, &roots[0], &trees, &allowed).map_err(in_member)?;

        let package = graph.as_ref().zip(spec.package);
        let first = loaded.len();
        for (tree, mut facts, mut impls, mut pending) in found {
            if loaded.len() > first {
                leave_shared(&loaded[first..], &tree, &mut facts, &mut pending);
                if let Some((graph, place)) = package {
                    let library = &graph.packages()[place].krate;
                    name_library(&mut facts, &mut impls, library);
                }
            }
            if let Some((graph, place)) = package {
                add_crates_of_renamed(&mut facts, &graph.renamed(place));
            }
            loaded.push((spec.member.clone(), tree, facts, impls, pending));
        }
        if let Some((graph, place)) = package {
            if common.dependencies {
                let manifest_file = sources::join(dir, "Cargo.toml");
                let text = disk.read(&manifest_file).unwrap_or_default();
                let unit = vec![name.clone()];
                dependencies.extend(packages::facts(graph, place, &unit, &manifest_file, &text));
            }
            packages.insert(graph.packages()[place].name.clone(), name.clone());
        }
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

/// A program names the library of its package as other crates do: `shop::Order`. The load
/// reads both as one crate, so the path is one of the crate itself.
fn name_library(facts: &mut FactSet, impls: &mut [Impl], library: &str) {
    let own = |path: &mut Path| {
        if path.first().map(String::as_str) == Some(library) {
            path[0] = "crate".to_owned();
        }
    };
    for fact in &mut facts.facts {
        if let What::Reference(reference) = &mut fact.what {
            own(&mut reference.target);
            reference.aliases.iter_mut().for_each(own);
        }
    }
    let relations = facts
        .declarations
        .iter_mut()
        .flat_map(|item| &mut item.relations);
    relations.for_each(|tie| own(&mut tie.target));
    for found in impls {
        found
            .types
            .iter_mut()
            .chain(&mut found.traits)
            .for_each(own);
    }
}

/// Makes a program of a package more code of the crate that `earlier` has: the crates of
/// the package that are read already.
///
/// - A module that both have is one module, as the roots of the two are: `crate`.
/// - A file that both declare as a module, such as `mod config;` in `lib.rs` and in
///   `main.rs`, is read as code of the first of them only. Otherwise each of its references
///   would be there twice.
fn leave_shared(earlier: &[Loaded], tree: &Tree, facts: &mut FactSet, pending: &mut Pending) {
    let known: HashSet<&Path> = earlier
        .iter()
        .flat_map(|(_, _, facts, ..)| facts.units.iter().chain(&facts.items))
        .collect();
    let mut kept = facts.units.iter().map(|unit| !known.contains(unit));
    facts.unit_lines.retain(|_| kept.next().unwrap_or(true));
    let mut kept = facts.units.iter().map(|unit| !known.contains(unit));
    facts.unit_files.retain(|_| kept.next().unwrap_or(true));
    facts.units.retain(|unit| !known.contains(unit));
    facts.items.retain(|item| !known.contains(item));

    let read: HashSet<&str> = earlier
        .iter()
        .flat_map(|(_, tree, ..)| &tree.files)
        .map(|file| file.path.as_str())
        .collect();
    let files = tree.files.iter().map(|file| file.path.as_str());
    let shared: HashSet<&str> = files.filter(|file| read.contains(file)).collect();
    if shared.is_empty() {
        return;
    }
    let is_shared = |file: &String| shared.contains(file.as_str());
    facts.declarations.retain(|item| !is_shared(&item.file));

    // A site belongs before a fact, by the position of the fact: the positions change.
    let mut left = 0;
    let mut moved: Vec<usize> = Vec::with_capacity(facts.facts.len() + 1);
    for fact in &facts.facts {
        moved.push(left);
        left += usize::from(!is_shared(&fact.file));
    }
    moved.push(left);
    pending.sites.retain(|(_, site)| !is_shared(&site.file));
    for (position, _) in &mut pending.sites {
        *position = moved[*position];
    }
    facts.facts.retain(|fact| !is_shared(&fact.file));
    pending
        .impl_fns
        .retain(|function| !is_shared(&function.file));
    pending.ends.retain(|((file, ..), _)| !is_shared(file));
    pending.macros.retain(|site| !is_shared(&site.file));
}

/// One analysed crate: its module tree, its facts and its `impl` blocks. A package with
/// programs is several, with the same member.
type Loaded = (Member, Tree, FactSet, Vec<Impl>, Pending);

/// What the analysis of one member needs: its directory, its root with the roots of its
/// programs, and the files that it may leave unreached. An error: the member has no
/// analysis.
type Planned = Result<(Disk, Vec<String>, Vec<String>), LoadError>;

/// What the analysis of one crate gives.
type Analysed = (Tree, FactSet, Vec<Impl>, Pending);

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
    // The facts of each member are moved into the merged set, not copied.
    let fact_sets: Vec<FactSet> = loaded
        .iter_mut()
        .map(|(_, _, facts, ..)| std::mem::take(facts))
        .collect();
    let macros: Vec<Vec<MacroSite>> = found.iter().map(|found| found.macros.clone()).collect();
    // A name is that of the first crate that has it: the library of a package, before its
    // programs.
    let names: BTreeMap<&str, usize> = loaded
        .iter()
        .enumerate()
        .rev()
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

    let members = loaded.iter().enumerate().zip(fact_sets);
    for (((index, (member, ..)), facts), found) in members.zip(found) {
        let in_place = |path: &mut Path| rename(path, &member.name);
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
        let renamed_all = |paths: Vec<Path>| {
            paths.into_iter().map(|mut path| {
                in_place(&mut path);
                path
            })
        };
        merged.units.extend(renamed_all(facts.units));
        merged.unit_lines.extend(&facts.unit_lines);
        let files = facts.unit_files.iter();
        merged
            .unit_files
            .extend(files.map(|file| sources::join(&member.dir, file)));
        merged.items.extend(renamed_all(facts.items));
        for tag in facts.tags {
            if !merged.tags.contains(&tag) {
                merged.tags.push(tag);
            }
        }
        merged.kinds = facts.kinds;
        merged.markers = facts.markers;
        merged.item_kinds = facts.item_kinds;
        merged.member_kinds = facts.member_kinds;
        merged.visibilities = facts.visibilities;
        merged.relations = facts.relations;
        merged.modifiers = facts.modifiers;
        for mut declaration in facts.declarations {
            in_place(&mut declaration.path);
            in_place(&mut declaration.unit);
            declaration.file = sources::join(&member.dir, &declaration.file);
            for tie in &mut declaration.relations {
                in_place(&mut tie.target);
            }
            merged.declarations.push(declaration);
        }
        for mut fact in facts.facts {
            in_place(&mut fact.unit);
            if let Some(item) = &mut fact.item {
                in_place(item);
            }
            fact.file = sources::join(&member.dir, &fact.file);
            if let What::Reference(reference) = &mut fact.what {
                in_place(&mut reference.target);
                reference.aliases.iter_mut().for_each(in_place);
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
    let mut members: Vec<Member> = loaded.into_iter().map(|(member, ..)| member).collect();
    members.dedup();
    Workspace {
        code: Crate::new(merged),
        members,
    }
}
