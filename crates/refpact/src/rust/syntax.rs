//! What the syntax of a file says before its facts are made: the names that an item declares,
//! the names that a `use` declaration brings in, and whether code is test code. The loader
//! and the extraction of facts both read it.

use proc_macro2::Span;
use syn::ext::IdentExt;
use syn::{Attribute, Item, ItemUse, Meta, UseTree};

use crate::rust::scope::{Import, Name, Names};

/// One file of the module tree. A file that two modules name is here two times.
pub(crate) struct LoadedFile {
    pub path: String,
    pub module: usize,
    /// The whole file is test code.
    pub test: bool,
    /// The `cfg` conditions that the whole file is under, as tags.
    pub cfg: Vec<String>,
}

/// Records what one item adds to the names of its scope. Modules are handled by the caller.
pub(crate) fn declare_item(names: &mut Names, item: &Item) {
    let mut add = |name: &syn::Ident, kind: Name| {
        names.items.insert(ident(name), kind);
    };
    match item {
        Item::Use(item) => {
            for leaf in flatten_use(item) {
                let import = Import::new(
                    leaf.path,
                    item.leading_colon.is_some(),
                    leaf.span.start().line as u32,
                    matches!(item.vis, syn::Visibility::Public(_)),
                );
                if leaf.glob {
                    names.globs.push(import);
                } else if let Some(binding) = leaf.binding {
                    names.imports.entry(binding).or_default().push(import);
                }
            }
        }
        Item::ExternCrate(item) => {
            let real = ident(&item.ident);
            if real != "self" {
                let name = item
                    .rename
                    .as_ref()
                    .map_or(&item.ident, |(_, rename)| rename);
                add(name, Name::ExternCrate(real));
            }
        }
        Item::Enum(item) => {
            let variants = item
                .variants
                .iter()
                .map(|variant| ident(&variant.ident))
                .collect();
            add(&item.ident, Name::Enum(variants));
        }
        Item::Const(item) => add(&item.ident, Name::Item),
        Item::Fn(item) => add(&item.sig.ident, Name::Item),
        Item::Static(item) => add(&item.ident, Name::Item),
        Item::Struct(item) => add(&item.ident, Name::Item),
        Item::Trait(item) => add(&item.ident, Name::Item),
        Item::TraitAlias(item) => add(&item.ident, Name::Item),
        Item::Type(item) => add(&item.ident, Name::Item),
        Item::Union(item) => add(&item.ident, Name::Item),
        Item::ForeignMod(item) => {
            for foreign in &item.items {
                match foreign {
                    syn::ForeignItem::Fn(item) => add(&item.sig.ident, Name::Item),
                    syn::ForeignItem::Static(item) => add(&item.ident, Name::Item),
                    syn::ForeignItem::Type(item) => add(&item.ident, Name::Item),
                    _ => {}
                }
            }
        }
        _ => {}
    }
}

/// An identifier as text, without the `r#` of a raw identifier.
pub(crate) fn ident(ident: &syn::Ident) -> String {
    ident.unraw().to_string()
}

/// One name that a `use` declaration brings in.
pub(crate) struct UseLeaf {
    pub path: Vec<String>,
    /// The name in scope. `None` for a glob and for `as _`.
    pub binding: Option<String>,
    pub glob: bool,
    pub span: Span,
    /// The last name of the path: `span`, or for a glob the name before the `*`.
    pub end: Span,
}

pub(crate) fn flatten_use(item: &ItemUse) -> Vec<UseLeaf> {
    let mut leaves = Vec::new();
    flatten(&item.tree, &mut Vec::new(), None, &mut leaves);
    leaves
}

fn flatten(
    tree: &UseTree,
    prefix: &mut Vec<String>,
    last: Option<Span>,
    leaves: &mut Vec<UseLeaf>,
) {
    let mut leaf = |prefix: &[String], name: &syn::Ident, rename: Option<&syn::Ident>| {
        let mut path = prefix.to_vec();
        // `use a::b::{self}` names `a::b`.
        if name != "self" || path.is_empty() {
            path.push(ident(name));
        }
        let binding = match rename {
            Some(rename) if rename == "_" => None,
            Some(rename) => Some(ident(rename)),
            None => path.last().cloned(),
        };
        leaves.push(UseLeaf {
            path,
            binding,
            glob: false,
            span: name.span(),
            end: name.span(),
        });
    };
    match tree {
        UseTree::Path(path) => {
            prefix.push(ident(&path.ident));
            flatten(&path.tree, prefix, Some(path.ident.span()), leaves);
            prefix.pop();
        }
        UseTree::Name(name) => leaf(prefix, &name.ident, None),
        UseTree::Rename(rename) => leaf(prefix, &rename.ident, Some(&rename.rename)),
        UseTree::Glob(glob) => {
            if !prefix.is_empty() {
                leaves.push(UseLeaf {
                    path: prefix.clone(),
                    binding: None,
                    glob: true,
                    span: glob.star_token.span,
                    end: last.unwrap_or(glob.star_token.span),
                });
            }
        }
        UseTree::Group(group) => {
            for tree in &group.items {
                flatten(tree, prefix, last, leaves);
            }
        }
    }
}

/// The comma-separated metas inside `#[name(...)]`.
pub(crate) fn nested_metas(attr: &Attribute) -> Vec<Meta> {
    attr.parse_args_with(syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated)
        .map(|metas| metas.into_iter().collect())
        .unwrap_or_default()
}

/// True for `#[cfg(test)]`, a `cfg` that holds only with `test`, and test functions.
pub(crate) fn is_test_code(attrs: &[Attribute]) -> bool {
    attrs.iter().any(|attr| {
        let path = attr.path();
        if path.is_ident("cfg") {
            return attr
                .parse_args::<Meta>()
                .is_ok_and(|meta| requires_test(&meta));
        }
        // `#[test]`, `#[bench]`, and test attributes of other crates such as `#[tokio::test]`.
        path.is_ident("bench")
            || path
                .segments
                .last()
                .is_some_and(|segment| segment.ident == "test")
    })
}

fn requires_test(meta: &Meta) -> bool {
    match meta {
        Meta::Path(path) => path.is_ident("test"),
        Meta::List(list) => {
            let members = || {
                list.parse_args_with(
                    syn::punctuated::Punctuated::<Meta, syn::Token![,]>::parse_terminated,
                )
                .map(|metas| metas.into_iter().collect::<Vec<_>>())
                .unwrap_or_default()
            };
            if list.path.is_ident("all") {
                members().iter().any(requires_test)
            } else if list.path.is_ident("any") {
                let members = members();
                !members.is_empty() && members.iter().all(requires_test)
            } else {
                false
            }
        }
        Meta::NameValue(_) => false,
    }
}
