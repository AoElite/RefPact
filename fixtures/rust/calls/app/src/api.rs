use shop_domain::{Order, Priced};
use std::collections::*;

pub fn handle(order: &Order, pool: &crate::storage::Pool) -> u64 {
    let bytes = pool.load("orders");
    let mut seen = HashMap::new();
    seen.insert(order.id, bytes.len());
    let made = Order::fresh();
    println!("{}", made.total());
    order.price() + order.discount() + vec![made.doubled()].len() as u64
}

pub struct Pair(pub u64, pub u64);

pub fn first(pair: &Pair) -> u64 {
    pair.0
}
