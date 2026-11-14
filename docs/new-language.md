# Adding a language

[Back to the README](../README.md)

refpact has one core, the crate `refpact-core`, which knows no language. To support a
language, you write two parts in it and reuse the core:

| Part | Does | Rust | TypeScript |
|---|---|---|---|
| Front end | reads the code and makes a fact set | `crates/refpact/src/rust` | `packages/refpact/src/frontend` |
| Binding | gives the rules a form for the language's tests, and calls the core | `crates/refpact/src` | `packages/refpact/src` |
| Core | evaluates rules, prints reports, applies baselines | `crates/refpact-core` | the same, as WebAssembly |

Nothing in the core changes for a new language. The words of a language, such as its
reference kinds, markers and visibilities, are text that the front end gives and the binding
names.

## 1. Call the core

Choose how the language reaches the core:

- **From Rust:** depend on `refpact-core` and call `Prepared::new(facts)` and
  `prepared.evaluate(&rule)`.
- **From a runtime with WebAssembly** (Node, browsers, Wasmtime, Wasmer, and the hosts that
  embed them): load the file that `cargo xtask wasm` builds. It imports nothing and exports
  `memory` and four functions. [SPEC section 8](../conformance/SPEC.md#8-the-json-of-the-core)
  describes them, and [`packages/refpact/src/core/wasm.ts`](../packages/refpact/src/core/wasm.ts)
  uses them in about 60 lines.
- **From a language with a C interface** (Python, Go, the JVM, .NET, and others): build
  `crates/refpact-wasm` for the platform instead of WebAssembly. It is a `cdylib`, so the same
  four functions are a native library.

In every case the binding sends JSON: the fact set once, with `load-facts`, then each rule
with the handle that it got. Keep the handle while the fact set lives, and send
`drop-facts` when it goes. The TypeScript binding keeps it in a `WeakMap`, beside a
`FinalizationRegistry` that drops it.

Check the version of the JSON with `version` when the core is loaded. A stale build then
fails at once, with a message that says to build it again.

## 2. Write the front end

The front end reads a project and returns a fact set (SPEC section 3). It decides what a unit
is, how a path is written, and which kinds of reference exist. The core needs:

- **Units.** One path for each unit of code that a rule can select: a module in Rust, a file
  in TypeScript. A path is a list of segments, and `separator` joins them in reports.
- **Facts.** One fact for each reference and each marker, in a stable order, with the file,
  the line and the column. The order of the facts is the order of the violations.
- **Targets.** Resolve each reference to the path that it names, as far as the front end can.
  Give the paths behind a re-export or an alias as `aliases`. When the front end cannot prove
  a path, give it as a `candidate`; never invent a target.
- **Properties.** What the front end knows of a target besides its path, such as the version
  and the licence of a package, goes on the reference as `properties`: names and values of
  your choice. The fact set lists the names, so that a rule that reads a name that no
  reference can have is an error. The core carries the values, and a rule of the user reads
  them, so a new property needs no change of the core.
- **Items.** The paths of the named things inside units, so that a pattern that names an item
  is known to exist.
- **Vocabulary.** All kinds, markers and tags that the front end can produce, also those that
  the project does not use. The core checks every rule against these lists, so a typo in a
  rule is an error and not a rule that cannot fail.
- **Declarations**, for rules about items: the named items of each unit, with their kind,
  their visibility, their relations, such as `implements` or `extends`, and their
  modifiers, such as `async` or `documented`.
- **Lines**, for rules about lengths: the number of lines of each unit, as `unitLines`, and
  the line on which each declaration ends, as `endLine`. A front end that gives neither
  cannot have such rules: the core reports them as `no-subjects`.
- **The path format**, when reports should print paths in the language's own way:
  `attachedPrefixes` and `hiddenPrefixes` (SPEC section 3).
- **Test code.** A tag on facts in test code, so that a rule can leave them out. The Rust
  front end tags them `test`, and `except_in_test_code` ignores that tag.
- **Items of facts**, for rules about what the code of an item uses: the path of the
  innermost declaration around each fact, as `item`. A front end that does not give it
  cannot have such rules: the core reports them as `no-subjects`.

A front end should be written in the language that it reads, with that language's own
parser, as both front ends are: `syn` for Rust and the TypeScript compiler for TypeScript.

## 3. Write the binding

The binding turns the language's way of writing tests into the rule model (SPEC section 4).
Keep the vocabulary of the other bindings, in the language's own naming style, and use the
words of the language where they differ: Rust code *uses* a module, and TypeScript code
*imports* a file.

| Rule model | Rust | TypeScript |
|---|---|---|
| subjects | `rule(name).modules(..)` | `rule(name).files(..)` |
| `must-not-use` | `must_not_use` | `mustNotImport` |
| `must-only-use` | `must_only_use` | `mustOnlyImport` |
| `must-not-contain` | `must_not_contain` | `mustNotContain` |
| `must-be-acyclic` | `must_be_acyclic`, `must_be_acyclic_between` | `mustBeAcyclic`, `mustBeAcyclicBetween` |
| `must-be-independent` | `must_be_independent_between` | `mustBeIndependentBetween` |
| `must-be-used` | `must_be_used` | `mustBeImported` |
| `must-not-reach` | `must_not_reach` | `mustNotReach` |
| `must-be-reachable-from` | `must_be_reachable_from` | `mustBeReachableFrom` |
| `must-only-be-used-by` | `must_only_be_used_by` | `mustOnlyBeImportedBy` |
| `must-not-exceed-lines` | `must_not_exceed_lines` | `mustNotExceedLines` |
| `items` with `not-exceed-lines` | `items(..).must_not_exceed_lines` | `items(..).mustNotExceedLines` |
| `items` | `items(..)` then a filter and a demand | `items(..)` then a filter and a demand |
| `items` with `not-use`, `only-use` | `items(..).must_not_use`, `must_only_use` | — |
| `reason` | `because` | `because` |
| `kinds` | `only_kinds`, `ignoring_kinds` | `ignoringKinds`, `ignoringTypeImports` |
| `ignoreTags` | `except_in_test_code` | — |
| `candidates` | `ignoring_candidates` | — |
| `exemptions` | `ignoring_use` | `ignoringImport` |
| `custom` | `must_satisfy`, `must_meet` | `mustSatisfy` |
| `items` with `custom` | `items(..).must_satisfy`, `must_meet` | `items(..).mustSatisfy` |

On top of single rules, both bindings have layers (`layers`), the onion architecture
(`onion_architecture`), groups of rules (`rules`) and
baselines. These are built from the rule model too: layers are one rule for each layer, and
baselines are applied by the core. Each of them is a rule set: a value that gives rules,
which one check reports together (`RuleSet`). A user can write more. A new binding can add
them later.

A rule of the user (`custom`, [SPEC 4.15](../conformance/SPEC.md#415-checks-of-the-user))
is a function in the language of the binding, so it cannot cross into the core. From Rust,
it is a `Check` in `Constraint::Custom`. Through the JSON, the binding sends the rule with
the request `select`: the core checks the rule, applies the subjects, the filters and the
exemptions, and answers with what the rule sees. The binding then runs the function on that
answer, and renders and baselines the violations with the core, as for every rule. The
TypeScript rules about `package.json` work this way: they read the references that the
`package.json` files give, which are facts like the imports.

A function of the binding cannot match a pattern of the core. The binding sends the lists
of patterns that the function needs with the request, as `targets` and `units`. The core
checks them, and the answer has what each list matches: the path of each reference, and the
units. The demand of a rule about items can be such a function too. The answer then has the
items that the filters of the rule select.

A check fails the test with the text of the core: `render-violations` for violations,
`render-errors` for a rule that cannot be checked. Do not print reports in the binding, so
that they read the same in every language.

## 4. Test it

A language is supported when these pass:

1. **The conformance cases through the binding.** Run every case of `conformance/cases`
   through the binding's call into the core, as
   [`conformance.test.ts`](../packages/refpact/tests/conformance.test.ts) does. Fail when the
   number of cases that ran is not the number in the directory.
2. **A planted fixture.** A small project of the language with one violation of each form
   that the front end detects, marked in a comment, and a test that finds each one and
   nothing else. See `fixtures/rust/planted` and `fixtures/ts`.
3. **A self-check.** The front end and the binding hold their own rules: the binding does not
   parse, the front end does not import the binding, and the parts form no cycle.
4. **Mutations.** Add a mutation to [`xtask/src/mutants/list.rs`](../xtask/src/mutants/list.rs)
   for each detection of the front end, with the test that must fail without it, and a suite
   for the language beside `rust` and `ts`.

The core itself needs no new tests: its behaviour is the specification, and the cases already
cover it.
