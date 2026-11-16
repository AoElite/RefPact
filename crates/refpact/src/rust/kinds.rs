//! The kinds of references, the markers and the tag of test code that the front end gives.

pub(crate) mod kind {
    /// A `use` declaration.
    pub(crate) const USE: &str = "use";
    /// A glob import. The path is that of the module.
    pub(crate) const USE_GLOB: &str = "use-glob";
    /// An `extern crate` declaration.
    pub(crate) const EXTERN_CRATE: &str = "extern-crate";
    /// A path in an expression, a struct literal or a call.
    pub(crate) const EXPR: &str = "expr";
    /// A path in a type position.
    pub(crate) const TYPE: &str = "type";
    /// A path in a pattern.
    pub(crate) const PATTERN: &str = "pattern";
    /// A trait in a bound.
    pub(crate) const BOUND: &str = "bound";
    /// The trait of an `impl`.
    pub(crate) const IMPL: &str = "impl";
    /// An attribute or derive path, or a path in the arguments of one.
    pub(crate) const ATTRIBUTE: &str = "attribute";
    /// The macro of a macro call. The path ends in `!`.
    pub(crate) const MACRO: &str = "macro";
    /// A path found in the tokens of a macro that is not parsed.
    pub(crate) const MACRO_TOKENS: &str = "macro-tokens";
    /// A call of a method, by the name of the method. Only a crate that is
    /// loaded with `resolving_members` has this kind.
    pub(crate) const METHOD_CALL: &str = "method-call";
    /// An access to a field, by the name of the field. Only a crate that is
    /// loaded with `resolving_members` has this kind.
    pub(crate) const FIELD: &str = "field";
    /// All kinds that every load gives.
    pub(crate) const ALL: &[&str] = &[
        USE,
        USE_GLOB,
        EXTERN_CRATE,
        EXPR,
        TYPE,
        PATTERN,
        BOUND,
        IMPL,
        ATTRIBUTE,
        MACRO,
        MACRO_TOKENS,
    ];
}

pub(crate) mod marker {
    /// An `unsafe fn`.
    pub(crate) const UNSAFE_FN: &str = "unsafe-fn";
    /// An `unsafe` block.
    pub(crate) const UNSAFE_BLOCK: &str = "unsafe-block";
    /// An `unsafe impl`.
    pub(crate) const UNSAFE_IMPL: &str = "unsafe-impl";
    /// An `unsafe trait`.
    pub(crate) const UNSAFE_TRAIT: &str = "unsafe-trait";
    /// An `unsafe extern` block.
    pub(crate) const UNSAFE_EXTERN: &str = "unsafe-extern";
    /// An attribute written as `#[unsafe(..)]`.
    pub(crate) const UNSAFE_ATTR: &str = "unsafe-attr";
    /// All markers.
    pub(crate) const ALL: &[&str] = &[
        UNSAFE_FN,
        UNSAFE_BLOCK,
        UNSAFE_IMPL,
        UNSAFE_TRAIT,
        UNSAFE_EXTERN,
        UNSAFE_ATTR,
    ];
}

pub(crate) const TAG_TEST: &str = "test";
