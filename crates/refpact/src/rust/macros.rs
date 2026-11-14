//! Macro arguments. Macro expansion is not available, so the arguments of a fixed list of
//! macros are parsed as what they are known to be.

use proc_macro2::TokenStream;
use syn::ext::IdentExt;
use syn::parse::{ParseStream, Parser};
use syn::{braced, bracketed, Attribute, Expr, Ident, Item, Pat, Token};

/// Macros whose arguments are a comma-separated list of expressions, where an argument may
/// have the form `name = expression`.
pub const EXPRESSION_LIST_MACROS: &[&str] = &[
    "assert",
    "assert_eq",
    "assert_ne",
    "dbg",
    "debug_assert",
    "debug_assert_eq",
    "debug_assert_ne",
    "eprint",
    "eprintln",
    "format",
    "format_args",
    "panic",
    "print",
    "println",
    "todo",
    "unimplemented",
    "unreachable",
    "write",
    "writeln",
];

/// Macros with their own argument form: `vec![a, b]` and `vec![a; n]`, `matches!(value,
/// pattern if guard)`, and `cfg_if! { if #[cfg(..)] { items } else { items } }`.
///
/// One more form has no name: a macro call in item position whose whole argument is a list
/// of items is read as those items.
pub const OTHER_PARSED_MACROS: &[&str] = &["vec", "matches", "cfg_if"];

pub(crate) enum Arguments {
    Expressions(Vec<Expr>),
    Matches {
        value: Box<Expr>,
        pattern: Box<Pat>,
        guard: Option<Box<Expr>>,
    },
    Items(Vec<Item>),
}

/// Parses the arguments of the macro `name`. `None` when the macro is not in the lists or
/// when its arguments do not have the known form.
pub(crate) fn parse(name: &str, tokens: TokenStream) -> Option<Arguments> {
    match name {
        "vec" => vec_arguments
            .parse2(tokens)
            .ok()
            .map(Arguments::Expressions),
        "matches" => matches_arguments.parse2(tokens).ok(),
        "cfg_if" => cfg_if_items.parse2(tokens).ok().map(Arguments::Items),
        name if EXPRESSION_LIST_MACROS.contains(&name) => expression_list
            .parse2(tokens)
            .ok()
            .map(Arguments::Expressions),
        _ => None,
    }
}

/// The items that a macro call in item position holds: the branches of `cfg_if!`, or, for
/// any other macro, its whole argument when that is a list of items, as in
/// `cfg_unix! { mod sys; }`. Modules declared this way are real modules with real files.
pub(crate) fn items_of(call: &syn::Macro) -> Option<Vec<Item>> {
    let is_cfg_if = call
        .path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "cfg_if");
    if is_cfg_if {
        return cfg_if_items.parse2(call.tokens.clone()).ok();
    }
    // A file is inner attributes and then items, which also covers `feature! { #![unix] mod a; }`.
    let items = syn::parse2::<syn::File>(call.tokens.clone()).ok()?.items;
    (!items.is_empty()).then_some(items)
}

fn expression_list(input: ParseStream) -> syn::Result<Vec<Expr>> {
    let mut expressions = Vec::new();
    while !input.is_empty() {
        let named = input.peek(Ident::peek_any)
            && input.peek2(Token![=])
            && !input.peek2(Token![==])
            && !input.peek2(Token![=>]);
        if named {
            input.call(Ident::parse_any)?;
            input.parse::<Token![=]>()?;
        }
        expressions.push(input.parse()?);
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(expressions)
}

fn vec_arguments(input: ParseStream) -> syn::Result<Vec<Expr>> {
    if input.is_empty() {
        return Ok(Vec::new());
    }
    let first: Expr = input.parse()?;
    if input.peek(Token![;]) {
        input.parse::<Token![;]>()?;
        let length: Expr = input.parse()?;
        return Ok(vec![first, length]);
    }
    let mut expressions = vec![first];
    while !input.is_empty() {
        input.parse::<Token![,]>()?;
        if input.is_empty() {
            break;
        }
        expressions.push(input.parse()?);
    }
    Ok(expressions)
}

fn matches_arguments(input: ParseStream) -> syn::Result<Arguments> {
    let value: Box<Expr> = input.parse()?;
    input.parse::<Token![,]>()?;
    let pattern = Box::new(Pat::parse_multi_with_leading_vert(input)?);
    let guard = if input.peek(Token![if]) {
        input.parse::<Token![if]>()?;
        Some(input.parse()?)
    } else {
        None
    };
    if !input.is_empty() {
        input.parse::<Token![,]>()?;
    }
    Ok(Arguments::Matches {
        value,
        pattern,
        guard,
    })
}

fn cfg_if_items(input: ParseStream) -> syn::Result<Vec<Item>> {
    let mut items = Vec::new();
    // The conditions of the branches before, each as `cfg(..)`.
    let mut earlier: Vec<String> = Vec::new();
    // Reads one branch. Its items are under its condition and under the opposite of each
    // condition before it, so each gets these as `cfg` attributes.
    let mut branch = |input: ParseStream, own: Option<&str>, earlier: &[String]| {
        let mut attributes = String::new();
        if let Some(condition) = own {
            attributes.push_str(&format!("#[{condition}]"));
        }
        for condition in earlier {
            let inner = condition.strip_prefix("cfg").unwrap_or(condition);
            attributes.push_str(&format!("#[cfg(not{inner})]"));
        }
        let body;
        braced!(body in input);
        while !body.is_empty() {
            let mut item: Item = body.parse()?;
            // An attribute cannot be copied, so each item gets its own.
            let more = Attribute::parse_outer.parse_str(&attributes);
            if let (Some(own), Ok(more)) = (attributes_of(&mut item), more) {
                own.extend(more);
            }
            items.push(item);
        }
        Ok::<(), syn::Error>(())
    };
    loop {
        input.parse::<Token![if]>()?;
        input.parse::<Token![#]>()?;
        let condition;
        bracketed!(condition in input);
        let condition = condition.parse::<TokenStream>()?.to_string();
        branch(input, Some(&condition), &earlier)?;
        earlier.push(condition);
        if input.is_empty() {
            break;
        }
        input.parse::<Token![else]>()?;
        if input.peek(Token![if]) {
            continue;
        }
        branch(input, None, &earlier)?;
        break;
    }
    if input.is_empty() {
        Ok(items)
    } else {
        Err(input.error("unexpected tokens after cfg_if branches"))
    }
}

/// The attributes of an item, to add to them. `None` for an item of a form without them.
fn attributes_of(item: &mut Item) -> Option<&mut Vec<Attribute>> {
    Some(match item {
        Item::Const(item) => &mut item.attrs,
        Item::Enum(item) => &mut item.attrs,
        Item::ExternCrate(item) => &mut item.attrs,
        Item::Fn(item) => &mut item.attrs,
        Item::ForeignMod(item) => &mut item.attrs,
        Item::Impl(item) => &mut item.attrs,
        Item::Macro(item) => &mut item.attrs,
        Item::Mod(item) => &mut item.attrs,
        Item::Static(item) => &mut item.attrs,
        Item::Struct(item) => &mut item.attrs,
        Item::Trait(item) => &mut item.attrs,
        Item::TraitAlias(item) => &mut item.attrs,
        Item::Type(item) => &mut item.attrs,
        Item::Union(item) => &mut item.attrs,
        Item::Use(item) => &mut item.attrs,
        _ => return None,
    })
}
