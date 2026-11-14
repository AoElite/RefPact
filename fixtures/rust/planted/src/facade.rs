//! Re-exports. A path through this module still leads to the forbidden module.

pub use crate::codegen::Generator;
pub use std::fs as os;

pub mod deep {
    pub use crate::codegen::*;
}
