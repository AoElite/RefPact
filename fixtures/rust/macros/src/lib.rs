//! Code that macros hide from the syntax: refpact parses the arguments of a few known macros
//! only, and rust-analyzer expands all of them.

pub mod api;
pub mod storage;

/// Gives back what it is given. refpact does not parse the arguments of this macro.
macro_rules! traced {
    ($value:expr) => {
        $value
    };
}
pub(crate) use traced;

/// Makes a struct with the given name.
macro_rules! entity {
    ($name:ident) => {
        pub struct $name {
            pub id: u32,
        }
    };
}

entity!(Order);
