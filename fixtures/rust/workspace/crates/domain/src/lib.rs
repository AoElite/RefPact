//! The domain. It uses no other crate of the workspace, except in its tests.

mod money;
pub mod order;

pub use money::Money;
pub use order::Order;

#[cfg(test)]
mod tests {
    use shop_storage::db::Db;

    #[test]
    fn a_database_starts_empty() {
        let _ = Db::default();
    }
}
