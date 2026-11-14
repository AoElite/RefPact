# refpact-core

The core of [refpact](https://github.com/AoElite/RefPact), a library for architecture tests.
It knows no language. A front end reads a project and gives the core a fact set, and the core
evaluates rules on it, prints the reports and applies baselines.

To test the architecture of a Rust project, use the crate
[`refpact`](https://github.com/AoElite/RefPact/blob/main/docs/rust.md), which has the front end for Rust and uses this
crate. Use `refpact-core` directly only to support another language:

- [Adding a language](https://github.com/AoElite/RefPact/blob/main/docs/new-language.md)
- [The specification of the core](https://github.com/AoElite/RefPact/blob/main/conformance/SPEC.md)

The crate has no dependencies and builds for any target, WebAssembly included.

## License

MIT
