use shop_domain::{Money, Order};

pub fn total(order: &Order) -> Money {
    order.total
}
