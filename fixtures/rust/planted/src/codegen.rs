//! The module that the rules forbid.

pub const MAX: usize = 64;
pub const SMALL: usize = 8;

pub type Byte = u8;

pub struct Generator;

impl Generator {
    pub fn new() -> Self {
        Generator
    }
}

pub struct Config {
    pub size: usize,
}

pub struct Wrapper(pub u8);

pub enum Mode {
    Fast,
    Slow,
}

pub trait Backend {
    fn emit(&self) -> u8;
}

pub fn generate() -> u8 {
    0
}

pub fn double(value: u8) -> u8 {
    value * 2
}
