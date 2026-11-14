use std::io::Read;

pub struct Pool;

impl Pool {
    pub fn load(&self, path: &str) -> Vec<u8> {
        let mut file = std::fs::File::open(path).unwrap();
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).unwrap();
        bytes
    }
}

macro_rules! read_all {
    ($path:expr) => {
        std::fs::read($path).unwrap()
    };
}

pub fn raw(path: &str) -> Vec<u8> {
    read_all!(path)
}

#[cfg(test)]
mod tests {
    #[test]
    fn loads() {
        assert!(super::Pool.load("missing").is_empty());
    }
}
