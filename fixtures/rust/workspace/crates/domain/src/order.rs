use crate::money::Money;

#[derive(Debug, Default)]
pub struct Order {
    pub total: Money,
}
