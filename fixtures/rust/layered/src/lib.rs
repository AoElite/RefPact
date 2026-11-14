//! A crate with layers, features, a dead part and a tool that nothing uses.
//! `crates/refpact/tests/graph_rules.rs` names the lines of these files, so add lines at
//! the end.

#![allow(unused, dead_code)]

pub mod api;
pub mod dead;
pub mod domain;
pub mod features;
pub mod storage;
pub mod tools;

pub fn run() {
    api::http::serve();
}
