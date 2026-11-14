//! Code that macros write. The source does not show it; the expansion of each macro does.

pub mod model;
pub mod service;

/// Calls a function that the place of the call does not name.
macro_rules! now {
    () => {
        std::time::Instant::now()
    };
}
pub(crate) use now;

/// When the code started.
pub fn started() -> std::time::Instant {
    now!()
}
