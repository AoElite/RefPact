//! A module of the program. It names the library as another crate does.

use shop_storage::db::Db;

pub fn open() -> Db {
    Db::default()
}

pub fn keep(db: &mut Db, order: shop_domain::Order) {
    db.save(order);
}
