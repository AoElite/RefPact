//! The application. Its crate name and its root file are set in `[lib]`.

pub mod cart;

use shop_storage::db::Db;

pub fn run() -> Db {
    Db::default()
}
