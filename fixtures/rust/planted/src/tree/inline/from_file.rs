pub fn load() -> bool {
    std::fs::read("from_file").is_ok() //~ expr std::fs::read
}
