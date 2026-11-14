use std::fs;
use std::path::PathBuf;

pub trait Storage {
    fn write(&mut self, key: &str, bytes: &[u8]);
}

pub struct Disk;

impl Storage for Disk {
    fn write(&mut self, key: &str, bytes: &[u8]) {
        let path = PathBuf::from(std::env::temp_dir()).join(key);
        let _ = fs::write(path, bytes);
    }
}
