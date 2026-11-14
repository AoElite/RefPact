use crate::{Order, Priced};

impl Order {
    pub fn discount(&self) -> u64 {
        self.total() / 10
    }
}

impl Priced for Order {
    fn price(&self) -> u64 {
        self.total - self.discount()
    }
}

pub fn price_of<T: Priced>(item: &T) -> u64 {
    item.price()
}
