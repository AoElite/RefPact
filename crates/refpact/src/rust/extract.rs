//! Walks the syntax trees and turns every path and every unsafe construct into a fact.

use std::collections::BTreeSet;

use crate::core::{Certainty, Declaration, Fact, Path, Reference, Relation, What};
use proc_macro2::{Delimiter, Spacing, Span, TokenStream, TokenTree};
use syn::punctuated::Punctuated;
use syn::visit::{self, Visit};
use syn::{Attribute, Generics, Item, Meta, Pat, QSelf, Safety, Stmt, Token};

use crate::rust::calls::{ImplFn, MacroSite, Pending};
use crate::rust::cfg;
use crate::rust::items::{self, item_kind, relation, Impl};
use crate::rust::load::{self, flatten_use, ident, is_test_code, LoadedFile};
use crate::rust::macros::{self, Arguments};
use crate::rust::scope::{Module, Name, Names, Resolved, Resolver, Single};
use crate::rust::tables;

pub(crate) mod kind {
    /// A `use` declaration.
    pub const USE: &str = "use";
    /// A glob import. The path is that of the module.
    pub const USE_GLOB: &str = "use-glob";
    /// An `extern crate` declaration.
    pub const EXTERN_CRATE: &str = "extern-crate";
    /// A path in an expression, a struct literal or a call.
    pub const EXPR: &str = "expr";
    /// A path in a type position.
    pub const TYPE: &str = "type";
    /// A path in a pattern.
    pub const PATTERN: &str = "pattern";
    /// A trait in a bound.
    pub const BOUND: &str = "bound";
    /// The trait of an `impl`.
    pub const IMPL: &str = "impl";
    /// An attribute or derive path, or a path in the arguments of one.
    pub const ATTRIBUTE: &str = "attribute";
    /// The macro of a macro call. The path ends in `!`.
    pub const MACRO: &str = "macro";
    /// A path found in the tokens of a macro that is not parsed.
    pub const MACRO_TOKENS: &str = "macro-tokens";
    /// A call of a method, by the name of the method. Only a crate that is
    /// loaded with `resolving_members` has this kind.
    pub const METHOD_CALL: &str = "method-call";
    /// An access to a field, by the name of the field. Only a crate that is
    /// loaded with `resolving_members` has this kind.
    pub const FIELD: &str = "field";
    /// All kinds that every load gives.
    pub const ALL: &[&str] = &[
        USE,
        USE_GLOB,
        EXTERN_CRATE,
        EXPR,
        TYPE,
        PATTERN,
        BOUND,
        IMPL,
        ATTRIBUTE,
        MACRO,
        MACRO_TOKENS,
    ];
}

pub(crate) mod marker {
    /// An `unsafe fn`.
    pub const UNSAFE_FN: &str = "unsafe-fn";
    /// An `unsafe` block.
    pub const UNSAFE_BLOCK: &str = "unsafe-block";
    /// An `unsafe impl`.
    pub const UNSAFE_IMPL: &str = "unsafe-impl";
    /// An `unsafe trait`.
    pub const UNSAFE_TRAIT: &str = "unsafe-trait";
    /// An `unsafe extern` block.
    pub const UNSAFE_EXTERN: &str = "unsafe-extern";
    /// An attribute written as `#[unsafe(..)]`.
    pub const UNSAFE_ATTR: &str = "unsafe-attr";
    /// All markers.
    pub const ALL: &[&str] = &[
        UNSAFE_FN,
        UNSAFE_BLOCK,
        UNSAFE_IMPL,
        UNSAFE_TRAIT,
        UNSAFE_EXTERN,
        UNSAFE_ATTR,
    ];
}

pub(crate) const TAG_TEST: &str = "test";

/// The facts of one file, in the order of the text.
/// What one file gives: its facts, the items that it declares, and its `impl` blocks of
/// traits.
#[derive(Debug, Default)]
pub(crate) struct Extracted {
    pub facts: Vec<Fact>,
    pub declarations: Vec<Declaration>,
    pub impls: Vec<Impl>,
    /// What the index of rust-analyzer can resolve. Empty unless it was asked for.
    pub pending: Pending,
}

/// The facts of one file. With `calls`, the places of method calls, of field accesses and
/// of `Self::name` paths are noted for [`crate::rust::calls`].
pub(crate) fn extract(
    modules: &[Module],
    file: &LoadedFile,
    ast: &syn::File,
    calls: bool,
) -> Extracted {
    let mut extractor = Extractor::new(modules, file);
    extractor.calls = calls;
    for attr in &ast.attrs {
        extractor.visit_attribute(attr);
    }
    for item in &ast.items {
        extractor.visit_item(item);
    }
    let mut facts = extractor.facts;
    facts.sort_by_key(|fact| (fact.line, fact.column));
    let mut sites = extractor.sites;
    sites.sort_by_key(|site| (site.line, site.column));
    // Each site belongs before the first fact that is after it in the text.
    let sites = sites
        .into_iter()
        .map(|site| {
            let place = (site.line, site.column);
            let before = facts.partition_point(|fact| (fact.line, fact.column) <= place);
            (before, site)
        })
        .collect();
    Extracted {
        facts,
        declarations: extractor.declarations,
        impls: extractor.impls,
        pending: Pending {
            sites,
            impl_fns: extractor.impl_fns,
            ends: extractor
                .ends
                .into_iter()
                .map(|((line, column), end)| ((file.path.clone(), line, column), end))
                .collect(),
            macros: extractor.macros,
        },
    }
}

struct Extractor<'a> {
    resolver: Resolver<'a>,
    file: &'a str,
    module: usize,
    /// Greater than zero inside test code.
    test: u32,
    /// The `cfg` conditions around the code, as tags, outermost first.
    conditions: Vec<String>,
    /// Names of the enclosing blocks, innermost last.
    blocks: Vec<Names>,
    /// Local variables of the enclosing functions, closures, blocks and match arms.
    locals: Vec<BTreeSet<String>>,
    /// Type and const parameters of the enclosing items.
    generics: Vec<BTreeSet<String>>,
    /// True inside a range or literal pattern, where paths are expressions in the tree.
    in_pattern: bool,
    /// The path of the innermost declaration around the code, for the facts.
    item: Option<Path>,
    /// Inside an `impl`: the path of its type, and whether it is an inherent `impl`.
    impl_of: Option<(Path, bool)>,
    /// Inside an `impl` of a trait: the paths of the trait.
    impl_traits: Vec<Path>,
    facts: Vec<Fact>,
    declarations: Vec<Declaration>,
    impls: Vec<Impl>,
    /// Whether the sites are noted.
    calls: bool,
    sites: Vec<Fact>,
    impl_fns: Vec<ImplFn>,
    /// For each path of several segments: the place of its first name, where its facts are,
    /// and the place of its last name, where the index says what the path names.
    ends: Vec<((u32, u32), (u32, u32))>,
    /// The macros whose expansion can hold code that the source does not show.
    macros: Vec<MacroSite>,
}

