pub fn name() -> String {
    #[cfg(windows)]
    let name = std::env::var("USERNAME");
    #[cfg(not(windows))]
    let name = std::env::var("USER");
    name.unwrap_or_default()
}

#[cfg(all(unix, feature = "tls"))]
pub fn secure() -> std::io::Result<std::net::TcpStream> {
    std::net::TcpStream::connect("localhost:443")
}

#[cfg(test)]
mod tests {
    #[test]
    fn named() {
        let _ = std::time::Instant::now();
    }
}
