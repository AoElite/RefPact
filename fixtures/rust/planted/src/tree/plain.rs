pub fn load() -> bool {
    std::fs::read("plain").is_ok() //~ expr std::fs::read
}
