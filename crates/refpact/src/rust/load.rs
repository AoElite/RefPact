//! Builds the module tree: follows `mod` declarations from the crate root, parses every
//! file, and records the names that each module declares and imports.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use syn::{Attribute, Expr, Item, Lit, Meta};

use crate::rust::cfg;
use crate::rust::extract::{self, Extracted};
use crate::rust::macros;
use crate::rust::scope::{Module, Name, Names, Resolver};
use crate::rust::sources::{self, Sources};
use crate::rust::syntax::{declare_item, ident, is_test_code, nested_metas, LoadedFile};
use crate::rust::workers;

/// Why a crate cannot be analysed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoadError {
    /// The crate root does not exist.
    RootNotFound {
        /// The files that were looked for.
        tried: Vec<String>,
    },
    /// A file cannot be parsed as Rust.
    Parse {
        /// The file.
        file: String,
        /// The line of the error.
        line: u32,
        /// The column of the error.
        column: u32,
        /// The message of the parser.
        message: String,
    },
    /// A `mod name;` declaration has no file.
    ModuleFileNotFound {
        /// The path of the module.
        module: String,
        /// The file that has the declaration.
        declared_in: String,
        /// The line of the declaration.
        line: u32,
        /// The files that were looked for.
        tried: Vec<String>,
    },
    /// Rust files below the root directory that no `mod` declaration reaches.
    UnreachedFiles {
        /// The files.
        files: Vec<String>,
    },
    /// An allowance for unreached files that matches no unreached file.
    StaleAllowance {
        /// The allowed files or directories that match nothing.
        prefixes: Vec<String>,
    },
    /// The `mod` declarations nest deeper than any real crate.
    TooDeep {
        /// The file at which the load stopped.
        file: String,
    },
    /// A `Cargo.toml` file that a workspace load needs is not there or lacks a value.
    Manifest {
        /// The manifest file.
        file: String,
        /// What is wrong with it.
        message: String,
    },
    /// The compiler cannot say what the options of the target of the load are.
    Target {
        /// What went wrong.
        message: String,
    },
    /// The index of rust-analyzer cannot be made or read. A load with `resolving_members` needs it.
    Index {
        /// What went wrong.
        message: String,
    },
    /// A crate of a workspace cannot be analysed.
    Member {
        /// The directory of the crate, relative to the workspace. Empty for the root package.
        dir: String,
        /// Why the crate cannot be analysed.
        error: Box<LoadError>,
    },
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LoadError::RootNotFound { tried } => {
                write!(
                    f,
                    "the crate root was not found; tried {}",
                    tried.join(", ")
                )
            }
            LoadError::Parse {
                file,
                line,
                column,
                message,
            } => {
                write!(f, "{file}:{line}:{column}: cannot parse: {message}")
            }
            LoadError::ModuleFileNotFound {
                module,
                declared_in,
                line,
                tried,
            } => write!(
                f,
                "{declared_in}:{line}: no file for module `{module}`; tried {}",
                tried.join(", ")
            ),
            LoadError::UnreachedFiles { files } => write!(
                f,
                "these files are not reached from the crate root, so they are not checked: {}. \
                 A `mod` declaration inside a macro is seen only in `cfg_if!` and in a \
                 macro whose whole argument is a list of items. Allow the files with \
                 `allow_unreached_files`, or declare their modules where refpact sees them",
                files.join(", ")
            ),
            LoadError::StaleAllowance { prefixes } => write!(
                f,
                "`allow_unreached_files` names paths that match no unreached file: {}",
                prefixes.join(", ")
            ),
            LoadError::TooDeep { file } => {
                write!(
                    f,
                    "{file}: the `mod` declarations nest too deep; is there a `#[path]` cycle?"
                )
            }
            LoadError::Manifest { file, message } => write!(f, "{file}: {message}"),
            LoadError::Target { message } => write!(
                f,
                "the load needs the `cfg` options of its target from the compiler: {message}"
            ),
            LoadError::Index { message } => write!(
                f,
                "the load needs the index of rust-analyzer to resolve method calls: {message}"
            ),
            // The root package of a workspace has no directory below it.
            LoadError::Member { dir, error } if dir.is_empty() => {
                write!(f, "in the crate at the root of the workspace: {error}")
            }
            LoadError::Member { dir, error } => write!(f, "in the crate at `{dir}`: {error}"),
        }
    }
}

