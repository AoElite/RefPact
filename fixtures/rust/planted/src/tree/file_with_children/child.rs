pub fn load() -> bool {
    std::fs::read("child").is_ok() //~ expr std::fs::read
}
