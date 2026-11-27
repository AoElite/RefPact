//! Declarations: the named items of each module, with their kind, visibility and relations.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use crate::core::{Declaration, Path, Relation};
use crate::rust::scope::{Module, Name, Resolver};

/// The kinds of item a declaration can have.
pub(crate) mod item_kind {
    /// A function.
    pub(crate) const FN: &str = "fn";
    /// A struct.
    pub(crate) const STRUCT: &str = "struct";
    /// An enum.
    pub(crate) const ENUM: &str = "enum";
    /// A union.
    pub(crate) const UNION: &str = "union";
    /// A trait.
    pub(crate) const TRAIT: &str = "trait";
    /// A type alias.
    pub(crate) const TYPE: &str = "type";
    /// A constant.
    pub(crate) const CONST: &str = "const";
    /// A static.
    pub(crate) const STATIC: &str = "static";
    /// A `macro_rules!` macro.
    pub(crate) const MACRO: &str = "macro";
    /// A field of a struct or a union, at the path of its owner and its name or number.
    pub(crate) const FIELD: &str = "field";
    /// A function of an inherent `impl` or of a trait, at the path of its type or trait.
    pub(crate) const METHOD: &str = "method";
    /// The kinds of the members of items. A rule selects them only when it names them.
    pub(crate) const MEMBERS: &[&str] = &[FIELD, METHOD];
    /// All item kinds.
    pub(crate) const ALL: &[&str] = &[
        FN, STRUCT, ENUM, UNION, TRAIT, TYPE, CONST, STATIC, MACRO, FIELD, METHOD,
    ];
}

/// The visibilities a declaration can have.
pub(crate) mod visibility {
    /// `pub`, or a macro with `#[macro_export]`.
    pub(crate) const PUB: &str = "pub";
    /// `pub(crate)`.
    pub(crate) const PUB_CRATE: &str = "pub(crate)";
    /// `pub(super)` or `pub(in path)`.
    pub(crate) const PUB_RESTRICTED: &str = "pub(restricted)";
    /// No `pub`, or `pub(self)`.
    pub(crate) const PRIVATE: &str = "private";
    /// All visibilities.
    pub(crate) const ALL: &[&str] = &[PUB, PUB_CRATE, PUB_RESTRICTED, PRIVATE];
}

/// The relations a declaration can have.
pub(crate) mod relation {
    /// A trait that an `impl Trait for Type` implements for the type.
    pub(crate) const IMPLEMENTS: &str = "implements";
    /// A derive macro of `#[derive(..)]`.
    pub(crate) const DERIVES: &str = "derives";
    /// Another attribute of the item: a built-in one by its name, any other by its path.
    pub(crate) const ATTRIBUTE: &str = "attribute";
    /// A path at which other crates can name the item: through `pub` modules and
    /// `pub use` re-exports from the crate root.
    pub(crate) const EXPORTED: &str = "exported";
    /// A path in the return type of a function or a method.
    pub(crate) const RETURNS: &str = "returns";
    /// A path in the type of a parameter of a function or a method, except `self`.
    pub(crate) const TAKES: &str = "takes";
    /// A path in the type of a field.
    pub(crate) const HAS_TYPE: &str = "has-type";
    /// All relations.
    pub(crate) const ALL: &[&str] = &[
        IMPLEMENTS, DERIVES, ATTRIBUTE, EXPORTED, RETURNS, TAKES, HAS_TYPE,
    ];
}

/// The modifiers a declaration can have.
pub(crate) mod modifier {
    /// An `async fn`.
    pub(crate) const ASYNC: &str = "async";
    /// A `const fn`.
    pub(crate) const CONST: &str = "const";
    /// An `unsafe fn` or an `unsafe trait`.
    pub(crate) const UNSAFE: &str = "unsafe";
    /// A function with an ABI: `extern "C" fn`.
    pub(crate) const EXTERN: &str = "extern";
    /// A `static mut`.
    pub(crate) const MUT: &str = "mut";
    /// An item with a doc comment (`///`) or a `#[doc = ".."]`.
    pub(crate) const DOCUMENTED: &str = "documented";
    /// An item with `#[doc(hidden)]`.
    pub(crate) const HIDDEN: &str = "hidden";
    /// All modifiers.
    pub(crate) const ALL: &[&str] = &[ASYNC, CONST, UNSAFE, EXTERN, MUT, DOCUMENTED, HIDDEN];
}

/// The modifiers of a signature: `async`, `const`, `unsafe` and `extern`.
pub(crate) fn signature_modifiers(signature: &syn::Signature, found: &mut Vec<String>) {
    let flags = [
        (signature.asyncness.is_some(), modifier::ASYNC),
        (signature.constness.is_some(), modifier::CONST),
        (
            matches!(signature.safety, syn::Safety::Unsafe(_)),
            modifier::UNSAFE,
        ),
        (signature.abi.is_some(), modifier::EXTERN),
    ];
    found.extend(
        flags
            .iter()
            .filter(|(on, _)| *on)
            .map(|(_, name)| (*name).to_owned()),
    );
}

/// The modifiers of the attributes: `documented` for a doc comment, `hidden` for
/// `#[doc(hidden)]`.
pub(crate) fn doc_modifiers(attrs: &[syn::Attribute], found: &mut Vec<String>) {
    let mut documented = false;
    let mut hidden = false;
    for attr in attrs.iter().filter(|attr| attr.path().is_ident("doc")) {
        match &attr.meta {
            syn::Meta::NameValue(_) => documented = true,
            syn::Meta::List(list) => {
                let parser =
                    syn::punctuated::Punctuated::<syn::Meta, syn::Token![,]>::parse_terminated;
                if let Ok(metas) = list.parse_args_with(parser) {
                    hidden |= metas.iter().any(|meta| meta.path().is_ident("hidden"));
                }
            }
            syn::Meta::Path(_) => {}
        }
    }
    if documented {
        found.push(modifier::DOCUMENTED.to_owned());
    }
    if hidden {
        found.push(modifier::HIDDEN.to_owned());
    }
}

