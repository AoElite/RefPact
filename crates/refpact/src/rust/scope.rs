//! Name tables of modules and blocks, and the resolution of paths against them.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use crate::core::Path;

use crate::rust::tables;

/// What a name declared in a module is.
#[derive(Debug, Clone)]
pub(crate) enum Name {
    /// A child module, by index.
    Module(usize),
    /// A function, type, trait, constant or static.
    Item,
    /// An enum with its variants.
    Enum(Vec<String>),
    /// `extern crate real as name;`
    ExternCrate(String),
}

/// One leaf of a `use` declaration.
#[derive(Debug)]
pub(crate) struct Import {
    pub path: Vec<String>,
    pub leading_colon: bool,
    pub line: u32,
    /// `pub use`: the leaf re-exports its name to other crates.
    pub public: bool,
    /// Conditions on this import and on its enclosing module.
    pub cfg: Vec<String>,
    /// True from the time that the resolution of the leaf begins. A leaf that is asked for
    /// while it is resolved is part of a cycle.
    busy: AtomicBool,
    /// The absolute path of the leaf, after its resolution. `None` inside: not resolved.
    done: OnceLock<Option<Path>>,
}

impl Import {
    pub fn new(
        path: Vec<String>,
        leading_colon: bool,
        line: u32,
        public: bool,
        cfg: Vec<String>,
    ) -> Arc<Import> {
        Arc::new(Import {
            path,
            leading_colon,
            line,
            public,
            cfg,
            busy: AtomicBool::new(false),
            done: OnceLock::new(),
        })
    }
}

/// The names of one scope: a module, or a block inside a function.
#[derive(Debug, Default)]
pub(crate) struct Names {
    pub items: BTreeMap<String, Name>,
    pub imports: BTreeMap<String, Vec<Arc<Import>>>,
    pub globs: Vec<Arc<Import>>,
}

impl Names {
    /// Adds the names of `other`, as if its items were declared after the items of `self`.
    ///
    /// The leaves of `use` declarations are made again and not shared: the path of a leaf
    /// depends on the module that it is in, and one file can be the file of two modules.
    pub fn absorb(&mut self, other: &Names, cfg: &[String]) {
        let again = |import: &Arc<Import>| {
            Import::new(
                import.path.clone(),
                import.leading_colon,
                import.line,
                import.public,
                super::cfg::joined(cfg, &import.cfg),
            )
        };
        for (name, kind) in &other.items {
            self.items.insert(name.clone(), kind.clone());
        }
        for (name, imports) in &other.imports {
            self.imports
                .entry(name.clone())
                .or_default()
                .extend(imports.iter().map(again));
        }
        self.globs.extend(other.globs.iter().map(again));
    }
}

#[derive(Debug)]
pub(crate) struct Module {
    pub path: Path,
    pub parent: Option<usize>,
    pub names: Names,
    /// Declared `pub mod`. The root is public.
    pub public: bool,
    /// The number of lines of the files and of the inline declarations of the module.
    pub lines: u32,
    /// The file of the first declaration of the module: its own file, or for an inline
    /// module the file that has it. `None` before the first declaration is read.
    pub file: Option<String>,
    /// The `cfg` conditions that every declaration of the module is under, as tags. `None`
    /// before the first declaration is read.
    pub cfg: Option<Vec<String>>,
}

/// One way a path can resolve.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Resolved {
    pub path: Path,
    pub exact: bool,
    /// The line of the `use` declaration that the name came through.
    pub via: Option<u32>,
}

enum Lookup {
    Found {
        path: Path,
        via: Option<u32>,
    },
    /// An item declared in a block. It has no path.
    BlockItem,
    /// The name may come from one of these glob imports of other crates.
    Candidates(Vec<(Path, u32)>),
    Missing,
}

/// Where a path of the analysed crate leads.
enum Located {
    Module(usize),
    Enum(usize, String),
    Other,
}

/// How an unresolved single name is treated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Single {
    /// A single unknown name is the name of another crate (`use rand;`).
    Crate,
    /// A single unknown name is dropped.
    Drop,
}

const MAX_DEPTH: usize = 16;

#[derive(Clone, Copy)]
pub(crate) struct Resolver<'a> {
    pub modules: &'a [Module],
}

