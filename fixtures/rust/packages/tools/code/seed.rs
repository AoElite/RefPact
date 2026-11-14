pub fn seed() -> u8 {
    let mut buffer = [0u8; 1];
    let _ = getrandom::fill(&mut buffer);
    buffer[0]
}
