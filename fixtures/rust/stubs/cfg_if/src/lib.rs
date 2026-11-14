//! Stand-in for the `cfg-if` crate: the forms with two and with three branches. See `../README.md`.

#[macro_export]
macro_rules! cfg_if {
    (if #[cfg($first:meta)] { $($yes:item)* } else if #[cfg($second:meta)] { $($other:item)* } else { $($no:item)* }) => {
        $(#[cfg($first)] $yes)*
        $(#[cfg(all(not($first), $second))] $other)*
        $(#[cfg(all(not($first), not($second)))] $no)*
    };
    (if #[cfg($condition:meta)] { $($yes:item)* } else { $($no:item)* }) => {
        $(#[cfg($condition)] $yes)*
        $(#[cfg(not($condition))] $no)*
    };
    (if #[cfg($condition:meta)] { $($yes:item)* }) => {
        $(#[cfg($condition)] $yes)*
    };
}
