//! No violation in this file. Every name here looks like a forbidden path and is a local
//! variable, a field, a type parameter or a module of this crate.

use std::collections::HashMap;

pub fn shadow(env: &[usize], time: usize, codegen: usize) -> usize {
    let fs = env.len();
    let thread = |process: usize| process + fs + time + codegen;
    let net = HashMap::<usize, usize>::new();
    for getrandom in 0..2 {
        let _ = getrandom;
    }
    match Some(3) {
        Some(rand) => rand,
        None => thread(net.len()),
    }
}

pub struct Fields {
    pub fs: u8,
    pub env: u8,
}

pub fn fields(value: Fields) -> u8 {
    let Fields { fs, env } = value;
    fs + env + value.fs
}

pub fn generic<getrandom: Default>() -> getrandom {
    getrandom::default()
}

mod fs {
    pub fn read() -> u8 {
        0
    }
}

pub fn own_module() -> u8 {
    fs::read() + self::fs::read()
}

/// `env!` is the macro `std::env!`, not the module `std::env`.
pub fn compiler_macros() -> &'static str {
    env!("CARGO_PKG_NAME")
}