impl<'a> Extractor<'a> {
    fn new(modules: &'a [Module], file: &'a LoadedFile) -> Self {
        Extractor {
            resolver: Resolver { modules },
            file: &file.path,
            module: file.module,
            test: u32::from(file.test),
            conditions: file.cfg.clone(),
            blocks: Vec::new(),
            locals: Vec::new(),
            generics: Vec::new(),
            in_pattern: false,
            item: None,
            impl_of: None,
            impl_traits: Vec::new(),
            facts: Vec::new(),
            declarations: Vec::new(),
            impls: Vec::new(),
            calls: false,
            sites: Vec::new(),
            impl_fns: Vec::new(),
            ends: Vec::new(),
            macros: Vec::new(),
        }
    }

    /// Notes a macro for its expansion: `name` is the last name of its path, and `last` is
    /// the last line of the code that the macro is given.
    fn macro_site(&mut self, name: &syn::Ident, last: u32) {
        if !self.calls {
            return;
        }
        let start = name.span().start();
        self.macros.push(MacroSite {
            file: self.file.to_owned(),
            line: start.line as u32,
            column: start.column as u32 + 1,
            name: ident(name),
            module: self.module,
            test: self.test > 0,
            cfg: self.conditions.clone(),
            item: self.item.clone(),
            last: last.max(start.line as u32),
        });
    }

