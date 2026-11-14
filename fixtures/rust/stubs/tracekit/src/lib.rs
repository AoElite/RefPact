//! An invented proc-macro crate with one attribute and one derive. See `../README.md`.

use proc_macro::TokenStream;

#[proc_macro_attribute]
pub fn traced(_arguments: TokenStream, item: TokenStream) -> TokenStream {
    item
}

#[proc_macro_derive(Marker, attributes(trace))]
pub fn marker(_item: TokenStream) -> TokenStream {
    TokenStream::new()
}
