pub mod below;

pub fn load() -> bool {
    std::fs::read("renamed").is_ok() //~ expr std::fs::read
}
