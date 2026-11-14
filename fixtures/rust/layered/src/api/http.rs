use crate::domain::order::Order;

pub const ROUTE: &str = "/orders";

pub fn serve() -> Order {
    crate::features::cart::total();
    crate::domain::price::route();
    Order::load()
}
