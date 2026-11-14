//! Stand-in for the `getrandom` crate. See `../README.md`.

#[derive(Debug)]
pub struct Error;

pub fn fill(buffer: &mut [u8]) -> Result<(), Error> {
    buffer.fill(4);
    Ok(())
}
