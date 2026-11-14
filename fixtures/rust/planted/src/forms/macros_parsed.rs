//! One line for each macro whose arguments are parsed. The list is
//! `EXPRESSION_LIST_MACROS` and `OTHER_PARSED_MACROS` in `refpact`.

use std::fmt::Write as _;

pub fn all(out: &mut String, mode: crate::codegen::Mode) -> Vec<bool> { //~ type crate::codegen::Mode
    let a = format!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    let b = std::fmt::format(format_args!("{}", std::env::var("A").is_ok())); //~ expr std::env::var
    print!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    println!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    eprint!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    eprintln!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    let _ = write!(out, "{}", std::env::var("A").is_ok()); //~ expr std::env::var
    let _ = writeln!(out, "{}", std::env::var("A").is_ok()); //~ expr std::env::var
    assert!(std::env::var("A").is_err()); //~ expr std::env::var
    assert_eq!(std::env::var("A").is_ok(), false); //~ expr std::env::var
    assert_ne!(std::env::var("A").is_ok(), true); //~ expr std::env::var
    debug_assert!(std::env::var("A").is_err()); //~ expr std::env::var
    debug_assert_eq!(std::env::var("A").is_ok(), false); //~ expr std::env::var
    debug_assert_ne!(std::env::var("A").is_ok(), true); //~ expr std::env::var
    let c = dbg!(std::env::var("A").is_ok()); //~ expr std::env::var
    let d = vec![std::env::var("A").is_ok()]; //~ expr std::env::var
    let e = vec![false; std::env::args().count()]; //~ expr std::env::args
    let f = matches!(std::env::var("A"), Ok(_)); //~ expr std::env::var
    let g = matches!(mode, crate::codegen::Mode::Fast); //~ pattern crate::codegen::Mode::Fast
    let h = matches!(1, n if n == std::env::args().count()); //~ expr std::env::args
    let named = format!("{value}", value = std::env::var("A").is_ok()); //~ expr std::env::var
    if a.is_empty() {
        panic!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    }
    if b.is_empty() {
        unreachable!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    }
    if c {
        todo!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    }
    if d.is_empty() {
        unimplemented!("{}", std::env::var("A").is_ok()); //~ expr std::env::var
    }
    vec![f, g, h, e.is_empty(), named.is_empty()]
}

cfg_if::cfg_if! {
    if #[cfg(windows)] {
        pub fn home() -> bool {
            std::env::var("USERPROFILE").is_ok() //~ expr std::env::var
        }
    } else {
        pub fn home() -> bool {
            std::env::var("HOME").is_ok() //~ expr std::env::var
        }
    }
}

macro_rules! items {
    ($($item:item)*) => {
        $($item)*
    };
}

// A macro call in item position whose whole argument is a list of items.
items! {
    pub fn wrapped() -> bool {
        std::env::var("W").is_ok() //~ expr std::env::var
    }
}
