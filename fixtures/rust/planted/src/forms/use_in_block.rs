pub fn load() -> bool {
    use std::fs::read; //~ use std::fs::read
    // The name on the right is the import. The variable of the same name starts after it.
    let read = read("x"); //~ expr std::fs::read
    let fs = read.is_ok();
    fs
}
