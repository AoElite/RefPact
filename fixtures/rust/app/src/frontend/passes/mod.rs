//! Passes over the IR. They must not know any back end.

pub mod dce;
pub mod inline;

use crate::ir::Function;

pub trait Pass {
    fn run(&self, function: &mut Function);
}

pub fn all() -> Vec<Box<dyn Pass>> {
    vec![Box::new(inline::Inline { budget: 8 }), Box::new(dce::Dce)]
}
