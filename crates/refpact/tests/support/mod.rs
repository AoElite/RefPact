//! What several test files use.

/// Removes a temporary file or directory when the test ends, also when it fails.
pub struct Removed(pub std::path::PathBuf);

impl Drop for Removed {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(&self.0);
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