impl std::error::Error for LoadError {}

pub(crate) struct Tree {
    pub modules: Vec<Module>,
    pub files: Vec<LoadedFile>,
}

/// What one file declares, as data that can leave the thread that parsed the file. A syntax
/// tree cannot: the positions of its tokens are known only to that thread.
struct Outline {
    /// The whole file is test code.
    test: bool,
    /// The conditions of the `#![cfg(..)]` attributes of the file, as tags.
    cfg: Vec<String>,
    /// The number of lines of the file.
    lines: u32,
    steps: Vec<Step>,
}

/// One step through the items of a file or of an inline module, in the order of the text.
enum Step {
    /// The names of items that are not modules.
    Names(Names),
    Module {
        name: String,
        /// Declared `pub mod`.
        public: bool,
        test: bool,
        /// The conditions of the `#[cfg(..)]` attributes of the declaration, as tags.
        cfg: Vec<String>,
        paths: PathAttributes,
        /// The line of the name, for the report of a missing file.
        line: u32,
        /// The number of lines of the declaration.
        lines: u32,
        /// The steps of an inline module. `None` for `mod name;`.
        content: Option<Vec<Step>>,
    },
}

/// What is known about one path after a thread read it.
enum Read {
    Missing,
    Failed(LoadError),
    File(Outline),
}

enum Job {
    Parse(String),
    /// Make the facts of the files that this thread parsed. It is the last job of a thread.
    Extract(Arc<Vec<Module>>, Arc<Vec<LoadedFile>>, bool),
}

enum Reply {
    Read(String, Read),
    /// What some files give, each with the position of the file in the tree.
    Facts(Vec<(usize, Extracted)>),
}

/// Reads the module tree of a crate and makes its facts.
///
/// The files are parsed on several threads, and each thread also makes the facts of the
/// files that it parsed. The main thread builds the module tree in the order of the `mod`
/// declarations, as one thread would, so the result does not depend on the threads.
///
/// With `calls`, the result also has what the index of rust-analyzer can resolve.
pub(crate) fn load(
    sources: &dyn Sources,
    root: &str,
    calls: bool,
) -> Result<(Tree, Extracted), LoadError> {
    // Each thread keeps the syntax trees of the files that it parsed.
    let start = || {
        let mut trees: BTreeMap<String, syn::File> = BTreeMap::new();
        move |job: Job| match job {
            Job::Parse(path) => {
                let found = read(sources, &path, &mut trees);
                Reply::Read(path, found)
            }
            Job::Extract(modules, files, calls) => {
                Reply::Facts(facts_of(&trees, &modules, &files, calls))
            }
        }
    };
    let (modules, files, facts) = workers::with_pool(start, |pool| {
        // Each round builds the tree from the files that are known, and asks for the files
        // that the tree names and that are not known. The last round is the result.
        let mut known: BTreeMap<String, Read> = BTreeMap::new();
        // The syntax trees of the files that this thread parsed, when there are no others.
        let mut own: BTreeMap<String, syn::File> = BTreeMap::new();
        let (modules, files) = loop {
            let mut loader = Loader {
                known: &known,
                wanted: BTreeSet::new(),
                modules: vec![Module {
                    path: vec!["crate".to_owned()],
                    parent: None,
                    names: Names::default(),
                    public: true,
                    lines: 0,
                    file: None,
                    cfg: None,
                }],
                files: Vec::new(),
                loaded: BTreeSet::new(),
                depth: 0,
            };
            let result = loader.load_file(0, root.to_owned(), true, false, &[]);
            if loader.wanted.is_empty() {
                result?;
                break (
                    loader.modules,
                    loader
                        .files
                        .into_iter()
                        .flatten()
                        .collect::<Vec<LoadedFile>>(),
                );
            }
            let mut waiting = 0;
            for path in loader.wanted {
                match pool.give(Job::Parse(path)) {
                    Ok(()) => waiting += 1,
                    Err(Job::Parse(path)) => {
                        let found = read(sources, &path, &mut own);
                        known.insert(path, found);
                    }
                    Err(Job::Extract(..)) => {}
                }
            }
            while waiting > 0 {
                if let Reply::Read(path, found) = pool.reply() {
                    known.insert(path, found);
                    waiting -= 1;
                }
            }
        };

        // The tables of the modules are complete. Resolve the `use` leaves in a fixed order
        // before the threads read the tables.
        Resolver { modules: &modules }.resolve_imports();
        let modules = Arc::new(modules);
        let files = Arc::new(files);
        let mut by_file: Vec<Extracted> = files.iter().map(|_| Extracted::default()).collect();
        let mut waiting =
            pool.give_last(|| Job::Extract(Arc::clone(&modules), Arc::clone(&files), calls));
        let mut found = facts_of(&own, &modules, &files, calls);
        while waiting > 0 {
            if let Reply::Facts(facts) = pool.reply() {
                found.extend(facts);
                waiting -= 1;
            }
        }
        for (position, extracted) in found {
            by_file[position] = extracted;
        }
        let mut all = Extracted::default();
        for extracted in by_file {
            let before = all.facts.len();
            let sites = extracted.pending.sites.into_iter();
            all.pending
                .sites
                .extend(sites.map(|(position, site)| (before + position, site)));
            all.pending.impl_fns.extend(extracted.pending.impl_fns);
            all.pending.ends.extend(extracted.pending.ends);
            all.pending.macros.extend(extracted.pending.macros);
            all.facts.extend(extracted.facts);
            all.declarations.extend(extracted.declarations);
            all.impls.extend(extracted.impls);
        }
        Ok((modules, files, all))
    })?;
    // The threads are done, so no other owner exists.
    let (Ok(modules), Ok(files)) = (Arc::try_unwrap(modules), Arc::try_unwrap(files)) else {
        unreachable!("the threads of the pool are done");
    };
    Ok((Tree { modules, files }, facts))
}