    /// Notes the derives and the attribute macros among the attributes of an item that ends
    /// at the line `last`.
    fn attribute_macros(&mut self, attrs: &[Attribute], last: u32) {
        if !self.calls {
            return;
        }
        for attr in attrs {
            let path = attr.path();
            let (Some(first), Some(name)) = (path.segments.first(), path.segments.last()) else {
                continue;
            };
            let head = ident(&first.ident);
            let single = path.segments.len() == 1 && path.leading_colon.is_none();
            if single && head == "derive" {
                let derived =
                    attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated);
                for derived in derived.iter().flatten() {
                    // The derives of the language write code that uses the language only.
                    let name = derived.segments.last().map(|segment| &segment.ident);
                    let single = derived.segments.len() == 1;
                    match name {
                        Some(name) if !(single && tables::is_builtin_derive(&ident(name))) => {
                            self.macro_site(name, last);
                        }
                        _ => {}
                    }
                }
            } else if !(single && tables::is_builtin_attribute(&head)) && !tables::is_tool(&head) {
                self.macro_site(&name.ident, last);
            }
        }
    }

    /// Notes a place whose name only the types resolve: a reference of `kind` without a
    /// target.
    fn site(&mut self, span: Span, kind: &str) {
        if !self.calls {
            return;
        }
        let reference = Reference {
            target: Vec::new(),
            aliases: Vec::new(),
            kind: kind.to_owned(),
            certainty: Certainty::Exact,
            properties: Vec::new(),
        };
        let facts = std::mem::take(&mut self.facts);
        self.fact(span, None, What::Reference(reference));
        self.sites.extend(self.facts.pop());
        self.facts = facts;
    }

    fn fact(&mut self, span: Span, note: Option<String>, what: What) {
        let start = span.start();
        self.facts.push(Fact {
            unit: self.resolver.modules[self.module].path.clone(),
            file: self.file.to_owned(),
            line: start.line as u32,
            column: start.column as u32 + 1,
            tags: self.tags(),
            note,
            item: self.item.clone(),
            what,
        });
    }

    /// The tags of what is found here: the tag of test code inside test code, and the `cfg`
    /// conditions around the place.
    fn tags(&self) -> Vec<String> {
        let mut tags = Vec::with_capacity(self.conditions.len() + 1);
        if self.test > 0 {
            tags.push(TAG_TEST.to_owned());
        }
        for condition in &self.conditions {
            if !tags.contains(condition) {
                tags.push(condition.clone());
            }
        }
        tags
    }

    fn marker(&mut self, span: Span, marker: &str) {
        self.fact(span, None, What::Marker(marker.to_owned()));
    }

    /// Records the paths that a path of the code resolves to. `span` is the first name of
    /// the path and `end` is its last name.
    fn references(&mut self, span: Span, end: Span, kind: &str, resolved: Vec<Resolved>) {
        let (first, last) = (span.start(), end.start());
        if self.calls && !resolved.is_empty() && first != last {
            self.ends.push((
                (first.line as u32, first.column as u32 + 1),
                (last.line as u32, last.column as u32 + 1),
            ));
        }
        for resolved in resolved {
            let note = resolved.via.map(|line| {
                if resolved.exact {
                    format!("through the `use` at line {line}")
                } else {
                    format!("may come through the glob import at line {line}")
                }
            });
            let reference = Reference {
                aliases: self.resolver.aliases(&resolved.path),
                target: resolved.path,
                kind: kind.to_owned(),
                certainty: if resolved.exact {
                    Certainty::Exact
                } else {
                    Certainty::Candidate
                },
                properties: Vec::new(),
            };
            self.fact(span, note, What::Reference(reference));
        }
    }

    /// The paths that a path of the code resolves to, each followed by its aliases.
    fn resolved_paths(&self, path: &syn::Path) -> Vec<Path> {
        let segments: Vec<String> = path.segments.iter().map(|s| ident(&s.ident)).collect();
        let resolved = self.resolver.resolve(
            self.module,
            &self.blocks,
            &segments,
            path.leading_colon.is_some(),
            Single::Drop,
        );
        let mut paths = Vec::new();
        for found in resolved {
            let aliases = self.resolver.aliases(&found.path);
            paths.push(found.path);
            paths.extend(aliases);
        }
        paths
    }

    /// The relations that the attributes of an item give: its derive macros, and its other
    /// attributes except documentation, `cfg_attr` and those of tools.
    fn attribute_relations(&self, attrs: &[Attribute]) -> Vec<Relation> {
        let mut relations = Vec::new();
        let mut add = |kind: &str, target: Path| {
            let tie = Relation {
                relation: kind.to_owned(),
                target,
            };
            if !relations.contains(&tie) {
                relations.push(tie);
            }
        };
        for attr in attrs {
            let path = attr.path();
            let Some(first) = path.segments.first() else {
                continue;
            };
            let head = ident(&first.ident);
            let single = path.segments.len() == 1 && path.leading_colon.is_none();
            if single && head == "derive" {
                let Ok(paths) =
                    attr.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                else {
                    continue;
                };
                for derived in &paths {
                    for target in self.resolved_paths(derived) {
                        add(relation::DERIVES, target);
                    }
                }
            } else if single && matches!(head.as_str(), "doc" | "cfg_attr") {
            } else if single && tables::is_builtin_attribute(&head) {
                add(relation::ATTRIBUTE, vec![head]);
            } else if !tables::is_tool(&head) {
                for target in self.resolved_paths(path) {
                    add(relation::ATTRIBUTE, target);
                }
            }
        }
        relations
    }

    /// Records a named item of a module, and returns its path.
    fn declare(&mut self, item: &Item) -> Option<Path> {
        let (name, kind, vis) = match item {
            Item::Fn(item) => (&item.sig.ident, item_kind::FN, Some(&item.vis)),
            Item::Struct(item) => (&item.ident, item_kind::STRUCT, Some(&item.vis)),
            Item::Enum(item) => (&item.ident, item_kind::ENUM, Some(&item.vis)),
            Item::Union(item) => (&item.ident, item_kind::UNION, Some(&item.vis)),
            Item::Trait(item) => (&item.ident, item_kind::TRAIT, Some(&item.vis)),
            Item::Type(item) => (&item.ident, item_kind::TYPE, Some(&item.vis)),
            Item::Const(item) => (&item.ident, item_kind::CONST, Some(&item.vis)),
            Item::Static(item) => (&item.ident, item_kind::STATIC, Some(&item.vis)),
            Item::Macro(item) => (item.ident.as_ref()?, item_kind::MACRO, None),
            _ => return None,
        };
        let start = name.span().start();
        let name = ident(name);
        if name == "_" {
            return None;
        }
        let attrs = item_attributes(item);
        let visibility = match vis {
            Some(vis) => items::visibility_of(vis),
            None if attrs
                .iter()
                .any(|attr| attr.path().is_ident("macro_export")) =>
            {
                items::visibility::PUB
            }
            None => items::visibility::PRIVATE,
        };
        let unit = self.resolver.modules[self.module].path.clone();
        // An exported `macro_rules!` macro is an item of the crate root.
        let exported_macro = kind == item_kind::MACRO && visibility == items::visibility::PUB;
        let mut path = if exported_macro {
            self.resolver.modules[0].path.clone()
        } else {
            unit.clone()
        };
        path.push(name.clone());
        let mut relations = self.attribute_relations(attrs);
        if let Item::Fn(item) = item {
            relations.extend(self.signature_relations(&item.sig));
        }
        let mut modifiers = Vec::new();
        match item {
            Item::Fn(item) => items::signature_modifiers(&item.sig, &mut modifiers),
            Item::Trait(item) if item.unsafety.is_some() => {
                modifiers.push(items::modifier::UNSAFE.to_owned());
            }
            Item::Static(item) if matches!(item.mutability, syn::StaticMutability::Mut(_)) => {
                modifiers.push(items::modifier::MUT.to_owned());
            }
            _ => {}
        }
        items::doc_modifiers(attrs, &mut modifiers);
        self.declarations.push(Declaration {
            path: path.clone(),
            name,
            unit,
            file: self.file.to_owned(),
            line: start.line as u32,
            column: start.column as u32 + 1,
            end_line: Some(end_of(item, start)),
            tags: self.tags(),
            kind: kind.to_owned(),
            visibility: visibility.to_owned(),
            relations,
            modifiers,
        });
        Some(path)
    }

    /// Records a field or a method: a member of the item at `owner`.
    fn declare_member(&mut self, member: Member<'_>, owner: &Path) -> Path {
        let mut path = owner.clone();
        path.push(member.name.clone());
        let start = member.span.start();
        self.declarations.push(Declaration {
            path: path.clone(),
            name: member.name,
            unit: self.resolver.modules[self.module].path.clone(),
            file: self.file.to_owned(),
            line: start.line as u32,
            column: start.column as u32 + 1,
            end_line: Some(member.end),
            tags: self.tags(),
            kind: member.kind.to_owned(),
            visibility: member.visibility,
            relations: member.relations,
            modifiers: member.modifiers,
        });
        path
    }

    /// The relations of a signature: the paths in the types of the parameters and in the
    /// return type. The generic parameters of the signature are no paths.
    fn signature_relations(&mut self, signature: &syn::Signature) -> Vec<Relation> {
        let mut relations = Vec::new();
        self.with_generics(&signature.generics, |this| {
            for input in &signature.inputs {
                if let syn::FnArg::Typed(typed) = input {
                    this.type_relations(relation::TAKES, &typed.ty, &mut relations);
                }
            }
            if let syn::ReturnType::Type(_, ty) = &signature.output {
                this.type_relations(relation::RETURNS, ty, &mut relations);
            }
        });
        relations
    }

    /// Adds a relation of this kind to each path in a type, resolved, with its aliases.
    fn type_relations(&self, kind: &str, ty: &syn::Type, relations: &mut Vec<Relation>) {
        let mut found = TypePaths(Vec::new());
        found.visit_type(ty);
        for path in found.0 {
            let first = path.segments.first().map(|segment| ident(&segment.ident));
            let generic = path.leading_colon.is_none()
                && first
                    .as_deref()
                    .is_some_and(|name| name == "Self" || self.is_generic(name));
            if generic {
                continue;
            }
            for target in self.resolved_paths(path) {
                let tie = Relation {
                    relation: kind.to_owned(),
                    target,
                };
                if !relations.contains(&tie) {
                    relations.push(tie);
                }
            }
        }
    }

    /// Visits the fields of a struct or a union, and records them as members of the item
    /// around them. A field without a name is at its type, or at `fallback`.
    /// `close` is the token that closes the list.
    fn fields(&mut self, fields: &Punctuated<syn::Field, Token![,]>, close: Span, fallback: Span) {
        let owner = self.item.clone().filter(|_| self.blocks.is_empty());
        for (number, pair) in fields.pairs().enumerate() {
            let field = *pair.value();
            // A field ends at its comma. The last field can have none: it ends where the
            // list closes.
            let end = pair.punct().map_or(close, |comma| comma.span);
            let outer = self.item.clone();
            let conditions = self.conditions.len();
            self.conditions.extend(cfg::tags(&field.attrs));
            if let Some(owner) = &owner {
                let name = field
                    .ident
                    .as_ref()
                    .map_or_else(|| number.to_string(), ident);
                let span = match (&field.ident, &field.ty) {
                    (Some(name), _) => name.span(),
                    (None, syn::Type::Path(path)) => path
                        .path
                        .segments
                        .first()
                        .map_or(fallback, |segment| segment.ident.span()),
                    (None, _) => fallback,
                };
                let mut relations = self.attribute_relations(&field.attrs);
                self.type_relations(relation::HAS_TYPE, &field.ty, &mut relations);
                let member = Member {
                    name,
                    span,
                    end: end.end().line as u32,
                    kind: item_kind::FIELD,
                    visibility: items::visibility_of(&field.vis).to_owned(),
                    relations,
                    modifiers: {
                        let mut modifiers = Vec::new();
                        items::doc_modifiers(&field.attrs, &mut modifiers);
                        modifiers
                    },
                };
                self.item = Some(self.declare_member(member, owner));
            }
            self.visit_field(field);
            self.conditions.truncate(conditions);
            self.item = outer;
        }
    }

    /// Visits a function of an `impl` or a trait at `owner`, and records it as a method when
    /// it is one. `visibility` is `None` for a function of a trait `impl`, which is no method.
    /// `end` is the line on which the function ends.
    fn method(
        &mut self,
        attrs: &[Attribute],
        signature: &syn::Signature,
        end: u32,
        visibility: Option<String>,
        visit: impl FnOnce(&mut Self),
    ) {
        let outer = self.item.clone();
        let owner = match (&self.impl_of, &self.item) {
            (Some((owner, _)), _) => Some(owner.clone()),
            (None, owner) => owner.clone(),
        };
        let owner = owner.filter(|_| self.blocks.is_empty());
        self.function(signature, |this| {
            if let Some(owner) = owner {
                let name = ident(&signature.ident);
                this.item = Some(match visibility {
                    Some(visibility) => {
                        let mut relations = this.attribute_relations(attrs);
                        relations.extend(this.signature_relations(signature));
                        let mut modifiers = Vec::new();
                        items::signature_modifiers(signature, &mut modifiers);
                        items::doc_modifiers(attrs, &mut modifiers);
                        let member = Member {
                            name,
                            span: signature.ident.span(),
                            end,
                            kind: item_kind::METHOD,
                            visibility,
                            relations,
                            modifiers,
                        };
                        this.declare_member(member, &owner)
                    }
                    None => {
                        let mut path = owner;
                        path.push(name);
                        if this.calls {
                            let start = signature.ident.span().start();
                            let of_trait = |of_trait: &Path| {
                                let mut function = of_trait.clone();
                                function.extend(path.last().cloned());
                                function
                            };
                            this.impl_fns.push(ImplFn {
                                file: this.file.to_owned(),
                                line: start.line as u32,
                                column: start.column as u32 + 1,
                                path: path.clone(),
                                traits: this.impl_traits.iter().map(of_trait).collect(),
                            });
                        }
                        path
                    }
                });
            }
            visit(this);
        });
        self.item = outer;
    }

    fn is_local(&self, name: &str) -> bool {
        self.locals.iter().any(|frame| frame.contains(name))
    }

    fn is_generic(&self, name: &str) -> bool {
        self.generics.iter().any(|frame| frame.contains(name))
    }

    fn bind(&mut self, name: String) {
        if let Some(frame) = self.locals.last_mut() {
            frame.insert(name);
        }
    }

    /// Records a path of the syntax tree. With a qualified self type (`<T as Trait>::name`),
    /// only the trait is a path that can be named.
    fn path(&mut self, qself: Option<&QSelf>, path: &syn::Path, kind: &str) {
        for segment in &path.segments {
            Visit::visit_path_arguments(self, &segment.arguments);
        }
        let mut count = path.segments.len();
        if let Some(qself) = qself {
            Visit::visit_type(self, &qself.ty);
            count = qself.position;
        }
        let segments: Vec<String> = path
            .segments
            .iter()
            .take(count)
            .map(|s| ident(&s.ident))
            .collect();
        // What `Self::name` names depends on the type.
        if let (None, [first, .., last]) = (qself, &segments[..]) {
            if first == "Self" && segments.len() == 2 {
                let _ = last;
                self.site(path.segments[1].ident.span(), kind);
            }
        }
        if let (Some(first), Some(last)) = (
            path.segments.first(),
            count
                .checked_sub(1)
                .and_then(|last| path.segments.iter().nth(last)),
        ) {
            self.named(
                first.ident.span(),
                last.ident.span(),
                &segments,
                path.leading_colon.is_some(),
                kind,
            );
        }
    }

    /// Resolves a path given as segments and records the result.
    fn named(
        &mut self,
        span: Span,
        end: Span,
        segments: &[String],
        leading_colon: bool,
        kind: &str,
    ) {
        let Some(head) = segments.first() else { return };
        if !leading_colon {
            if self.is_generic(head) {
                return;
            }
            let value_position = matches!(kind, kind::EXPR | kind::PATTERN | kind::MACRO_TOKENS);
            if segments.len() == 1 && value_position && self.is_local(head) {
                return;
            }
        }
        let resolved = self.resolver.resolve(
            self.module,
            &self.blocks,
            segments,
            leading_colon,
            Single::Drop,
        );
        self.references(span, end, kind, resolved);
    }

    /// Records the macro that is called. The last segment gets a `!`, because a macro and a
    /// module can have the same path: `std::env!` is not `std::env`.
    fn macro_path(&mut self, path: &syn::Path) {
        let Some(first) = path.segments.first() else {
            return;
        };
        let segments: Vec<String> = path.segments.iter().map(|s| ident(&s.ident)).collect();
        let leading_colon = path.leading_colon.is_some();
        let imported = self
            .resolver
            .is_imported(self.module, &self.blocks, &segments[0]);
        let mut resolved = if segments.len() == 1 && !leading_colon && !imported {
            if !tables::is_std_macro(&segments[0]) {
                return;
            }
            vec![Resolved {
                path: vec!["std".to_owned(), segments[0].clone()],
                exact: true,
                via: None,
            }]
        } else {
            self.resolver.resolve(
                self.module,
                &self.blocks,
                &segments,
                leading_colon,
                Single::Drop,
            )
        };
        for resolved in &mut resolved {
            if let Some(last) = resolved.path.last_mut() {
                last.push('!');
            }
        }
        self.references(
            first.ident.span(),
            first.ident.span(),
            kind::MACRO,
            resolved,
        );
    }

    fn with_generics(&mut self, generics: &Generics, visit: impl FnOnce(&mut Self)) {
        let names = generics
            .params
            .iter()
            .filter_map(|param| match param {
                syn::GenericParam::Type(param) => Some(ident(&param.ident)),
                syn::GenericParam::Const(param) => Some(ident(&param.ident)),
                syn::GenericParam::Lifetime(_) => None,
            })
            .collect();
        self.generics.push(names);
        visit(self);
        self.generics.pop();
    }

    fn with_locals(&mut self, visit: impl FnOnce(&mut Self)) {
        self.locals.push(BTreeSet::new());
        visit(self);
        self.locals.pop();
    }

    /// Visits what has some attributes: inside it, the code is test code when the attributes
    /// say so, and is under their `cfg` conditions.
    fn with_test(&mut self, attrs: &[Attribute], visit: impl FnOnce(&mut Self)) {
        if attrs.is_empty() {
            return visit(self);
        }
        let test = is_test_code(attrs);
        let outer = self.conditions.len();
        self.conditions.extend(cfg::tags(attrs));
        self.test += u32::from(test);
        visit(self);
        self.test -= u32::from(test);
        self.conditions.truncate(outer);
    }

    /// A function does not see the local variables of the code around it.
    fn function(&mut self, signature: &syn::Signature, visit: impl FnOnce(&mut Self)) {
        if let Safety::Unsafe(token) = &signature.safety {
            self.marker(token.span, marker::UNSAFE_FN);
        }
        let outer = std::mem::take(&mut self.locals);
        self.with_generics(&signature.generics, |this| this.with_locals(visit));
        self.locals = outer;
    }

    fn module_item(&mut self, node: &syn::ItemMod) {
        for attr in &node.attrs {
            Visit::visit_attribute(self, attr);
        }
        let Some((_, items)) = &node.content else {
            return;
        };
        let names = &self.resolver.modules[self.module].names;
        let child = match names.items.get(&ident(&node.ident)) {
            Some(Name::Module(child)) => *child,
            _ => self.module,
        };
        let outer_module = std::mem::replace(&mut self.module, child);
        let outer_blocks = std::mem::take(&mut self.blocks);
        let outer_locals = std::mem::take(&mut self.locals);
        let outer_generics = std::mem::take(&mut self.generics);
        let outer_item = self.item.take();
        for item in items {
            Visit::visit_item(self, item);
        }
        self.item = outer_item;
        self.module = outer_module;
        self.blocks = outer_blocks;
        self.locals = outer_locals;
        self.generics = outer_generics;
    }

    fn use_item(&mut self, node: &syn::ItemUse) {
        for attr in &node.attrs {
            Visit::visit_attribute(self, attr);
        }
        let leading_colon = node.leading_colon.is_some();
        for leaf in flatten_use(node) {
            let resolved =
                self.resolver
                    .resolve_use(self.module, &self.blocks, &leaf.path, leading_colon);
            let kind = if leaf.glob { kind::USE_GLOB } else { kind::USE };
            self.references(leaf.span, leaf.end, kind, resolved);
        }
    }

    fn macro_item(&mut self, node: &syn::ItemMacro) {
        for attr in &node.attrs {
            Visit::visit_attribute(self, attr);
        }
        if node.ident.is_some() {
            // The body of a macro definition is not code yet. Its paths are still recorded.
            self.scan(node.mac.tokens.clone(), kind::MACRO_TOKENS, true);
        } else if let Some(items) = macros::items_of(&node.mac) {
            self.macro_path(&node.mac.path);
            for item in &items {
                Visit::visit_item(self, item);
            }
        } else {
            self.any_macro(&node.mac);
        }
    }

    fn any_macro(&mut self, node: &syn::Macro) {
        self.macro_path(&node.path);
        let name = node
            .path
            .segments
            .last()
            .map(|segment| ident(&segment.ident))
            .unwrap_or_default();
        // A macro of the standard library writes code that uses the standard library only.
        let single = node.path.segments.len() == 1 && node.path.leading_colon.is_none();
        let imported = self.resolver.is_imported(self.module, &self.blocks, &name);
        let library = matches!(
            node.path
                .segments
                .first()
                .map(|first| ident(&first.ident))
                .as_deref(),
            Some("std" | "core" | "alloc")
        );
        let standard = (single && !imported && tables::is_std_macro(&name)) || library;
        if let (false, Some(last)) = (standard, node.path.segments.last()) {
            let end = node.delimiter.span().close().end().line as u32;
            self.macro_site(&last.ident, end);
        }
        match macros::parse(&name, node.tokens.clone()) {
            Some(Arguments::Expressions(expressions)) => {
                for expression in &expressions {
                    Visit::visit_expr(self, expression);
                }
            }
            Some(Arguments::Matches {
                value,
                pattern,
                guard,
            }) => {
                Visit::visit_expr(self, &value);
                self.with_locals(|this| {
                    Visit::visit_pat(this, &pattern);
                    if let Some(guard) = &guard {
                        Visit::visit_expr(this, guard);
                    }
                });
            }
            Some(Arguments::Items(items)) => {
                for item in &items {
                    Visit::visit_item(self, item);
                }
            }
            None => {
                self.scan(node.tokens.clone(), kind::MACRO_TOKENS, true);
                self.macro_sites(node.tokens.clone());
            }
        }
    }

    /// Notes the method calls and the field accesses in the tokens of a macro call that is
    /// not parsed: `.name(..)` and `.name::<..>(..)` are calls, and another `.name` or `.0` is
    /// an access. rust-analyzer expands the macro, so its index says what each one names; a
    /// site that the expansion does not use as code resolves to nothing.
    fn macro_sites(&mut self, stream: TokenStream) {
        let tokens: Vec<TokenTree> = stream.into_iter().collect();
        for (index, token) in tokens.iter().enumerate() {
            if let TokenTree::Group(group) = token {
                self.macro_sites(group.stream());
                continue;
            }
            // One `.`, not the second of `..`.
            let after_dot = punct_alone(token) == Some('.')
                && (index == 0 || punct(&tokens[index - 1]) != Some('.'));
            if !after_dot {
                continue;
            }
            match tokens.get(index + 1) {
                Some(TokenTree::Ident(name)) if !tables::is_keyword(&ident(name)) => {
                    let call = match tokens.get(index + 2) {
                        Some(TokenTree::Group(group)) => {
                            group.delimiter() == Delimiter::Parenthesis
                        }
                        Some(next) => punct(next) == Some(':'),
                        None => false,
                    };
                    let kind = if call { kind::METHOD_CALL } else { kind::FIELD };
                    self.site(name.span(), kind);
                }
                Some(TokenTree::Literal(number))
                    if number.to_string().bytes().all(|byte| byte.is_ascii_digit()) =>
                {
                    self.site(number.span(), kind::FIELD);
                }
                _ => {}
            }
        }
    }

    /// Finds paths in a token stream that could not be parsed: `a::b::c` anywhere, and a
    /// single name when `singles` is true and an explicit `use` brings that name in.
    fn scan(&mut self, stream: TokenStream, kind: &str, singles: bool) {
        let tokens: Vec<TokenTree> = stream.into_iter().collect();
        let mut index = 0;
        while index < tokens.len() {
            match &tokens[index] {
                TokenTree::Group(group) => {
                    self.scan(group.stream(), kind, singles);
                    index += 1;
                }
                TokenTree::Ident(first) => {
                    let start = index;
                    let mut segments = vec![ident(first)];
                    let mut last = first.span();
                    index += 1;
                    while index + 2 < tokens.len()
                        && is_path_separator(&tokens[index], &tokens[index + 1])
                    {
                        let TokenTree::Ident(next) = &tokens[index + 2] else {
                            break;
                        };
                        segments.push(ident(next));
                        last = next.span();
                        index += 3;
                    }
                    self.scanned(
                        &tokens,
                        start,
                        index,
                        (first.span(), last),
                        &segments,
                        kind,
                        singles,
                    );
                }
                _ => index += 1,
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn scanned(
        &mut self,
        tokens: &[TokenTree],
        start: usize,
        end: usize,
        (span, last): (Span, Span),
        segments: &[String],
        kind: &str,
        singles: bool,
    ) {
        let head = segments[0].as_str();
        let before = start.checked_sub(1).and_then(|index| punct(&tokens[index]));
        if tables::is_keyword(head) {
            return;
        }
        // `$crate::a` is a path of this crate. Any other `$name` is a macro variable.
        if before == Some('$') && head != "crate" {
            return;
        }
        let leading_colon = start >= 2 && is_path_separator(&tokens[start - 2], &tokens[start - 1]);
        if leading_colon {
            // `<T as Trait>::name` and `Type::<T>::name` continue a path that began earlier.
            let continues =
                start.checked_sub(3).and_then(|index| punct(&tokens[index])) == Some('>');
            if continues || segments.len() == 1 {
                return;
            }
        }
        if segments.len() == 1 {
            let after = tokens.get(end).and_then(punct_alone);
            let is_field_or_label = before == Some('.') || matches!(after, Some(':' | '='));
            if !singles
                || is_field_or_label
                || !self.resolver.is_imported(self.module, &self.blocks, head)
            {
                return;
            }
        }
        self.named(span, last, segments, leading_colon, kind);
    }

    fn meta(&mut self, meta: &Meta) {
        let path = meta.path();
        let Some(first) = path.segments.first() else {
            return;
        };
        let head = ident(&first.ident);
        let nested = |meta: &Meta| match meta {
            Meta::List(list) => list
                .parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
                .map(|metas| metas.into_iter().collect::<Vec<_>>())
                .unwrap_or_default(),
            _ => Vec::new(),
        };
        if path.segments.len() == 1 && path.leading_colon.is_none() {
            match head.as_str() {
                "derive" => {
                    let Meta::List(list) = meta else { return };
                    let Ok(paths) =
                        list.parse_args_with(Punctuated::<syn::Path, Token![,]>::parse_terminated)
                    else {
                        return;
                    };
                    for path in &paths {
                        self.path(None, path, kind::ATTRIBUTE);
                    }
                    return;
                }
                "cfg_attr" => {
                    for meta in nested(meta).iter().skip(1) {
                        self.meta(meta);
                    }
                    return;
                }
                "unsafe" => {
                    self.marker(first.ident.span(), marker::UNSAFE_ATTR);
                    for meta in &nested(meta) {
                        self.meta(meta);
                    }
                    return;
                }
                name if tables::is_builtin_attribute(name) => return,
                _ => {}
            }
        } else if tables::is_tool(&head) {
            return;
        }
        self.path(None, path, kind::ATTRIBUTE);
        match meta {
            Meta::List(list) => self.scan(list.tokens.clone(), kind::ATTRIBUTE, false),
            Meta::NameValue(pair) => Visit::visit_expr(self, &pair.value),
            Meta::Path(_) => {}
        }
    }
}

/// A field or a method, before it is a declaration.
struct Member<'a> {
    name: String,
    span: Span,
    /// The line on which the member ends.
    end: u32,
    kind: &'a str,
    visibility: String,
    relations: Vec<Relation>,
    modifiers: Vec<String>,
}

/// The paths in a type: those of the types and of the traits in bounds. Macros in types are
/// not read.
struct TypePaths<'ast>(Vec<&'ast syn::Path>);