impl Resolver<'_> {
    /// Resolves a path that is written in `module`, inside the blocks `blocks`.
    ///
    /// Returns nothing when the path names something without a path: a local item, `Self`,
    /// a primitive type, or a single name that no table knows.
    pub fn resolve(
        &self,
        module: usize,
        blocks: &[Names],
        segments: &[String],
        leading_colon: bool,
        single: Single,
    ) -> Vec<Resolved> {
        self.resolve_skipping(module, blocks, segments, leading_colon, single, None)
    }

    /// Resolves the path of one `use` leaf. The leaf itself is not a way to resolve its own
    /// first segment: in `use log::log;`, the first `log` is the crate.
    pub fn resolve_use(
        &self,
        module: usize,
        blocks: &[Names],
        segments: &[String],
        leading_colon: bool,
    ) -> Vec<Resolved> {
        self.resolve_skipping(
            module,
            blocks,
            segments,
            leading_colon,
            Single::Crate,
            Some(segments),
        )
    }

    fn resolve_skipping(
        &self,
        module: usize,
        blocks: &[Names],
        segments: &[String],
        leading_colon: bool,
        single: Single,
        skip: Option<&[String]>,
    ) -> Vec<Resolved> {
        let Some((head, rest)) = segments.split_first() else {
            return Vec::new();
        };
        let exact = |mut path: Path, via| {
            path.extend_from_slice(rest);
            vec![Resolved {
                path,
                exact: true,
                via,
            }]
        };
        if leading_colon {
            return exact(vec![head.clone()], None);
        }
        match head.as_str() {
            "crate" => return exact(vec!["crate".to_owned()], None),
            "Self" => return Vec::new(),
            "self" => {
                if rest.is_empty() && single == Single::Drop {
                    return Vec::new();
                }
                return exact(self.modules[module].path.clone(), None);
            }
            "super" => {
                let mut current = module;
                let mut remaining = segments;
                while let Some((segment, tail)) = remaining.split_first() {
                    if segment != "super" {
                        break;
                    }
                    match self.modules[current].parent {
                        Some(parent) => current = parent,
                        None => return Vec::new(),
                    }
                    remaining = tail;
                }
                let mut path = self.modules[current].path.clone();
                path.extend_from_slice(remaining);
                return vec![Resolved {
                    path,
                    exact: true,
                    via: None,
                }];
            }
            _ => {}
        }

        let candidates = match self.lookup(module, blocks, head, skip) {
            Lookup::Found { path, via } => return exact(path, via),
            Lookup::BlockItem => return Vec::new(),
            Lookup::Candidates(candidates) => candidates,
            Lookup::Missing => Vec::new(),
        };
        if let Some(path) = tables::prelude(head) {
            return exact(path.split("::").map(str::to_owned).collect(), None);
        }
        if tables::is_primitive(head) {
            return Vec::new();
        }

        let mut resolved = Vec::new();
        let starts_lowercase = head.chars().next().is_some_and(|c| !c.is_uppercase());
        if starts_lowercase && (!rest.is_empty() || single == Single::Crate) {
            resolved.extend(exact(vec![head.clone()], None));
        }
        // `std`, `core` and `alloc` are always crates, never names from a glob import.
        let is_sysroot = matches!(head.as_str(), "std" | "core" | "alloc");
        for (base, line) in candidates.into_iter().filter(|_| !is_sysroot) {
            let mut path = base;
            path.extend_from_slice(segments);
            resolved.push(Resolved {
                path,
                exact: false,
                via: Some(line),
            });
        }
        resolved
    }

    /// True when `name` is bound by an explicit `use` in the blocks or the module.
    pub fn is_imported(&self, module: usize, blocks: &[Names], name: &str) -> bool {
        blocks.iter().any(|block| block.imports.contains_key(name))
            || self.modules[module].names.imports.contains_key(name)
    }

    /// Visit each chain separately: alternate re-exports can have distinct definitions
    /// and different conditions for when their names are available.
    pub fn alias_chains(&self, path: &[String], mut visit: impl FnMut(&[Path], &[String])) {
        self.walk_aliases(path, path, &mut Vec::new(), &[], &mut visit);
    }

    fn walk_aliases(
        &self,
        root: &[String],
        path: &[String],
        chain: &mut Vec<Path>,
        cfg: &[String],
        visit: &mut impl FnMut(&[Path], &[String]),
    ) {
        let mut followed = false;
        if chain.len() < MAX_DEPTH {
            self.reexports(path, |next, conditions| {
                if next != root && !chain.contains(&next) {
                    followed = true;
                    chain.push(next.clone());
                    let conditions = super::cfg::joined(cfg, conditions);
                    self.walk_aliases(root, &next, chain, &conditions, visit);
                    chain.pop();
                }
            });
        }
        if !followed {
            visit(chain, cfg);
        }
    }

    /// All names reached from the path, for consumers that collect a set of names.
    pub fn aliases(&self, path: &[String]) -> Vec<Path> {
        let mut aliases = Vec::new();
        self.alias_chains(path, |chain, _| {
            for next in chain {
                if !aliases.contains(next) {
                    aliases.push(next.clone());
                }
            }
        });
        aliases
    }

    /// One step through the first imported segment of a path.
    fn reexports(&self, path: &[String], mut emit: impl FnMut(Path, &[String])) {
        if path.first().map(String::as_str) != Some("crate") {
            return;
        }
        let mut module = 0;
        for (index, segment) in path.iter().enumerate().skip(1) {
            let names = &self.modules[module].names;
            let mut step = |mut target: Path, conditions: &[String]| {
                target.extend_from_slice(&path[index + 1..]);
                emit(target, conditions);
            };
            match names.items.get(segment) {
                Some(Name::Module(child)) => {
                    module = *child;
                    continue;
                }
                Some(Name::ExternCrate(real)) => step(vec![real.clone()], &[]),
                Some(_) => {}
                None => {
                    let mut found = false;
                    for import in names.imports.get(segment).into_iter().flatten() {
                        if let Some(target) = self.import(module, &[], import) {
                            step(target, &import.cfg);
                            found = true;
                        }
                    }
                    if !found {
                        if let Some(base) =
                            self.provided_by_globs(names, module, &[], segment, &mut Vec::new())
                        {
                            step(base, &[]);
                        }
                    }
                }
            }
            return;
        }
    }

    fn lookup(
        &self,
        module: usize,
        blocks: &[Names],
        head: &str,
        skip: Option<&[String]>,
    ) -> Lookup {
        let mut candidates = Vec::new();
        for (depth, block) in blocks.iter().enumerate().rev() {
            let visible = &blocks[..=depth];
            if let Some(found) = self.in_imports(block, module, visible, head, skip) {
                return found;
            }
            if block.items.contains_key(head) {
                return Lookup::BlockItem;
            }
            if let Some(found) = self.in_globs(block, module, visible, head, skip, &mut candidates)
            {
                return found;
            }
        }
        let names = &self.modules[module].names;
        if let Some(name) = names.items.get(head) {
            return Lookup::Found {
                path: self.item_path(module, head, name),
                via: None,
            };
        }
        if let Some(path) = self.conditional_binding(module, head, skip) {
            return Lookup::Found { path, via: None };
        }
        if let Some(found) = self.in_imports(names, module, &[], head, skip) {
            return found;
        }
        if let Some(found) = self.in_globs(names, module, &[], head, skip, &mut candidates) {
            return found;
        }
        if candidates.is_empty() {
            Lookup::Missing
        } else {
            Lookup::Candidates(candidates)
        }
    }

    /// Keep the module binding so alias resolution can attach the condition of each
    /// possible definition. A use leaf cannot supply its own first segment.
    fn conditional_binding(
        &self,
        module: usize,
        name: &str,
        skip: Option<&[String]>,
    ) -> Option<Path> {
        let owner = &self.modules[module];
        let imports = owner.names.imports.get(name)?;
        let enclosing = owner.cfg.as_deref().unwrap_or_default();
        if imports
            .iter()
            .any(|import| Some(import.path.as_slice()) == skip)
            || !imports
                .iter()
                .any(|import| import.cfg.iter().any(|tag| !enclosing.contains(tag)))
        {
            return None;
        }
        let mut path = self.modules[module].path.clone();
        path.push(name.to_owned());
        Some(path)
    }

    fn in_imports(
        &self,
        names: &Names,
        module: usize,
        blocks: &[Names],
        head: &str,
        skip: Option<&[String]>,
    ) -> Option<Lookup> {
        names
            .imports
            .get(head)?
            .iter()
            .filter(|import| Some(import.path.as_slice()) != skip)
            .find_map(|import| {
                let path = self.import(module, blocks, import)?;
                Some(Lookup::Found {
                    path,
                    via: Some(import.line),
                })
            })
    }

    fn in_globs(
        &self,
        names: &Names,
        module: usize,
        blocks: &[Names],
        head: &str,
        skip: Option<&[String]>,
        candidates: &mut Vec<(Path, u32)>,
    ) -> Option<Lookup> {
        for glob in names
            .globs
            .iter()
            .filter(|glob| Some(glob.path.as_slice()) != skip)
        {
            let Some(base) = self.import(module, blocks, glob) else {
                continue;
            };
            if base.first().map(String::as_str) == Some("crate") {
                if let Some(path) = self.provided(&base, head, &mut Vec::new()) {
                    return Some(Lookup::Found {
                        path,
                        via: Some(glob.line),
                    });
                }
            } else {
                candidates.push((base, glob.line));
            }
        }
        None
    }

    fn item_path(&self, module: usize, name: &str, kind: &Name) -> Path {
        match kind {
            Name::ExternCrate(real) => vec![real.clone()],
            _ => {
                let mut path = self.modules[module].path.clone();
                path.push(name.to_owned());
                path
            }
        }
    }

    /// The absolute path of one `use` leaf. `None` when it cannot be resolved.
    pub fn import(&self, module: usize, blocks: &[Names], import: &Import) -> Option<Path> {
        if let Some(path) = import.done.get() {
            return path.clone();
        }
        if import.busy.swap(true, Ordering::Relaxed) {
            return None;
        }
        let path = self
            .resolve_use(module, blocks, &import.path, import.leading_colon)
            .into_iter()
            .find(|resolved| resolved.exact)
            .map(|resolved| resolved.path);
        import.done.get_or_init(|| path.clone());
        path
    }

    /// Resolves every `use` leaf of every module, in the order of the modules.
    ///
    /// After this, the tables of the modules do not change, and several threads can read
    /// them. The order is fixed because the path of a leaf in a cycle of `use` declarations
    /// depends on the leaf that is resolved first.
    pub fn resolve_imports(&self) {
        for (module, entry) in self.modules.iter().enumerate() {
            for import in entry
                .names
                .imports
                .values()
                .flatten()
                .chain(&entry.names.globs)
            {
                self.import(module, &[], import);
            }
        }
    }

    /// The path of `name` when the module or enum at `base` provides it to a glob import.
    ///
    /// `visited` holds the bases that this search has seen. Glob imports can form cycles.
    fn provided(&self, base: &[String], name: &str, visited: &mut Vec<Path>) -> Option<Path> {
        if visited.iter().any(|seen| seen == base) {
            return None;
        }
        visited.push(base.to_vec());
        match self.locate(base) {
            Located::Module(module) => {
                let names = &self.modules[module].names;
                if let Some(kind) = names.items.get(name) {
                    return Some(self.item_path(module, name, kind));
                }
                if let Some(path) = self.conditional_binding(module, name, None) {
                    return Some(path);
                }
                if let Some(imports) = names.imports.get(name) {
                    if let Some(path) = imports
                        .iter()
                        .find_map(|import| self.import(module, &[], import))
                    {
                        return Some(path);
                    }
                }
                self.provided_by_globs(names, module, &[], name, visited)
            }
            Located::Enum(module, enum_name) => {
                let Some(Name::Enum(variants)) = self.modules[module].names.items.get(&enum_name)
                else {
                    return None;
                };
                variants.iter().any(|variant| variant == name).then(|| {
                    let mut path = self.modules[module].path.clone();
                    path.push(enum_name.clone());
                    path.push(name.to_owned());
                    path
                })
            }
            Located::Other => None,
        }
    }

    fn provided_by_globs(
        &self,
        names: &Names,
        module: usize,
        blocks: &[Names],
        name: &str,
        visited: &mut Vec<Path>,
    ) -> Option<Path> {
        names.globs.iter().find_map(|glob| {
            let base = self.import(module, blocks, glob)?;
            if base.first().map(String::as_str) == Some("crate") {
                self.provided(&base, name, visited)
            } else {
                None
            }
        })
    }

    /// The module of the analysed crate that a path names, through re-exports.
    pub fn module_at(&self, path: &[String]) -> Option<usize> {
        match self.locate(path) {
            Located::Module(module) => Some(module),
            Located::Enum(..) | Located::Other => None,
        }
    }

    /// Finds the module or enum that a path of the analysed crate names, through re-exports.
    fn locate(&self, path: &[String]) -> Located {
        let mut seen: BTreeSet<Path> = BTreeSet::new();
        let mut current: Path = path.to_vec();
        loop {
            if let Some(located) = self.locate_declared(&current) {
                return located;
            }
            if !seen.insert(current.clone()) || seen.len() > MAX_DEPTH {
                return Located::Other;
            }
            let mut next = None;
            self.reexports(&current, |path, _| {
                next.get_or_insert(path);
            });
            match next {
                Some(next) => current = next,
                None => return Located::Other,
            }
        }
    }

    fn locate_declared(&self, path: &[String]) -> Option<Located> {
        if path.first().map(String::as_str) != Some("crate") {
            return Some(Located::Other);
        }
        let mut module = 0;
        for (index, segment) in path.iter().enumerate().skip(1) {
            match self.modules[module].names.items.get(segment) {
                Some(Name::Module(child)) => module = *child,
                Some(Name::Enum(_)) if index + 1 == path.len() => {
                    return Some(Located::Enum(module, segment.clone()));
                }
                Some(_) => return Some(Located::Other),
                None => return None,
            }
        }
        Some(Located::Module(module))
    }
}
