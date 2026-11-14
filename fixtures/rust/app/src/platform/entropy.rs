use rand::rngs::OsRng;
use rand::Rng;

pub fn seed() -> u64 {
    let mut bytes = [0u8; 4];
    if getrandom::fill(&mut bytes).is_ok() {
        return u64::from(u32::from_le_bytes(bytes));
    }
    u64::from(OsRng.next_u32()) + rand_core::RngCore::next_u64(&mut rand_core::OsRng)
}