impl<'ast> Visit<'ast> for TypePaths<'ast> {
    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        if node.qself.is_none() {
            self.0.push(&node.path);
        }
        visit::visit_type_path(self, node);
    }

    fn visit_trait_bound(&mut self, node: &'ast syn::TraitBound) {
        self.0.push(&node.path);
        visit::visit_trait_bound(self, node);
    }

    fn visit_macro(&mut self, _: &'ast syn::Macro) {}
}

fn punct(token: &TokenTree) -> Option<char> {
    match token {
        TokenTree::Punct(punct) => Some(punct.as_char()),
        _ => None,
    }
}

/// The character of a punctuation token that is not the start of a longer operator.
fn punct_alone(token: &TokenTree) -> Option<char> {
    match token {
        TokenTree::Punct(punct) if punct.spacing() == Spacing::Alone => Some(punct.as_char()),
        _ => None,
    }
}

fn is_path_separator(first: &TokenTree, second: &TokenTree) -> bool {
    matches!(
        (first, second),
        (TokenTree::Punct(a), TokenTree::Punct(b))
            if a.as_char() == ':' && a.spacing() == Spacing::Joint && b.as_char() == ':'
    )
}

/// The last line of an item, or 0 for an item of a form that is not known.
fn last_line(item: &Item) -> u32 {
    let close = match item {
        Item::Impl(item) => item.brace_token.span.close(),
        Item::Mod(item) => match &item.content {
            Some((brace, _)) => brace.span.close(),
            None => return 0,
        },
        Item::ForeignMod(item) => item.brace_token.span.close(),
        other => return end_of(other, proc_macro2::LineColumn { line: 0, column: 0 }),
    };
    close.end().line as u32
}

