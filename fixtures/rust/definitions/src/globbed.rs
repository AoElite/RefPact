use facade::*;
use std::collections::*;

pub fn build() -> Gear {
    let teeth: HashMap<u32, u32> = HashMap::new();
    Gear(wheel() + teeth.len() as u32)
}

pub fn local() -> u32 {
    crate::bind!(wheel);
    wheel
}

pub fn ordered() -> BTreeMap<u32, u32> {
    BTreeMap::new()
}
