/// The file with the data.
pub fn source() -> std::fs::File {
    std::fs::File::open("data").unwrap()
}
