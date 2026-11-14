# Design

[Back to the README](../README.md)

## How the parts fit

```text
conformance/SPEC.md        the specification of the core
conformance/cases/*.json   executable cases, language-neutral, run directly, through JSON and from TypeScript
conformance/CASES.md       the cases as text: each rule and the report it gives. Generated.

crates/refpact-core        the core: rule model, matching, evaluation, reports, and its JSON (`wire`)
crates/refpact-wasm        the core as WebAssembly, for every language but Rust
crates/refpact/src/rust    Rust front end: source to facts, with syn
crates/refpact/src         Rust binding: rules as values, checked in tests

packages/refpact/src/core      the types of the core, and calls into its WebAssembly build
packages/refpact/src/frontend  TypeScript front end: project to facts
packages/refpact/src           TypeScript binding
packages/refpact/wasm          the WebAssembly build of the core, made by `cargo xtask wasm`
packages/api-docs              builds the TypeScript API reference with TypeDoc

xtask                      the tasks of the repository: `cargo xtask bump`, `mutants`, `wasm`

fixtures/rust/app          clean fixture, the five rules pass
fixtures/rust/planted      one planted violation for each detection form
fixtures/rust/workspace    three crates, for rules across crates
fixtures/rust/calls        a workspace for method calls and field accesses, which rust-analyzer resolves
fixtures/rust/macros       a crate with calls in macro arguments, and an item that a macro makes
fixtures/rust/expanded     a derive, an attribute macro and a `macro_rules!` whose code only the expansion shows
fixtures/rust/definitions  paths through the re-exports and the glob imports of another crate
fixtures/rust/platforms    code under `cfg`: platforms, a feature and `cfg_if!`
fixtures/rust/packages     a workspace for the dependencies that the manifests name
fixtures/rust/layered      layers, features and dead modules, for the rules about the graph
fixtures/ts                the same for TypeScript
```

The core works on facts: *this unit, at this file and line, refers to this path*, or *this
unit contains this marker*. Beside the facts, a front end lists *declarations*: *this unit
declares this item, of this kind and visibility, tied to these paths, with these modifiers*. A unit is a Rust module or a TypeScript file. The core knows
nothing about the syntax of either language.

### refpact checks itself

The layers above are held in place by tests of this repository:

- [`crates/refpact/tests/own_architecture.rs`](../crates/refpact/tests/own_architecture.rs)
  loads the three crates as one workspace. The core uses only itself and the standard
  library, so it does not name `syn`, the front end or the binding, and it does not use the
  operating system. The front end uses only the core, `syn` and `proc-macro2`. In the front
  end, only `sources` uses `std::fs` and starts rust-analyzer, only `workers` starts threads, and name resolution does
  not use `syn`. The WebAssembly crate uses only the core, and it is the only crate with
  `unsafe` code: it reads and writes the memory that JavaScript hands over.
- [`packages/refpact/tests/architecture.test.ts`](../packages/refpact/tests/architecture.test.ts)
  loads the package. The core imports nothing but the core, except its loader, `wasm.ts`,
  which reads the module file and nothing else. Only the two backends, `native.ts` and `classic.ts`, import a
  compiler package, and only `compiler.ts` imports them. Only the project loader (`project.ts`), the baseline, the
  reader of the dependencies (`dependencies.ts`) and the loader of the core read the file
  system.

Both also use the newer rule forms. The parts of the core form no cycle, and the TypeScript
source has no import cycle when
type imports are left out. The cycle rules found a real cycle on their first run, between the
evaluation code and the cycle search of the core; the code was changed, not the rule.

Both also use layers, chains, and the rules for unused and unreachable code. The binding, the
front end and the core are layers, and no chain leads from the core to the file system. Every module and every source file is used and is reachable from its entry.
The TypeScript source imports only packages that its `package.json` declares.

The built-in rules are checks like the checks of a user. In the core, the modules of the
built-in constraints and demands (`uses`, `cycles`, `reach` and `demands`) may use only the
modules that hold what the crate exports: the `Check` interface, the graphs, the model and
the errors. A rule of the Rust test checks this, so a check of a user can do what a built-in
rule does. The lookups of the index and the checks of patterns stay behind `Setup` and
`Scope`.

Each file ends with a probe that must find the uses that the rules allow, so the rules are
known to see this code.

### One core

There is one core, the crate `refpact-core`, and every language uses it. Rust calls it
directly. Any other language sends it JSON: a fact set once, then rules that name it
(SPEC section 8). The TypeScript package runs it as WebAssembly, which needs no native binary
for each platform.

An earlier version had a second core in TypeScript, kept equal to the Rust one by the
conformance cases. Every change of the rules was then written twice, and every new language
would have needed a third copy. With one core, a new language needs only a front end and a
binding; [adding a language](new-language.md) lists what they must do.

The core knows no language. A front end turns source into a fact set: units, references,
markers and declarations, with the kinds, markers, tags, visibilities, relations and modifiers of the
language as opaque text. A binding turns the rules of the language into the rule model.
Rules and facts are plain data, so the cases in `conformance/cases` state them as JSON and
compare structured results, not text. The tests run every case against the core directly,
through its JSON, and from TypeScript, and each harness fails if the number of cases that it
ran is not the number in the directory.

The core is also faster this way. A `Prepared` fact set makes the lookups that every rule
needs once: the unit of each fact, the owner of each path, and the checks of the facts. Each
rule then costs only what depends on the rule. `node xtask/node/bench-core.mjs` times the core
from TypeScript on a fact set of 3000 files and 120000 imports.

## Dependencies

A crate that uses `refpact` gets three packages besides refpact's own:

| Package | Why |
|---|---|
| `syn` 3, features `full`, `parsing`, `visit`, no default features | Parses Rust source and walks the syntax tree. Without default features it needs neither `quote` nor the proc-macro bridge. |
| `proc-macro2`, feature `span-locations` | Token streams, and the line and column of every token. |
| `unicode-ident` | Required by `proc-macro2`. |

The core uses no dependency, also for its JSON; `own_architecture.rs` checks this. `serde_json` is a
dev-dependency only: it reads the conformance cases in tests and never reaches a consumer.

The TypeScript package has no dependencies besides the `typescript` that the project has. It
ships the core as one WebAssembly file, which needs no generated glue.
TypeDoc is a dev-dependency of `packages/api-docs` only, which is not part of the package.

Patterns are matched by hand-written code, modules are found by following `mod` declarations,
and Cargo says what the manifests mean (`cargo metadata`); `Cargo.toml` is read only for the lines of its dependencies. So there is no glob, regex, directory-walking or TOML crate.
