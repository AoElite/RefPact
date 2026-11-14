//! Rust front end of refpact.
//!
//! [`Crate::load`] reads one crate from source with `syn`, builds its module tree, resolves
//! the paths it can, and returns the facts that the core evaluates rules against.
//!
//! It works on stable Rust and does not run the compiler. `docs/rust.md` lists what it
//! cannot see.

mod calls;
mod cfg;
mod expand;
mod extract;
pub(crate) mod items;
mod load;
mod macros;
mod manifest;
#[macro_use]
mod options;
mod packages;
mod scip;
mod scope;
mod sources;
mod tables;
mod workers;
mod workspace;

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use crate::core::{FactSet, Path, Prepared};

pub use load::LoadError;
pub use macros::{EXPRESSION_LIST_MACROS, OTHER_PARSED_MACROS};
pub use packages::{PROPERTY_LICENSE, PROPERTY_SOURCE, PROPERTY_VERSION};
pub use workspace::{Member, Workspace, WorkspaceOptions};

use options::Common;
use scope::Name;
use sources::Sources;

/// The reference kinds this front end produces.
pub mod kind {
    pub use crate::rust::extract::kind::*;
    pub use crate::rust::packages::kind::{
        BUILD_DEPENDENCY, DEPENDENCY, DEV_DEPENDENCY, TRANSITIVE_DEPENDENCY,
    };
}

/// The markers this front end produces.
pub mod marker {
    pub use crate::rust::extract::marker::*;
}

/// The tag of facts inside `#[cfg(test)]` code and test functions.
pub const TAG_TEST: &str = extract::TAG_TEST;

/// The tag of what the expansion of a macro holds. Only a load with
/// [`Options::expanding_macros`] gives facts with it.
pub const TAG_GENERATED: &str = expand::TAG;

/// The tag of facts under a `cfg` condition, such as `cfg(windows)` for `"windows"`. Each
/// condition that a `#[cfg(..)]` requires is a tag of the code inside.
pub fn cfg_tag(condition: &str) -> String {
    cfg::tag_of(condition)
}

/// One analysed crate.
#[derive(Debug, Clone)]
pub struct Crate {
    /// The facts with the lookups that every rule needs, made once for all rules.
    prepared: Prepared,
}

impl Crate {
    /// Analyses the crate in `manifest_dir`. The root is the root file of the library that
    /// Cargo names for the package, or of its program when it has no library: `src/lib.rs`
    /// or `src/main.rs` unless the manifest says otherwise. A directory without a
    /// `Cargo.toml` is read as a crate with one of these two roots.
    ///
    /// The load reads the syntax only. Method calls and field accesses are references with
    /// [`Options::resolving_members`].
    ///
    /// In a test, pass `env!("CARGO_MANIFEST_DIR")`.
    pub fn load(manifest_dir: impl Into<PathBuf>) -> Result<Crate, LoadError> {
        Crate::options(manifest_dir).load()
    }

    /// Starts a load with options.
    pub fn options(manifest_dir: impl Into<PathBuf>) -> Options {
        Options {
            dir: manifest_dir.into(),
            root: None,
            common: Common::default(),
        }
    }

    /// Analyses source text given in memory. `files` are pairs of path and text; `root` is
    /// the path of the crate root among them.
    ///
    /// rust-analyzer needs the files on disk, so [`Options::resolving_members`] has no
    /// counterpart here: method calls and field accesses are not references.
    pub fn from_memory(root: &str, files: &[(&str, &str)]) -> Result<Crate, LoadError> {
        let files: BTreeMap<String, String> = files
            .iter()
            .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
            .collect();
        let settings = cfg::Settings::default();
        let (_, mut facts, ..) =
            analyse_tree(&sources::Memory { files }, root, &[], false, &settings)?;
        cfg::finish(&mut facts, &settings);
        Ok(Crate::new(facts))
    }

    pub(crate) fn new(facts: FactSet) -> Crate {
        Crate {
            prepared: Prepared::new(facts),
        }
    }

    /// The facts of the crate, for the core.
    pub fn facts(&self) -> &FactSet {
        self.prepared.facts()
    }

    /// The facts with the lookups that every rule needs.
    pub(crate) fn prepared(&self) -> &Prepared {
        &self.prepared
    }

    /// The paths of all modules, such as `crate::frontend::passes`.
    pub fn modules(&self) -> impl Iterator<Item = String> + '_ {
        self.facts().units.iter().map(|unit| unit.join("::"))
    }
}

/// Options for [`Crate::options`].
#[derive(Debug, Clone)]
pub struct Options {
    dir: PathBuf,
    root: Option<String>,
    common: Common,
}

impl Options {
    /// Sets the crate root, relative to the manifest directory, for example `src/main.rs`.
    pub fn root(mut self, path: impl Into<String>) -> Options {
        self.root = Some(path.into());
        self
    }

    /// Analyses the crate.
    pub fn load(self) -> Result<Crate, LoadError> {
        let plan = workspace::Plan::Crate { root: self.root };
        workspace::load(self.dir, plan, self.common).map(Workspace::into_crate)
    }
}

option_methods!(Options, "Crate", "crate");

/// The packages of the workspace of a directory, as Cargo names them: with what each
/// depends on when the load reads the dependencies, and without it otherwise, which needs
/// no lock file and no network.
fn read_packages(disk: &sources::Disk, common: &Common) -> Result<packages::Graph, LoadError> {
    let features = common.settings.features.as_ref();
    let features = features.map(|list| {
        list.iter()
            .map(String::as_str)
            .collect::<Vec<_>>()
            .join(",")
    });
    let mut arguments: Vec<&str> = Vec::new();
    match (common.dependencies, &features) {
        (false, _) => arguments.push("--no-deps"),
        (true, None) => arguments.push("--all-features"),
        (true, Some(list)) => {
            arguments.push("--no-default-features");
            if !list.is_empty() {
                arguments.extend(["--features", list]);
            }
        }
    }
    let graph = sources::metadata(disk, &arguments).and_then(|text| packages::Graph::read(&text));
    graph.map_err(|message| LoadError::Manifest {
        file: "Cargo.toml".to_owned(),
        message,
    })
}

