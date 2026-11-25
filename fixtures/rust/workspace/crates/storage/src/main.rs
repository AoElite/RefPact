//! The program of the package. It is a crate of its own for the compiler; a load reads it
//! with the library, so its module is `shop_storage::cli`.

mod cli;
// The library declares this file too. It is read once.
#[allow(dead_code)]
mod stored;

fn main() {
    let _ = cli::open();
}
