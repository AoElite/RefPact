pub fn run() {
    let _ = shop_api::order();
}

#[cfg(unix)]
pub fn seed() -> u8 {
    tooling::seed::seed()
}
