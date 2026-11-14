//! Fixed tables: the std prelude, std macros, primitive types, built-in attributes.

/// The path of a name that the std prelude brings into every module.
pub(crate) fn prelude(name: &str) -> Option<&'static str> {
    Some(match name {
        "Option" => "std::option::Option",
        "Some" => "std::option::Option::Some",
        "None" => "std::option::Option::None",
        "Result" => "std::result::Result",
        "Ok" => "std::result::Result::Ok",
        "Err" => "std::result::Result::Err",
        "Vec" => "std::vec::Vec",
        "String" => "std::string::String",
        "ToString" => "std::string::ToString",
        "Box" => "std::boxed::Box",
        "ToOwned" => "std::borrow::ToOwned",
        "Clone" => "std::clone::Clone",
        "Copy" => "std::marker::Copy",
        "Send" => "std::marker::Send",
        "Sync" => "std::marker::Sync",
        "Sized" => "std::marker::Sized",
        "Unpin" => "std::marker::Unpin",
        "Drop" => "std::ops::Drop",
        "Fn" => "std::ops::Fn",
        "FnMut" => "std::ops::FnMut",
        "FnOnce" => "std::ops::FnOnce",
        "AsyncFn" => "std::ops::AsyncFn",
        "AsyncFnMut" => "std::ops::AsyncFnMut",
        "AsyncFnOnce" => "std::ops::AsyncFnOnce",
        "drop" => "std::mem::drop",
        "AsRef" => "std::convert::AsRef",
        "AsMut" => "std::convert::AsMut",
        "Into" => "std::convert::Into",
        "From" => "std::convert::From",
        "TryFrom" => "std::convert::TryFrom",
        "TryInto" => "std::convert::TryInto",
        "Default" => "std::default::Default",
        "Eq" => "std::cmp::Eq",
        "PartialEq" => "std::cmp::PartialEq",
        "Ord" => "std::cmp::Ord",
        "PartialOrd" => "std::cmp::PartialOrd",
        "Iterator" => "std::iter::Iterator",
        "IntoIterator" => "std::iter::IntoIterator",
        "DoubleEndedIterator" => "std::iter::DoubleEndedIterator",
        "ExactSizeIterator" => "std::iter::ExactSizeIterator",
        "Extend" => "std::iter::Extend",
        "FromIterator" => "std::iter::FromIterator",
        "Future" => "std::future::Future",
        "IntoFuture" => "std::future::IntoFuture",
        // Derive macros that the prelude provides under the name of their trait.
        "Debug" => "std::fmt::Debug",
        "Hash" => "std::hash::Hash",
        _ => return None,
    })
}

/// Macros that std puts in scope everywhere. They are recorded as `std::name!`.
pub(crate) fn is_std_macro(name: &str) -> bool {
    matches!(
        name,
        "assert"
            | "assert_eq"
            | "assert_ne"
            | "cfg"
            | "column"
            | "compile_error"
            | "concat"
            | "dbg"
            | "debug_assert"
            | "debug_assert_eq"
            | "debug_assert_ne"
            | "env"
            | "eprint"
            | "eprintln"
            | "file"
            | "format"
            | "format_args"
            | "include"
            | "include_bytes"
            | "include_str"
            | "is_x86_feature_detected"
            | "line"
            | "matches"
            | "module_path"
            | "option_env"
            | "panic"
            | "print"
            | "println"
            | "stringify"
            | "thread_local"
            | "todo"
            | "unimplemented"
            | "unreachable"
            | "vec"
            | "write"
            | "writeln"
    )
}

pub(crate) fn is_primitive(name: &str) -> bool {
    matches!(
        name,
        "bool"
            | "char"
            | "str"
            | "u8"
            | "u16"
            | "u32"
            | "u64"
            | "u128"
            | "usize"
            | "i8"
            | "i16"
            | "i32"
            | "i64"
            | "i128"
            | "isize"
            | "f16"
            | "f32"
            | "f64"
            | "f128"
    )
}

/// Attributes of the language. Their arguments hold no paths to other code.
pub(crate) fn is_builtin_attribute(name: &str) -> bool {
    matches!(
        name,
        "allow"
            | "automatically_derived"
            | "bench"
            | "cfg"
            | "cold"
            | "collapse_debuginfo"
            | "coverage"
            | "debugger_visualizer"
            | "deny"
            | "deprecated"
            | "doc"
            | "expect"
            | "export_name"
            | "feature"
            | "forbid"
            | "global_allocator"
            | "ignore"
            | "inline"
            | "instruction_set"
            | "link"
            | "link_name"
            | "link_ordinal"
            | "link_section"
            | "macro_export"
            | "macro_use"
            | "must_use"
            | "naked"
            | "no_builtins"
            | "no_implicit_prelude"
            | "no_link"
            | "no_main"
            | "no_mangle"
            | "no_std"
            | "non_exhaustive"
            | "panic_handler"
            | "path"
            | "proc_macro"
            | "proc_macro_attribute"
            | "proc_macro_derive"
            | "recursion_limit"
            | "repr"
            | "should_panic"
            | "target_feature"
            | "test"
            | "track_caller"
            | "type_length_limit"
            | "unused"
            | "used"
            | "warn"
            | "windows_subsystem"
    )
}

/// First segments of tool attributes such as `#[rustfmt::skip]`.
pub(crate) fn is_tool(name: &str) -> bool {
    matches!(
        name,
        "rustfmt" | "clippy" | "rustdoc" | "miri" | "diagnostic" | "rust_analyzer"
    )
}

/// Keywords that cannot start a path in a token stream.
pub(crate) fn is_keyword(name: &str) -> bool {
    matches!(
        name,
        "as" | "async"
            | "await"
            | "become"
            | "break"
            | "const"
            | "continue"
            | "dyn"
            | "else"
            | "enum"
            | "extern"
            | "false"
            | "fn"
            | "for"
            | "gen"
            | "if"
            | "impl"
            | "in"
            | "let"
            | "loop"
            | "match"
            | "mod"
            | "move"
            | "mut"
            | "pub"
            | "ref"
            | "return"
            | "safe"
            | "static"
            | "struct"
            | "trait"
            | "true"
            | "try"
            | "type"
            | "union"
            | "unsafe"
            | "use"
            | "where"
            | "while"
            | "yield"
    )
}

/// The derives of the language. Their code uses the language only.
pub(crate) fn is_builtin_derive(name: &str) -> bool {
    matches!(
        name,
        "Clone" | "Copy" | "Debug" | "Default" | "Eq" | "Hash" | "Ord" | "PartialEq" | "PartialOrd"
    )
}
