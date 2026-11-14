//! The planted fixture: every file below `forms` and `tree` holds violations on purpose.
//!
//! A comment `//~ kind path` at the end of a line says that the line must give exactly that
//! violation. `//~ marker name` is for unsafe code. A `?` after the path means a candidate,
//! and `written=path` gives the path as the code names it when a re-export is followed.
//! `crates/refpact/tests/planted.rs` compares these comments with what the library finds:
//! nothing may be missed, and nothing may be found that has no comment.

#![allow(unused, dead_code, unexpected_cfgs, non_camel_case_types, clippy::all)]

pub mod facade;
pub mod forms;
pub mod codegen;
pub mod rings;
pub mod tree;
