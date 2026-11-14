pub fn load() -> bool {
    std::fs::read("in_macro_windows").is_ok() //~ expr std::fs::read
}
