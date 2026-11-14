//! Paths whose definition is not where the code names them.

pub mod direct;
pub mod globbed;

/// Declares a variable with the name that it is given.
#[macro_export]
macro_rules! bind {
    ($name:ident) => {
        let $name = 3;
    };
}
