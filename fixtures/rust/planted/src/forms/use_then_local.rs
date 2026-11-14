use std::fs; //~ use std::fs

/// The parameter `fs` hides the import. The body has no violation.
pub fn shadowed(fs: &[u8]) -> usize {
    let closure = |fs: usize| fs + 1;
    closure(fs.len())
}

pub fn bound_later() -> bool {
    let found = fs::metadata("x").is_ok(); //~ expr std::fs::metadata
    let fs = found;
    fs
}
