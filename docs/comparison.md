# Compared with other tools

[Back to the README](../README.md)

## Rust

| Tool | What refpact covers | What it does not cover |
|---|---|---|
| cargo-pup | Rules that deny paths for some modules, and rules against unsafe code, as described in its documentation. refpact runs on stable Rust as an ordinary test. | Any rule of cargo-pup that is not about dependencies. cargo-pup was not run for this comparison; see its documentation for what it checks. |
| ArchUnitRust (`archunit` 0.0.2) | Dependency rules between modules, at the precision of an item path: `std::fs` is not `std::collections`, and `rand::thread_rng` is not the rest of `rand`. Also cycles between modules, slices and crates, layers (strict, with allowed users, and with every module in a layer), slices that must not use each other, and rules about items: where they live, their names, their visibility, and what they implement or derive. | Rules that read a diagram. Metrics are no built-in rule: a [rule of your own](rust.md#metrics-in-a-check) counts the users of a module and its instability. |
| clippy | The path-shaped uses of `disallowed_methods`, `disallowed_types` and `disallowed_macros`, with a scope for each rule: "outside these modules". | Everything else in clippy. Clippy has type information: it finds `x.elapsed()` and items behind a re-export of another crate. With `resolving_members()`, refpact finds the method call through rust-analyzer, at the path of its definition, and follows the re-exports of other crates through the index of rust-analyzer. Clippy also has the lints for a whole crate, such as `print_stdout` and `todo`. Keep clippy. |
| cargo-deny | Bans by name: with `reading_dependencies()`, a rule says that a crate must not depend on a package, directly or through other packages, for each crate of a workspace. Licences, sources, versions and duplicate versions are no built-in rule: a [rule of your own](rust.md#versions-and-licences) reads them, for each crate and under the filters of a rule. | Advisories. A licence is compared as text, not as an SPDX expression, and a version as text, not as a range. Keep cargo-deny. |

**Chained target selectors in `archunit` 0.0.2.** In this version, a rule with two chained
target selectors passes code that the rule with one selector fails. Observed in November
2026:

- `should_not().depend_on_files().in_path("src/a.rs")` reports the dependency on `a.rs`.
- `...in_path("src/a.rs").in_path("src/b.rs")` reports nothing, in both orders, also when
  the file depends on both. No error is given.
- In `src/files/assertion/depend_on_files.rs`, a target must match every chained filter
  (`filters.iter().all(..)`), and the unit tests of the crate use this AND meaning. The README
  of the crate describes the selectors as OR. Later versions can differ.
- One glob with a character class (`src/[ab].rs`) or one rule for each target does work.

In refpact, several targets are a union, and
[`no_list_weakens_a_rule`](../crates/refpact/tests/planted.rs) keeps it so. The
same test exists for TypeScript and as conformance cases.

`archunit` 0.0.2 resolves to 70 other packages (its own lock file) or 71 (resolved in November
2026). `refpact` adds 3.

## TypeScript

Where an existing tool covers your rules, use it. This table is from the tools' documentation
and from running them in November 2026, with the versions named; newer versions can differ.
"Text" means that the tool compares the specifier as written and does not resolve it.

| | Path must not import path | Package or export, outside a named set | Through barrel files | Kinds of import | A rule that matches nothing | Project resolution |
|---|---|---|---|---|---|---|
| dependency-cruiser 18.7 | yes | whole packages; no rule for one export | any path that leads there (`reachable`), not the name you import | yes (`dependencyTypes`) | passes | `paths` yes; `exports` with an option; not project references |
| ESLint `no-restricted-imports` 10 | text, by `files` blocks | text; `importNames` for exports; `fs` and `node:fs` are two entries | no | static imports and `import type`; not `require` | passes | no |
| eslint-plugin-boundaries 7 | yes | yes for the import statement itself (`specifiers`) | no: the target is the barrel | yes | passes for a policy; flags files that no element matches | through the import resolver you configure |
| import-x `no-restricted-paths` 4 | yes | no | no | all kinds, no switch | passes | with the TypeScript resolver |
| Biome 2.6, oxlint 1.93 `noRestrictedImports` | text | text | no | some; see the options of each tool | passes | no |
| **refpact** | yes | yes, also through a namespace import | yes: to each file that re-exports the name and to the file that declares it | yes | **error** | the compiler's own |

What refpact has in common with dependency-cruiser beyond the table: allow-lists, cycles,
files that nothing imports (`orphan`), chains (`reachable`), packages that `package.json`
does not declare or has only in `devDependencies`, and a baseline of known violations. With
`readDependencies`, a rule also sees what `package.json` names and what those packages need,
so it can forbid a package that no code imports. The licence and the version of each
installed package are there for a [rule of your own](typescript.md#versions-and-licences-in-typescript),
where dependency-cruiser has the rules `license` and `licenseNot`. Its rule about deprecated
packages has no counterpart.

What refpact adds:

- **A barrel file does not hide a dependency.** An import of `query` from `src/db/index.ts`
  is an import of `src/db/client.ts#query`. Of the tools in the table, dependency-cruiser
  follows the path through a barrel, and none follows the imported name.
- **A rule that cannot fail is an error.** In the tools in the table, a rule whose path
  matches nothing passes. eslint-plugin-boundaries reports files that no element matches,
  which is a different check.
- **One export of a package** is found as a named import and as `ns.name` after a namespace
  import.
- **It runs with only `typescript@7` installed.** With TypeScript 7.0.2 and no alias to
  TypeScript 6:
  - dependency-cruiser 18.7.2 prints `missing-tsc-transpiler`, checks 0 modules and exits 0.
    Its 18.8 beta adds support for TypeScript 7.1.
  - typescript-eslint 8.72 does not install (`typescript: ">=4.8.4 <6.1.0"`) and throws when
    forced: "typescript-eslint does not support TS 7.0". Every ESLint rule for `.ts` files
    needs its parser, so the three ESLint rows above need the alias
    `"typescript": "npm:@typescript/typescript6"`.
  - Biome and oxlint have no TypeScript dependency and run.

When the existing tool is the better choice:

- Your project is on TypeScript 5 or older, which refpact does not read. Or your rules compare only
  paths and package names, and you can install the TypeScript 6 alias or wait for
  TypeScript 7.1: use dependency-cruiser or eslint-plugin-boundaries.
  eslint-plugin-boundaries reports in the editor through ESLint. dependency-cruiser reads
  CoffeeScript and the markup of Vue and Svelte files, which refpact does not (refpact reads
  the scripts of `.svelte`, `.vue` and `.astro` files with `components`), and has more output formats, such as
  an HTML report. refpact draws what a rule or a set of layers sees as a Mermaid or DOT diagram (`diagram`),
  reads a JavaScript directory without a config file with default options, and finds
  imports that lead to no file with `mustNotContain("unresolved-import")`. A use of a global
  such as `process.env` or `fetch` is a reference that every rule sees (`globalApi`). Its warnings
  (`asWarning`) are a level of severity, and a rule of your own (`mustSatisfy`) can require
  an import.
- You only need to ban specifiers as text: Biome or oxlint do it, and they do not need
  TypeScript.
- refpact depends on an API that TypeScript calls unstable. A linter rule does not.
