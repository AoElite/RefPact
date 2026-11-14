pub fn load() -> bool {
    std::fs::read("leaf").is_ok() //~ expr std::fs::read
}
