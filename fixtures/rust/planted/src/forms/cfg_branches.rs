//! All branches of `cfg` are checked, also those that this build leaves out.

#[cfg(windows)]
pub fn load() -> bool {
    std::fs::read("a").is_ok() //~ expr std::fs::read
}

#[cfg(not(windows))]
pub fn load() -> bool {
    std::fs::read("b").is_ok() //~ expr std::fs::read
}

#[cfg(feature = "never")]
pub fn never() -> bool {
    std::env::var("c").is_ok() //~ expr std::env::var
}
