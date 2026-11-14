//! Macros whose arguments are not parsed. Paths are found in their tokens.

use std::fs; //~ use std::fs

macro_rules! pair {
    ($value:expr) => {
        ($value, $crate::codegen::MAX) //~ macro-tokens crate::codegen::MAX
    };
}

macro_rules! name_of {
    ($path:path) => {
        stringify!($path)
    };
}

pub fn call() -> (bool, usize) {
    let first = pair!(std::fs::read("x").is_ok()); //~ macro-tokens std::fs::read
    let second = pair!(fs::metadata("x").is_ok()); //~ macro-tokens std::fs::metadata
    let name = name_of!(fs); //~ macro-tokens std::fs
    let fs_len = name.len();
    pair!(fs_len == 2)
}
