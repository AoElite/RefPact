//! One module for each way in which a module gets its file.

pub mod dir;
pub mod file_with_children;
pub mod plain;

#[path = "odd/renamed_file.rs"]
pub mod renamed;

pub mod inline {
    pub fn load() -> bool {
        std::fs::read("inline").is_ok() //~ expr std::fs::read
    }

    pub mod deeper {
        pub fn load() -> bool {
            std::fs::read("deeper").is_ok() //~ expr std::fs::read
        }
    }

    pub mod from_file;
}

#[cfg_attr(windows, path = "os/windows.rs")]
#[cfg_attr(not(windows), path = "os/unix.rs")]
pub mod os;

cfg_if::cfg_if! {
    if #[cfg(windows)] {
        pub mod in_macro_windows;
    } else {
        pub mod in_macro_other;
    }
}

macro_rules! when_unix {
    ($($item:item)*) => {
        $(#[cfg(unix)] $item)*
    };
}

when_unix! {
    pub mod in_wrapper;
}
