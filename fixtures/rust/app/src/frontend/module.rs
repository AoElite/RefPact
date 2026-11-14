use crate::ir::Function;

pub struct Module {
    pub name: String,
    pub functions: Vec<Function>,
}

impl Module {
    /// Not a violation of rule 3: `env!` is a macro of the compiler. It does not read the
    /// environment of the process, and its path is `std::env!`, not `std::env`.
    pub fn compiler_version() -> &'static str {
        env!("CARGO_PKG_VERSION")
    }
}
