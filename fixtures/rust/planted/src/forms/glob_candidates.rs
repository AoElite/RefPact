//! Names that come through a glob import of another crate. The syntax does not say what
//! `rand` exports: a load that is syntax only reports them as candidates, and the index of
//! rust-analyzer proves them.

use rand::*;

pub fn roll() -> u32 {
    let mut rng = thread_rng(); //~ expr rand::thread_rng
    let os = rngs::OsRng; //~ expr rand::rngs::OsRng
    rng.next_u32()
}
