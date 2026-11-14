//! An invented crate with one exported macro. See `../README.md`.

pub fn write(_text: &str) {}

#[macro_export]
macro_rules! emit {
    ($($argument:tt)*) => {
        $crate::write(stringify!($($argument)*))
    };
}
