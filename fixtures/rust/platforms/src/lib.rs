//! Code for several platforms.

#[cfg(unix)]
pub mod nix;
pub mod shared;
#[cfg(windows)]
pub mod win;

cfg_if::cfg_if! {
    if #[cfg(target_os = "macos")] {
        pub fn home() -> std::path::PathBuf {
            std::path::PathBuf::from("/Users")
        }
    } else if #[cfg(unix)] {
        pub fn home() -> std::path::PathBuf {
            std::path::PathBuf::from("/home")
        }
    } else {
        pub fn home() -> std::path::PathBuf {
            std::env::temp_dir()
        }
    }
}
