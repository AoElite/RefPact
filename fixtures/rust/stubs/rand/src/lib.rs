//! Stand-in for the `rand` crate. See `../README.md`.

pub mod rngs {
    pub struct OsRng;
    pub struct ThreadRng;
    pub struct StdRng(pub u64);

    impl Default for ThreadRng {
        fn default() -> Self {
            ThreadRng
        }
    }
}

pub trait Rng {
    fn next_u32(&mut self) -> u32;
}

pub trait SeedableRng: Sized {
    fn seed_from_u64(seed: u64) -> Self;
}

impl SeedableRng for rngs::StdRng {
    fn seed_from_u64(seed: u64) -> Self {
        rngs::StdRng(seed)
    }
}

impl Rng for rngs::StdRng {
    fn next_u32(&mut self) -> u32 {
        self.0 = self.0.wrapping_mul(6364136223846793005).wrapping_add(1);
        (self.0 >> 32) as u32
    }
}

impl Rng for rngs::ThreadRng {
    fn next_u32(&mut self) -> u32 {
        4
    }
}

impl Rng for rngs::OsRng {
    fn next_u32(&mut self) -> u32 {
        4
    }
}

pub fn thread_rng() -> rngs::ThreadRng {
    rngs::ThreadRng
}

pub fn rng() -> rngs::ThreadRng {
    rngs::ThreadRng
}
