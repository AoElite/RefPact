use std::collections::BTreeMap;

use crate::facade::Generator;
use crate::rings::leaf::Leaf;

pub struct Own;

pub struct Strict {
    pub map: BTreeMap<u8, Own>,
    pub leaf: Leaf,
}

pub fn own() -> Own {
    Own
}

pub fn generator() -> Option<Generator> {
    None
}

pub fn text() -> String {
    format!("{}", core::mem::size_of::<Own>())
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_test_uses_more() {
        let _ = std::env::args();
        let _ = super::own();
    }
}
