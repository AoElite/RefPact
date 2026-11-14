//! The clean fixture: a small compiler-like crate that obeys the five rules of
//! `crates/refpact/tests/five_rules.rs`.
//!
//! It also holds code that looks like a violation and is not one. Each such place has a
//! comment that starts with "Not a violation".

pub mod cache;
pub mod execution;
pub mod ir;
pub mod frontend;
pub mod codegen;
pub mod platform;
pub mod interpreter;
pub mod service;
