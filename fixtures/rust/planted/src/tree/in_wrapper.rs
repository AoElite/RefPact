pub fn load() -> bool {
    std::fs::read("in_wrapper").is_ok() //~ expr std::fs::read
}
