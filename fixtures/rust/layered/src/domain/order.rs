use crate::storage::disk;

pub struct Order;

impl Order {
    pub fn load() -> Order {
        disk::read();
        Order
    }
}
