pub mod leaf;

pub fn load() -> bool {
    std::fs::read("dir").is_ok() //~ expr std::fs::read
}
