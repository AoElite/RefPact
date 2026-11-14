pub fn load() -> bool {
    std::fs::read("unix").is_ok() //~ expr std::fs::read
}
