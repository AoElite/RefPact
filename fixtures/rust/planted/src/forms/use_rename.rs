use std::fs as files; //~ use std::fs

pub fn load() -> bool {
    files::read("x").is_ok() //~ expr std::fs::read
}