/// The visibility of an item as one of [`visibility::ALL`].
pub(crate) fn visibility_of(vis: &syn::Visibility) -> &'static str {
    match vis {
        syn::Visibility::Public(_) => visibility::PUB,
        syn::Visibility::Inherited => visibility::PRIVATE,
        syn::Visibility::Restricted(restricted) => {
            let path = &restricted.path;
            if restricted.in_token.is_none() && path.is_ident("crate") {
                visibility::PUB_CRATE
            } else if restricted.in_token.is_none() && path.is_ident("self") {
                visibility::PRIVATE
            } else {
                visibility::PUB_RESTRICTED
            }
        }
    }
}

/// An `impl Trait for Type`: the paths of the type and of the trait, each with its aliases.
#[derive(Debug, Clone)]
pub(crate) struct Impl {
    pub types: Vec<Path>,
    pub traits: Vec<Path>,
}

/// Adds the relation `implements` to each declaration that an `impl` names as its type.
pub(crate) fn attach_impls(declarations: &mut [Declaration], impls: &[Impl]) {
    // Looked up by the type, so that the work does not grow with declarations times `impl` blocks.
    let mut of_type: HashMap<&Path, Vec<&Impl>> = HashMap::new();
    for found in impls {
        for path in &found.types {
            let blocks = of_type.entry(path).or_default();
            if !blocks.iter().any(|block| std::ptr::eq(*block, found)) {
                blocks.push(found);
            }
        }
    }
    for declaration in declarations.iter_mut() {
        for found in of_type.get(&declaration.path).into_iter().flatten() {
            for target in &found.traits {
                let tie = Relation {
                    relation: relation::IMPLEMENTS.to_owned(),
                    target: target.clone(),
                };
                if !declaration.relations.contains(&tie) {
                    declaration.relations.push(tie);
                }
            }
        }
    }
}

/// Adds the relation `exported` to each `pub` declaration that other crates can name, once
/// for each path at which they can name it.
///
/// The paths are found from the crate root: a `pub mod` leads into its module, a `pub use`
/// gives a new path to its target, and a `pub use` glob gives the names of its module. A
/// `pub` item in a module that is not public is not exported unless a `pub use` names it.
pub(crate) fn attach_exports(declarations: &mut [Declaration], modules: &[Module]) {
    let mut exports = Exports {
        resolver: Resolver { modules },
        visited: BTreeSet::new(),
        paths: BTreeMap::new(),
    };
    exports.walk(0, &["crate".to_owned()]);
    for declaration in declarations.iter_mut() {
        if declaration.visibility != visibility::PUB {
            continue;
        }
        // An exported `macro_rules!` macro is always named at the crate root.
        let paths = if declaration.kind == item_kind::MACRO {
            vec![declaration.path.clone()]
        } else {
            exports
                .paths
                .get(&declaration.path)
                .cloned()
                .unwrap_or_default()
        };
        for target in paths {
            declaration.relations.push(Relation {
                relation: relation::EXPORTED.to_owned(),
                target,
            });
        }
    }
}

struct Exports<'a> {
    resolver: Resolver<'a>,
    /// The modules whose names are already exported. A module is walked once.
    visited: BTreeSet<usize>,
    /// The public paths of each definition, in the order that they are found.
    paths: BTreeMap<Path, Vec<Path>>,
}

impl Exports<'_> {
    /// Exports the names of `module` at `prefix`.
    fn walk(&mut self, module: usize, prefix: &[String]) {
        if !self.visited.insert(module) {
            return;
        }
        let modules = self.resolver.modules;
        let at = |name: &str| {
            let mut path = prefix.to_vec();
            path.push(name.to_owned());
            path
        };
        let names = &modules[module].names;
        for (name, kind) in &names.items {
            match kind {
                Name::Module(child) if modules[*child].public => self.walk(*child, &at(name)),
                Name::Item | Name::Enum(_) => {
                    let mut definition = modules[module].path.clone();
                    definition.push(name.clone());
                    self.record(definition, at(name));
                }
                Name::Module(_) | Name::ExternCrate(_) => {}
            }
        }
        for (binding, imports) in &names.imports {
            for import in imports.iter().filter(|import| import.public) {
                let Some(target) = self.internal(module, import) else {
                    continue;
                };
                match self.resolver.module_at(&target) {
                    Some(child) => self.walk(child, &at(binding)),
                    None => {
                        let resolver = self.resolver;
                        resolver.alias_chains(&target, |chain, _| {
                            self.record(chain.last().unwrap_or(&target).clone(), at(binding));
                        });
                    }
                }
            }
        }
        for glob in names.globs.iter().filter(|glob| glob.public) {
            if let Some(base) = self.internal(module, glob) {
                if let Some(child) = self.resolver.module_at(&base) {
                    self.walk(child, prefix);
                }
            }
        }
    }

    /// The target of a `use` leaf, when it is a path of the analysed crate.
    fn internal(&self, module: usize, import: &crate::rust::scope::Import) -> Option<Path> {
        self.resolver
            .import(module, &[], import)
            .filter(|path| path.first().map(String::as_str) == Some("crate"))
    }

    fn record(&mut self, definition: Path, public: Path) {
        let paths = self.paths.entry(definition).or_default();
        if !paths.contains(&public) {
            paths.push(public);
        }
    }
}
