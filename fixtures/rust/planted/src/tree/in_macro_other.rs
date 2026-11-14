pub fn load() -> bool {
    std::fs::read("in_macro_other").is_ok() //~ expr std::fs::read
}