/// The line on which an item that has a name ends: the line of its last token. `start` is
/// the place of the name.
fn end_of(item: &Item, start: proc_macro2::LineColumn) -> u32 {
    let last = match item {
        Item::Fn(item) => item.block.brace_token.span.close(),
        Item::Struct(item) => match (&item.semi_token, &item.fields) {
            (Some(semi), _) => semi.span,
            (None, syn::Fields::Named(fields)) => fields.brace_token.span.close(),
            (None, _) => return start.line as u32,
        },
        Item::Enum(item) => item.brace_token.span.close(),
        Item::Union(item) => item.fields.brace_token.span.close(),
        Item::Trait(item) => item.brace_token.span.close(),
        Item::Type(item) => item.semi_token.span,
        Item::Const(item) => item.semi_token.span,
        Item::Static(item) => item.semi_token.span,
        Item::Macro(item) => match &item.semi_token {
            Some(semi) => semi.span,
            None => item.mac.delimiter.span().close(),
        },
        _ => return start.line as u32,
    };
    last.end().line as u32
}

/// The attributes of an expression of a form that code puts under `cfg`.
fn expression_attributes(expression: &syn::Expr) -> &[Attribute] {
    use syn::Expr;
    match expression {
        Expr::Array(node) => &node.attrs,
        Expr::Assign(node) => &node.attrs,
        Expr::Async(node) => &node.attrs,
        Expr::Await(node) => &node.attrs,
        Expr::Binary(node) => &node.attrs,
        Expr::Block(node) => &node.attrs,
        Expr::Call(node) => &node.attrs,
        Expr::Closure(node) => &node.attrs,
        Expr::ForLoop(node) => &node.attrs,
        Expr::If(node) => &node.attrs,
        Expr::Loop(node) => &node.attrs,
        Expr::Macro(node) => &node.attrs,
        Expr::Match(node) => &node.attrs,
        Expr::MethodCall(node) => &node.attrs,
        Expr::Path(node) => &node.attrs,
        Expr::Return(node) => &node.attrs,
        Expr::Struct(node) => &node.attrs,
        Expr::Try(node) => &node.attrs,
        Expr::Tuple(node) => &node.attrs,
        Expr::Unsafe(node) => &node.attrs,
        Expr::While(node) => &node.attrs,
        _ => &[],
    }
}

