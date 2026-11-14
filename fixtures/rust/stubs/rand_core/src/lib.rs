//! Stand-in for the `rand_core` crate. See `../README.md`.

pub struct OsRng;

pub trait RngCore {
    fn next_u64(&mut self) -> u64;
}

impl RngCore for OsRng {
    fn next_u64(&mut self) -> u64 {
        4
    }
}