/// Reads and parses one file, and keeps its syntax tree in `trees`.
fn read(sources: &dyn Sources, path: &str, trees: &mut BTreeMap<String, syn::File>) -> Read {
    let Some(text) = sources.read(path) else {
        return Read::Missing;
    };
    match syn::parse_file(&text) {
        Ok(ast) => {
            let mut steps = Vec::new();
            outline(&ast.items, &mut steps);
            let outline = Outline {
                test: is_test_code(&ast.attrs),
                cfg: cfg::tags(&ast.attrs),
                lines: text.lines().count() as u32,
                steps,
            };
            trees.insert(path.to_owned(), ast);
            Read::File(outline)
        }
        Err(error) => {
            let start = error.span().start();
            Read::Failed(LoadError::Parse {
                file: path.to_owned(),
                line: start.line as u32,
                column: start.column as u32 + 1,
                message: error.to_string(),
            })
        }
    }
}

/// What the files of the tree whose syntax trees are in `trees` give, each with the
/// position of the file in the tree.
fn facts_of(
    trees: &BTreeMap<String, syn::File>,
    modules: &[Module],
    files: &[LoadedFile],
    calls: bool,
) -> Vec<(usize, Extracted)> {
    files
        .iter()
        .enumerate()
        .filter_map(|(position, file)| {
            Some((
                position,
                extract::extract(modules, file, trees.get(&file.path)?, calls),
            ))
        })
        .collect()
}

/// Writes the steps of some items. The items of a macro call are steps of the same list.
fn outline(items: &[Item], steps: &mut Vec<Step>) {
    for item in items {
        match item {
            Item::Mod(item) => steps.push(Step::Module {
                name: ident(&item.ident),
                public: matches!(item.vis, syn::Visibility::Public(_)),
                test: is_test_code(&item.attrs),
                cfg: cfg::tags(&item.attrs),
                paths: path_attributes(&item.attrs),
                line: item.ident.span().start().line as u32,
                lines: item.content.as_ref().map_or(0, |(brace, _)| {
                    let first = item.mod_token.span.start().line;
                    (brace.span.close().end().line + 1).saturating_sub(first) as u32
                }),
                content: item.content.as_ref().map(|(_, items)| {
                    let mut inner = Vec::new();
                    outline(items, &mut inner);
                    inner
                }),
            }),
            Item::Macro(item) if item.ident.is_none() => {
                if let Some(items) = macros::items_of(&item.mac) {
                    outline(&items, steps);
                }
            }
            other => {
                if !matches!(steps.last(), Some(Step::Names(_))) {
                    steps.push(Step::Names(Names::default()));
                }
                if let Some(Step::Names(names)) = steps.last_mut() {
                    declare_item(names, other);
                }
            }
        }
    }
}

