pub fn load() -> bool {
    std::fs::read("windows").is_ok() //~ expr std::fs::read
}
