pub fn load() -> bool {
    std::fs::read("below").is_ok() //~ expr std::fs::read
}
