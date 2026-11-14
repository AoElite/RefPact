//! Stand-in for a crate whose items another crate gives again. See `../README.md`.

pub struct Motor;

impl Motor {
    pub fn start() -> Motor {
        Motor
    }
}

pub mod parts {
    pub struct Gear(pub u32);

    pub fn wheel() -> u32 {
        4
    }
}
