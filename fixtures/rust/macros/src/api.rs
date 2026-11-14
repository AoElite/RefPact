use std::io::Read;

/// Reads the data, and gives the number of the order.
pub fn read(order: &crate::Order, bytes: &mut Vec<u8>) -> u32 {
    let mut source = crate::storage::source();
    crate::traced!(source.read_to_end(bytes)).unwrap();
    let _all = crate::traced!(bytes[..].len());
    crate::traced!(order.id)
}
