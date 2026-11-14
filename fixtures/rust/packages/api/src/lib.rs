pub fn order() -> shop_domain::Order {
    shop_domain::Order
}

/// The manifest names the package `rand` as `random`.
pub fn seed() -> random::rngs::StdRng {
    random::rngs::StdRng(4)
}
