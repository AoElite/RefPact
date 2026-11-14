use std::fs; //~ use std::fs

pub fn load() -> bool {
    fs::read("x").is_ok() //~ expr std::fs::read
}
