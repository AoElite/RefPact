pub mod pricing;

pub struct Order {
    pub id: u64,
    pub total: u64,
}

impl Order {
    pub fn new(id: u64) -> Order {
        Order { id, total: 0 }
    }

    pub fn total(&self) -> u64 {
        self.total
    }

    pub fn fresh() -> Self {
        Self::new(1)
    }
}

pub trait Priced {
    fn price(&self) -> u64;

    fn doubled(&self) -> u64 {
        self.price() * 2
    }
}
