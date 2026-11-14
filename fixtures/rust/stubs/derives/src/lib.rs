//! Stand-in for a crate of procedural macros. See `../README.md`.

use proc_macro::{TokenStream, TokenTree};

/// The name of the struct or the enum that the tokens declare.
fn name_of(input: TokenStream) -> String {
    let mut tokens = input.into_iter();
    while let Some(token) = tokens.next() {
        if matches!(token.to_string().as_str(), "struct" | "enum") {
            if let Some(TokenTree::Ident(name)) = tokens.next() {
                return name.to_string();
            }
        }
    }
    panic!("the derive needs a struct or an enum");
}

/// Gives the type a function `load` that reads a file.
#[proc_macro_derive(Stored)]
pub fn stored(input: TokenStream) -> TokenStream {
    let name = name_of(input);
    format!(
        "impl {name} {{
            pub fn load() -> ::std::io::Result<::std::vec::Vec<u8>> {{
                ::std::fs::read(\"stored\")
            }}
        }}"
    )
    .parse()
    .unwrap()
}

/// Wraps a function: the new body prints the name of the function first.
#[proc_macro_attribute]
pub fn logged(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    let text = item.to_string();
    let open = text.find('{').expect("the attribute needs a function with a body");
    format!(
        "{} {{ ::std::println!(\"called\"); ::std::process::id(); {}",
        &text[..open],
        &text[open + 1..]
    )
    .parse()
    .unwrap()
}