fn item_attributes(item: &Item) -> &[Attribute] {
    match item {
        Item::Const(item) => &item.attrs,
        Item::Enum(item) => &item.attrs,
        Item::ExternCrate(item) => &item.attrs,
        Item::Fn(item) => &item.attrs,
        Item::ForeignMod(item) => &item.attrs,
        Item::Impl(item) => &item.attrs,
        Item::Macro(item) => &item.attrs,
        Item::Mod(item) => &item.attrs,
        Item::Static(item) => &item.attrs,
        Item::Struct(item) => &item.attrs,
        Item::Trait(item) => &item.attrs,
        Item::TraitAlias(item) => &item.attrs,
        Item::Type(item) => &item.attrs,
        Item::Union(item) => &item.attrs,
        Item::Use(item) => &item.attrs,
        _ => &[],
    }
}

impl<'ast> Visit<'ast> for Extractor<'_> {
    fn visit_item(&mut self, node: &'ast Item) {
        self.with_test(item_attributes(node), |this| {
            let outer = this.item.clone();
            if this.blocks.is_empty() {
                if let Some(path) = this.declare(node) {
                    this.item = Some(path);
                }
            }
            this.attribute_macros(item_attributes(node), last_line(node));
            match node {
                Item::Mod(node) => this.module_item(node),
                Item::Use(node) => this.use_item(node),
                Item::Macro(node) => this.macro_item(node),
                other => visit::visit_item(this, other),
            }
            this.item = outer;
        });
    }

    fn visit_impl_item(&mut self, node: &'ast syn::ImplItem) {
        let attrs: &[Attribute] = match node {
            syn::ImplItem::Const(item) => &item.attrs,
            syn::ImplItem::Fn(item) => &item.attrs,
            syn::ImplItem::Type(item) => &item.attrs,
            syn::ImplItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.with_test(attrs, |this| visit::visit_impl_item(this, node));
    }

    fn visit_trait_item(&mut self, node: &'ast syn::TraitItem) {
        let attrs: &[Attribute] = match node {
            syn::TraitItem::Const(item) => &item.attrs,
            syn::TraitItem::Fn(item) => &item.attrs,
            syn::TraitItem::Type(item) => &item.attrs,
            syn::TraitItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.with_test(attrs, |this| visit::visit_trait_item(this, node));
    }

    fn visit_foreign_item(&mut self, node: &'ast syn::ForeignItem) {
        let attrs: &[Attribute] = match node {
            syn::ForeignItem::Fn(item) => &item.attrs,
            syn::ForeignItem::Static(item) => &item.attrs,
            syn::ForeignItem::Type(item) => &item.attrs,
            syn::ForeignItem::Macro(item) => &item.attrs,
            _ => &[],
        };
        self.with_test(attrs, |this| visit::visit_foreign_item(this, node));
    }

    fn visit_variant(&mut self, node: &'ast syn::Variant) {
        self.with_test(&node.attrs, |this| visit::visit_variant(this, node));
    }

    fn visit_field_value(&mut self, node: &'ast syn::FieldValue) {
        self.with_test(&node.attrs, |this| visit::visit_field_value(this, node));
    }

    fn visit_stmt(&mut self, node: &'ast Stmt) {
        match node {
            Stmt::Macro(call) => {
                self.with_test(&call.attrs, |this| visit::visit_stmt(this, node));
            }
            _ => visit::visit_stmt(self, node),
        }
    }

    fn visit_expr(&mut self, node: &'ast syn::Expr) {
        self.with_test(expression_attributes(node), |this| {
            visit::visit_expr(this, node);
        });
    }

    fn visit_item_extern_crate(&mut self, node: &'ast syn::ItemExternCrate) {
        visit::visit_item_extern_crate(self, node);
        let name = ident(&node.ident);
        if name != "self" {
            let resolved = vec![Resolved {
                path: vec![name],
                exact: true,
                via: None,
            }];
            let span = node.ident.span();
            self.references(span, span, kind::EXTERN_CRATE, resolved);
        }
    }

    fn visit_item_fn(&mut self, node: &'ast syn::ItemFn) {
        self.function(&node.sig, |this| visit::visit_item_fn(this, node));
    }

    fn visit_impl_item_fn(&mut self, node: &'ast syn::ImplItemFn) {
        let inherent = self.impl_of.as_ref().is_some_and(|(_, inherent)| *inherent);
        let visibility = inherent.then(|| items::visibility_of(&node.vis).to_owned());
        let end = node.block.brace_token.span.close().end().line as u32;
        self.method(&node.attrs, &node.sig, end, visibility, |this| {
            this.attribute_macros(&node.attrs, end);
            visit::visit_impl_item_fn(this, node)
        });
    }

    fn visit_trait_item_fn(&mut self, node: &'ast syn::TraitItemFn) {
        // A function of a trait is as visible as the trait.
        let visibility = self.item.as_ref().and_then(|owner| {
            self.declarations
                .iter()
                .rev()
                .find(|declaration| &declaration.path == owner)
                .map(|declaration| declaration.visibility.clone())
        });
        let last = match (&node.default, &node.semi_token) {
            (Some(block), _) => block.brace_token.span.close(),
            (None, Some(semi)) => semi.span,
            (None, None) => node.sig.ident.span(),
        };
        let end = last.end().line as u32;
        self.method(&node.attrs, &node.sig, end, visibility, |this| {
            visit::visit_trait_item_fn(this, node)
        });
    }

    fn visit_foreign_item_fn(&mut self, node: &'ast syn::ForeignItemFn) {
        self.function(&node.sig, |this| visit::visit_foreign_item_fn(this, node));
    }

    fn visit_item_struct(&mut self, node: &'ast syn::ItemStruct) {
        self.with_generics(&node.generics, |this| {
            for attr in &node.attrs {
                this.visit_attribute(attr);
            }
            this.visit_generics(&node.generics);
            match &node.fields {
                syn::Fields::Named(fields) => {
                    let close = fields.brace_token.span.close();
                    this.fields(&fields.named, close, node.ident.span());
                }
                syn::Fields::Unnamed(fields) => {
                    let close = fields.paren_token.span.close();
                    this.fields(&fields.unnamed, close, node.ident.span());
                }
                syn::Fields::Unit => {}
            }
        });
    }

    fn visit_item_enum(&mut self, node: &'ast syn::ItemEnum) {
        self.with_generics(&node.generics, |this| visit::visit_item_enum(this, node));
    }

    fn visit_item_union(&mut self, node: &'ast syn::ItemUnion) {
        self.with_generics(&node.generics, |this| {
            for attr in &node.attrs {
                this.visit_attribute(attr);
            }
            this.visit_generics(&node.generics);
            let close = node.fields.brace_token.span.close();
            this.fields(&node.fields.named, close, node.ident.span());
        });
    }

    fn visit_item_type(&mut self, node: &'ast syn::ItemType) {
        self.with_generics(&node.generics, |this| visit::visit_item_type(this, node));
    }

    fn visit_item_trait_alias(&mut self, node: &'ast syn::ItemTraitAlias) {
        self.with_generics(&node.generics, |this| {
            visit::visit_item_trait_alias(this, node)
        });
    }

    fn visit_impl_item_type(&mut self, node: &'ast syn::ImplItemType) {
        self.with_generics(&node.generics, |this| {
            visit::visit_impl_item_type(this, node)
        });
    }

    fn visit_trait_item_type(&mut self, node: &'ast syn::TraitItemType) {
        self.with_generics(&node.generics, |this| {
            visit::visit_trait_item_type(this, node)
        });
    }

    fn visit_item_trait(&mut self, node: &'ast syn::ItemTrait) {
        if let Some(token) = &node.unsafety {
            self.marker(token.span, marker::UNSAFE_TRAIT);
        }
        self.with_generics(&node.generics, |this| visit::visit_item_trait(this, node));
    }

    fn visit_item_foreign_mod(&mut self, node: &'ast syn::ItemForeignMod) {
        if let Some(token) = &node.unsafety {
            self.marker(token.span, marker::UNSAFE_EXTERN);
        }
        visit::visit_item_foreign_mod(self, node);
    }

    fn visit_item_impl(&mut self, node: &'ast syn::ItemImpl) {
        if let Some(token) = &node.unsafety {
            self.marker(token.span, marker::UNSAFE_IMPL);
        }
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        if let (Some((path, _)), syn::Type::Path(type_path)) = (&node.trait_, &*node.self_ty) {
            if type_path.qself.is_none() {
                let found = Impl {
                    types: self.resolved_paths(&type_path.path),
                    traits: self.resolved_paths(path),
                };
                self.impls.push(found);
            }
        }
        // The code of an `impl` belongs to its type, when the type is a path of an item.
        let self_path = match &*node.self_ty {
            syn::Type::Path(type_path) if type_path.qself.is_none() && self.blocks.is_empty() => {
                let generic = type_path.path.get_ident().is_some_and(|name| {
                    node.generics.params.iter().any(|param| {
                        matches!(param, syn::GenericParam::Type(param) if &param.ident == name)
                    })
                });
                (!generic)
                    .then(|| self.resolved_paths(&type_path.path).into_iter().next())
                    .flatten()
            }
            _ => None,
        };
        let outer_item = self.item.clone();
        let outer_impl = self.impl_of.take();
        let outer_traits = match &node.trait_ {
            Some((path, _)) if self.calls => {
                let traits = self.resolved_paths(path);
                std::mem::replace(&mut self.impl_traits, traits)
            }
            _ => std::mem::take(&mut self.impl_traits),
        };
        if let Some(path) = &self_path {
            self.item = Some(path.clone());
        }
        self.impl_of = self_path.map(|path| (path, node.trait_.is_none()));
        self.with_generics(&node.generics, |this| {
            this.visit_generics(&node.generics);
            if let Some((path, _)) = &node.trait_ {
                this.path(None, path, kind::IMPL);
            }
            this.visit_type(&node.self_ty);
            for item in &node.items {
                this.visit_impl_item(item);
            }
        });
        self.item = outer_item;
        self.impl_of = outer_impl;
        self.impl_traits = outer_traits;
    }

    fn visit_expr_method_call(&mut self, node: &'ast syn::ExprMethodCall) {
        self.site(node.method.span(), kind::METHOD_CALL);
        visit::visit_expr_method_call(self, node);
    }

    fn visit_expr_field(&mut self, node: &'ast syn::ExprField) {
        let span = match &node.member {
            syn::Member::Named(name) => name.span(),
            syn::Member::Unnamed(index) => index.span,
        };
        self.site(span, kind::FIELD);
        visit::visit_expr_field(self, node);
    }

    fn visit_block(&mut self, node: &'ast syn::Block) {
        let mut names = Names::default();
        for statement in &node.stmts {
            if let Stmt::Item(item) = statement {
                load::declare_item(&mut names, item);
            }
        }
        self.blocks.push(names);
        self.with_locals(|this| visit::visit_block(this, node));
        self.blocks.pop();
    }

    /// The initialiser is visited before the pattern: in `let fs = fs::read(..)`, the name
    /// on the right is not yet the new variable.
    fn visit_local(&mut self, node: &'ast syn::Local) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        // The variable is declared also after the statement, so only the code of the
        // statement is under its conditions.
        self.with_test(&node.attrs, |this| {
            if let Some(init) = &node.init {
                this.visit_expr(&init.expr);
                if let Some((_, diverge)) = &init.diverge {
                    this.visit_expr(diverge);
                }
            }
        });
        self.visit_pat(&node.pat);
    }

    fn visit_expr_let(&mut self, node: &'ast syn::ExprLet) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.visit_expr(&node.expr);
        self.visit_pat(&node.pat);
    }

    fn visit_expr_if(&mut self, node: &'ast syn::ExprIf) {
        self.with_locals(|this| visit::visit_expr_if(this, node));
    }

    fn visit_expr_while(&mut self, node: &'ast syn::ExprWhile) {
        self.with_locals(|this| visit::visit_expr_while(this, node));
    }

    fn visit_expr_for_loop(&mut self, node: &'ast syn::ExprForLoop) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.visit_expr(&node.expr);
        self.with_locals(|this| {
            this.visit_pat(&node.pat);
            this.visit_block(&node.body);
        });
    }

    fn visit_expr_closure(&mut self, node: &'ast syn::ExprClosure) {
        self.with_locals(|this| visit::visit_expr_closure(this, node));
    }

    fn visit_arm(&mut self, node: &'ast syn::Arm) {
        self.with_test(&node.attrs, |this| {
            this.with_locals(|this| visit::visit_arm(this, node));
        });
    }

    fn visit_expr_unsafe(&mut self, node: &'ast syn::ExprUnsafe) {
        self.marker(node.unsafe_token.span, marker::UNSAFE_BLOCK);
        visit::visit_expr_unsafe(self, node);
    }

    fn visit_expr_path(&mut self, node: &'ast syn::ExprPath) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        let kind = if self.in_pattern {
            kind::PATTERN
        } else {
            kind::EXPR
        };
        self.path(node.qself.as_ref(), &node.path, kind);
    }

    fn visit_expr_struct(&mut self, node: &'ast syn::ExprStruct) {
        for attr in &node.attrs {
            self.visit_attribute(attr);
        }
        self.path(node.qself.as_ref(), &node.path, kind::EXPR);
        for field in &node.fields {
            self.visit_field_value(field);
        }
        if let Some(rest) = &node.rest {
            self.visit_expr(rest);
        }
    }

    fn visit_type_path(&mut self, node: &'ast syn::TypePath) {
        self.path(node.qself.as_ref(), &node.path, kind::TYPE);
    }

    fn visit_trait_bound(&mut self, node: &'ast syn::TraitBound) {
        self.path(None, &node.path, kind::BOUND);
    }

    fn visit_pat(&mut self, node: &'ast Pat) {
        match node {
            Pat::Ident(pattern) => {
                for attr in &pattern.attrs {
                    self.visit_attribute(attr);
                }
                let name = ident(&pattern.ident);
                // `match x { MAX => .. }` names a constant; `match x { max => .. }` binds a
                // variable. Without types, the capital letter is the only sign.
                let plain = pattern.by_ref.is_none()
                    && pattern.mutability.is_none()
                    && pattern.subpat.is_none();
                if plain && name.starts_with(char::is_uppercase) {
                    let resolved = self.resolver.resolve(
                        self.module,
                        &self.blocks,
                        std::slice::from_ref(&name),
                        false,
                        Single::Drop,
                    );
                    if !resolved.is_empty() {
                        let span = pattern.ident.span();
                        self.references(span, span, kind::PATTERN, resolved);
                        return;
                    }
                }
                self.bind(name);
                if let Some((_, subpattern)) = &pattern.subpat {
                    self.visit_pat(subpattern);
                }
            }
            Pat::Path(pattern) => {
                for attr in &pattern.attrs {
                    self.visit_attribute(attr);
                }
                self.path(pattern.qself.as_ref(), &pattern.path, kind::PATTERN);
            }
            Pat::Struct(pattern) => {
                for attr in &pattern.attrs {
                    self.visit_attribute(attr);
                }
                self.path(pattern.qself.as_ref(), &pattern.path, kind::PATTERN);
                for field in &pattern.fields {
                    self.visit_field_pat(field);
                }
            }
            Pat::TupleStruct(pattern) => {
                for attr in &pattern.attrs {
                    self.visit_attribute(attr);
                }
                self.path(pattern.qself.as_ref(), &pattern.path, kind::PATTERN);
                for element in &pattern.elems {
                    self.visit_pat(element);
                }
            }
            Pat::Range(_) | Pat::Lit(_) => {
                let outer = std::mem::replace(&mut self.in_pattern, true);
                visit::visit_pat(self, node);
                self.in_pattern = outer;
            }
            other => visit::visit_pat(self, other),
        }
    }

    fn visit_macro(&mut self, node: &'ast syn::Macro) {
        self.any_macro(node);
    }

    fn visit_attribute(&mut self, node: &'ast Attribute) {
        self.meta(&node.meta);
    }

    /// `pub(in path)` is not a use of that path.
    fn visit_visibility(&mut self, _: &'ast syn::Visibility) {}

    /// A path that no other method handled. Recorded as a type so that it is not lost.
    fn visit_path(&mut self, node: &'ast syn::Path) {
        self.path(None, node, kind::TYPE);
    }
}
