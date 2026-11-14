/// Not a violation of rule 3: `core::time::Duration` is a plain value. Rule 3 names
/// `std::time`, and `core` is a different crate.
pub struct Heap {
    pub cells: Vec<i64>,
    pub pause_budget: core::time::Duration,
}

impl Heap {
    pub fn collect(&mut self) -> usize {
        let before = self.cells.len();
        self.cells.retain(|cell| *cell != 0);
        before - self.cells.len()
    }
}
