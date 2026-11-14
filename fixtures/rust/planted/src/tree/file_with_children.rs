pub mod child;

pub fn load() -> bool {
    std::fs::read("parent").is_ok() //~ expr std::fs::read
}