/// Builds the module tree from the files that are known.
struct Loader<'k> {
    known: &'k BTreeMap<String, Read>,
    /// The files that the tree names and that are not known. While there are some, the
    /// tree is not complete, and an error can come from that.
    wanted: BTreeSet<String>,
    modules: Vec<Module>,
    files: Vec<Option<LoadedFile>>,
    loaded: BTreeSet<(usize, String)>,
    depth: usize,
}

/// The file that the items come from. `mod_rs` is true for the root, for `mod.rs`, and for
/// a file named by `#[path]`: the modules below such a file are in its own directory.
struct FileContext<'c> {
    path: &'c str,
    mod_rs: bool,
}

impl FileContext<'_> {
    /// The directory in which the files of child modules are found.
    fn child_dir(&self, inline: &[String]) -> String {
        let mut dir = sources::parent(self.path).to_owned();
        if !self.mod_rs {
            let stem = sources::file_name(self.path).trim_end_matches(".rs");
            dir = sources::join(&dir, stem);
        }
        for name in inline {
            dir = sources::join(&dir, name);
        }
        dir
    }
}

impl Loader<'_> {
    fn load_file(
        &mut self,
        module: usize,
        path: String,
        mod_rs: bool,
        test: bool,
        cfg: &[String],
    ) -> Result<(), LoadError> {
        if !self.loaded.insert((module, path.clone())) {
            return Ok(());
        }
        if self.depth > 128 {
            return Err(LoadError::TooDeep { file: path });
        }
        let known = self.known;
        let outline = match known.get(&path) {
            None => {
                self.wanted.insert(path);
                return Ok(());
            }
            Some(Read::Missing) => return Err(LoadError::RootNotFound { tried: vec![path] }),
            Some(Read::Failed(error)) => return Err(error.clone()),
            Some(Read::File(outline)) => outline,
        };
        let test = test || outline.test;
        let cfg = joined(cfg, &outline.cfg);
        self.narrow(module, &cfg);
        self.modules[module].lines += outline.lines;
        self.modules[module]
            .file
            .get_or_insert_with(|| path.clone());
        let slot = self.files.len();
        self.files.push(None);
        self.depth += 1;
        let context = FileContext {
            path: &path,
            mod_rs,
        };
        self.declare(
            module,
            &outline.steps,
            &context,
            &mut Vec::new(),
            test,
            &cfg,
        )?;
        self.depth -= 1;
        self.files[slot] = Some(LoadedFile {
            path,
            module,
            test,
            cfg,
        });
        Ok(())
    }

    /// Notes the conditions of one declaration of a module. Under `cfg`, a module can be
    /// declared twice; its conditions are those that all its declarations have.
    fn narrow(&mut self, module: usize, cfg: &[String]) {
        match &mut self.modules[module].cfg {
            Some(known) => known.retain(|tag| cfg.contains(tag)),
            none => *none = Some(cfg.to_vec()),
        }
    }

    /// The child module `name` of `parent`. Under `cfg`, a module can be declared twice; it
    /// is public when one declaration is.
    fn child(&mut self, parent: usize, name: &str, public: bool) -> usize {
        if let Some(Name::Module(index)) = self.modules[parent].names.items.get(name) {
            let index = *index;
            self.modules[index].public |= public;
            return index;
        }
        let mut path = self.modules[parent].path.clone();
        path.push(name.to_owned());
        let index = self.modules.len();
        self.modules.push(Module {
            path,
            parent: Some(parent),
            names: Names::default(),
            public,
            lines: 0,
            file: None,
            cfg: None,
        });
        self.modules[parent]
            .names
            .items
            .insert(name.to_owned(), Name::Module(index));
        index
    }

    fn declare(
        &mut self,
        module: usize,
        steps: &[Step],
        context: &FileContext<'_>,
        inline: &mut Vec<String>,
        test: bool,
        cfg: &[String],
    ) -> Result<(), LoadError> {
        for step in steps {
            match step {
                Step::Names(names) => self.modules[module].names.absorb(names),
                Step::Module {
                    name,
                    public,
                    test: own_test,
                    cfg: own_cfg,
                    paths,
                    line,
                    lines,
                    content,
                } => {
                    let child = self.child(module, name, *public);
                    let test = test || *own_test;
                    let cfg = joined(cfg, own_cfg);
                    match content {
                        Some(steps) => {
                            self.narrow(child, &cfg);
                            self.modules[child].lines += lines;
                            let file = &mut self.modules[child].file;
                            file.get_or_insert_with(|| context.path.to_owned());
                            inline.push(paths.always.clone().unwrap_or_else(|| name.clone()));
                            self.declare(child, steps, context, inline, test, &cfg)?;
                            inline.pop();
                        }
                        None => {
                            let under = (test, &cfg[..]);
                            self.load_module_files(
                                child, name, *line, paths, context, inline, under,
                            )?
                        }
                    }
                }
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn load_module_files(
        &mut self,
        module: usize,
        name: &str,
        line: u32,
        paths: &PathAttributes,
        context: &FileContext<'_>,
        inline: &[String],
        (test, cfg): (bool, &[String]),
    ) -> Result<(), LoadError> {
        let child_dir = context.child_dir(inline);
        // A `#[path]` outside inline modules is relative to the directory of the file.
        let attribute_dir = if inline.is_empty() {
            sources::parent(context.path).to_owned()
        } else {
            child_dir.clone()
        };

        let candidates: Vec<(String, bool)> = match &paths.always {
            Some(path) => vec![(sources::join(&attribute_dir, path), true)],
            None => {
                let mut candidates: Vec<(String, bool)> = paths
                    .conditional
                    .iter()
                    .map(|path| (sources::join(&attribute_dir, path), true))
                    .collect();
                candidates.push((sources::join(&child_dir, &format!("{name}.rs")), false));
                candidates.push((sources::join(&child_dir, &format!("{name}/mod.rs")), true));
                candidates
            }
        };

        let mut found = false;
        for (path, mod_rs) in &candidates {
            // A file that is not known yet can exist: `load_file` asks for it.
            if !matches!(self.known.get(path), Some(Read::Missing)) {
                found = true;
                self.load_file(module, path.clone(), *mod_rs, test, cfg)?;
            }
        }
        if found {
            Ok(())
        } else {
            Err(LoadError::ModuleFileNotFound {
                module: self.modules[module].path.join("::"),
                declared_in: context.path.to_owned(),
                line,
                tried: candidates.into_iter().map(|(path, _)| path).collect(),
            })
        }
    }
}

/// The conditions around a declaration and its own, each one time.
fn joined(outer: &[String], own: &[String]) -> Vec<String> {
    let mut all = outer.to_vec();
    for tag in own {
        if !all.contains(tag) {
            all.push(tag.clone());
        }
    }
    all
}

#[derive(Default)]
struct PathAttributes {
    /// `#[path = "..."]`
    always: Option<String>,
    /// `#[cfg_attr(condition, path = "...")]`. All branches of `cfg` are checked.
    conditional: Vec<String>,
}

fn path_attributes(attrs: &[Attribute]) -> PathAttributes {
    let mut paths = PathAttributes::default();
    for attr in attrs {
        if attr.path().is_ident("path") {
            if let Some(path) = path_value(&attr.meta) {
                paths.always = Some(path);
            }
        } else if attr.path().is_ident("cfg_attr") {
            for meta in nested_metas(attr).into_iter().skip(1) {
                if meta.path().is_ident("path") {
                    paths.conditional.extend(path_value(&meta));
                }
            }
        }
    }
    paths
}

fn path_value(meta: &Meta) -> Option<String> {
    match meta {
        Meta::NameValue(pair) => match &pair.value {
            Expr::Lit(literal) => match &literal.lit {
                Lit::Str(text) => Some(text.value()),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }
}
