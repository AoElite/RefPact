pub fn registry() -> std::io::Result<std::fs::File> {
    std::fs::File::open("C:/registry")
}
