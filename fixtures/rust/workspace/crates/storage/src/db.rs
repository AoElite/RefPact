use std::fs;

use shop_domain::Order;

#[derive(Default)]
pub struct Db {
    orders: Vec<Order>,
}

impl Db {
    pub fn save(&mut self, order: shop_domain::order::Order) {
        let _ = fs::metadata(".");
        self.orders.push(order);
    }
}