/// Asks the compiler for the options of the target of the settings.
fn resolve(settings: &mut cfg::Settings) -> Result<(), LoadError> {
    settings
        .resolve(sources::target_options)
        .map_err(|message| LoadError::Target { message })
}

/// The index of rust-analyzer for a directory. `files` are the source files of the code,
/// relative to the directory: the index is made again when one of them or a manifest
/// changes, and is read from where it is kept otherwise.
fn read_index(disk: &sources::Disk, mut files: Vec<String>) -> Result<scip::Index, LoadError> {
    files.extend(["Cargo.toml", "Cargo.lock"].map(str::to_owned));
    files.sort();
    files.dedup();
    let index = disk
        .index(&files)
        .and_then(|bytes| scip::Index::read(&bytes));
    index.map_err(|message| LoadError::Index { message })
}

/// The module tree and the facts of one crate, with `crate` as the first segment of paths,
/// and the `impl` blocks of the crate. They are already attached to the declarations of the
/// crate; a workspace attaches them to the declarations of the other members.
fn analyse_tree(
    sources: &dyn Sources,
    root: &str,
    allowed: &[String],
    calls: bool,
    settings: &cfg::Settings,
) -> Result<(load::Tree, FactSet, Vec<items::Impl>, calls::Pending), LoadError> {
    let (tree, mut found) = load::load(sources, root, calls)?;
    items::attach_impls(&mut found.declarations, &found.impls);
    items::attach_exports(&mut found.declarations, &tree.modules);
    check_unreached(sources, root, &tree, allowed)?;

    let mut units: Vec<Path> = Vec::new();
    let mut unit_lines: Vec<u32> = Vec::new();
    let mut items: Vec<Path> = Vec::new();
    // The conditions of the modules are tags too, also those of a module without facts.
    let mut tags = vec![TAG_TEST.to_owned(), TAG_GENERATED.to_owned()];
    for module in &tree.modules {
        let conditions = module.cfg.as_deref().unwrap_or_default();
        // A module that the build of the settings leaves out is no unit.
        if settings.leaves_out(conditions) {
            continue;
        }
        for condition in conditions {
            if !tags.contains(condition) {
                tags.push(condition.clone());
            }
        }
        units.push(module.path.clone());
        unit_lines.push(module.lines);
        for (name, kind) in &module.names.items {
            let mut path = module.path.clone();
            path.push(name.clone());
            match kind {
                Name::Item => items.push(path),
                Name::Enum(variants) => {
                    for variant in variants {
                        let mut variant_path = path.clone();
                        variant_path.push(variant.clone());
                        items.push(variant_path);
                    }
                    items.push(path);
                }
                Name::Module(_) | Name::ExternCrate(_) => {}
            }
        }
    }

    let to_strings = |values: &[&str]| values.iter().map(|value| (*value).to_owned()).collect();
    let facts = FactSet {
        separator: "::".to_owned(),
        units,
        unit_lines,
        items,
        kinds: to_strings(kind::ALL),
        markers: to_strings(marker::ALL),
        tags,
        facts: found.facts,
        declarations: found.declarations,
        item_kinds: to_strings(items::item_kind::ALL),
        visibilities: to_strings(items::visibility::ALL),
        relations: to_strings(items::relation::ALL),
        modifiers: to_strings(items::modifier::ALL),
        properties: Vec::new(),
        attached_prefixes: Vec::new(),
        hidden_prefixes: Vec::new(),
    };
    Ok((tree, facts, found.impls, found.pending))
}

/// A file that no `mod` declaration reaches is not checked by any rule. That is an error
/// unless it is another crate root of the package or is allowed by name.
fn check_unreached(
    sources: &dyn Sources,
    root: &str,
    tree: &load::Tree,
    allowed: &[String],
) -> Result<(), LoadError> {
    let root_dir = sources::parent(root);
    let reached: BTreeSet<&str> = tree.files.iter().map(|file| file.path.as_str()).collect();
    let other_roots = [
        sources::join(root_dir, "lib.rs"),
        sources::join(root_dir, "main.rs"),
    ];
    let bin_dir = format!("{}/", sources::join(root_dir, "bin"));

    let under = |file: &str, prefix: &str| {
        let prefix = prefix.trim_end_matches('/');
        file == prefix
            || file
                .strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('/'))
    };
    let mut used: BTreeSet<&str> = BTreeSet::new();
    let mut unreached = Vec::new();
    for file in sources.rust_files(root_dir) {
        if reached.contains(file.as_str())
            || other_roots.contains(&file)
            || file.starts_with(&bin_dir)
        {
            continue;
        }
        match allowed.iter().find(|prefix| under(&file, prefix)) {
            Some(prefix) => {
                used.insert(prefix);
            }
            None => unreached.push(file),
        }
    }
    if !unreached.is_empty() {
        return Err(LoadError::UnreachedFiles { files: unreached });
    }
    let stale: Vec<String> = allowed
        .iter()
        .filter(|prefix| !used.contains(prefix.as_str()))
        .cloned()
        .collect();
    if !stale.is_empty() {
        return Err(LoadError::StaleAllowance { prefixes: stale });
    }
    Ok(())
}
