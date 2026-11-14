# Development

[Back to the README](../README.md)

```sh
cargo test --workspace                          # unit, conformance, fixture and planted tests
cargo clippy --workspace --all-targets -- -D warnings
cargo xtask mutants rust                        # each detection has a test that fails without it (about 40 s)
```

The TypeScript side. The package runs the core of the crate `refpact-core`, built as
WebAssembly into `packages/refpact/wasm`. `pnpm test` and the build make it with
`cargo xtask wasm`, which needs the target once: `rustup target add wasm32-unknown-unknown`.

```sh
pnpm install
cargo xtask wasm                                # the core for the package; run it after a change to the core before `vitest`
node xtask/node/bench-core.mjs                  # times the core from TypeScript on 3000 files and 120000 imports
pnpm test                                       # build, package tests with TypeScript 7.0.2, and the fixture under Vitest and node --test
pnpm typecheck
pnpm --filter @aoelite/refpact-ts test:ts6                  # the package tests with TypeScript 6.0.3
pnpm --filter @aoelite/refpact-ts test:ts71                 # the package tests with TypeScript 7.1.0-beta
cargo xtask mutants ts                          # the same proof for the TypeScript detections (about 25 s)
```

`cargo xtask mutants` with no argument runs both suites side by side. Each
mutation runs only the test that it names, and several copies of the sources run at the same
time (`MUTATION_JOBS`, default a third of the cores). Most Rust mutations are built once
for all, each behind a switch that the test binaries turn on at run time
([`xtask/src/mutants/schemata.rs`](../xtask/src/mutants/schemata.rs)), so the Rust suite
needs one build and not one for each mutation. The TypeScript mutations are switched the same way, and all of them
run in one Vitest run. A second argument keeps the mutations
whose name or file contains it, so after a change to one file you can check only that file:
`cargo xtask mutants ts imports.ts`. Run it when you change how code is read or
how a rule is evaluated; the ordinary tests are enough for other changes.

The tests in `crates/refpact/tests/calls.rs` run rust-analyzer on the fixture
`fixtures/rust/calls`, and fail without it: `rustup component add rust-analyzer`.

The tasks of the repository are Rust, in the crate [`xtask`](../xtask), so working on refpact
needs only Rust and Node. The mutations are listed in
[`xtask/src/mutants/list.rs`](../xtask/src/mutants/list.rs). The `ts` suite also runs Node,
for Vitest and for the TypeScript parser, in the two helpers in [`xtask/node`](../xtask/node).

`pnpm --filter @aoelite/refpact-ts test` runs the conformance cases, the planted fixture, the rule tests,
and the compiler on both fixtures. The conformance cases run three ways: against the core in
Rust, through its JSON in Rust, and from TypeScript through WebAssembly. A test that runs
Vitest by itself uses the module that is in `packages/refpact/wasm`, so build it again after a
change to the core; the loader fails with a clear message when the module is missing or
speaks another version of the JSON.

The fixtures are real crates that compile on stable. `crates/refpact/tests/fixtures_compile.rs`
runs `cargo check` on them. They depend only on the stub crates in `fixtures/rust/stubs`, so
they build without network access.

## Continuous integration

[`.github/workflows/ci.yml`](../.github/workflows/ci.yml) runs on each push to `main` and
on each pull request:

- **Rust:** `cargo fmt --check`, clippy with warnings as errors, and `cargo test --workspace`.
- **TypeScript:** typecheck, `pnpm test` and the package tests with TypeScript 6 and 7.1, on Node
  22, 24 and 26. The job installs the WebAssembly target of Rust to build the core.
- **API references:** `pnpm api-docs`, with rustdoc warnings as errors.
- **Mutation check:** `cargo xtask mutants`, both suites.

## Releases

A release is a tag `vX.Y.Z`, the crates `refpact-core` and `refpact` on crates.io, and the
package `@aoelite/refpact-ts` on npm.
[`.github/workflows/release.yml`](../.github/workflows/release.yml) makes it in one of two
ways:

- **Increment the version automatically.** Run the workflow on `main`:
  ```sh
  gh workflow run release.yml -f bump=patch    # or minor, major; or use the Actions tab
  ```
  It works on the latest `main` and first looks at the version `X.Y.Z` in the files there:
  - **No tag `vX.Y.Z` yet**, as for the first release: it releases `X.Y.Z` as it is and
    pushes the tag.
  - **The tag, but no npm package `X.Y.Z`**, because an earlier run failed after its push:
    it publishes that tag and does not make a new version.
  - **The tag and the npm package:** it increments the version, and pushes the commit
    "Release vX.Y.Z" and the tag to `main` together.

  Each run tests before it pushes or publishes. When a run fails, fix the cause and run it
  again: it continues where the other stopped, and never skips a version. It publishes the
  crates first and the npm package last, and it skips a crate that crates.io already has.
- **Set the version yourself.** Run `cargo xtask bump patch` (or `minor`,
  `major`, or an exact `X.Y.Z`), commit, and push the tag `vX.Y.Z`. The workflow checks that
  the tag matches the files, runs the tests, and publishes.

The version is in `Cargo.toml`, `Cargo.lock` and `packages/refpact/package.json`.
`cargo xtask bump` changes all of them; `cargo xtask bump current` prints it and fails
when they disagree.

A version that is published cannot be changed or published again.

The root `Cargo.toml` keeps `publish = false` and the package keeps `"private": true`, so
`cargo publish` and `npm publish` from a checkout fail. Only the release job removes them,
in its own copy. `crates/refpact-wasm` and `xtask` have their own `publish = false` and are
never published.

A push made with the `GITHUB_TOKEN` of the run does not start other workflows, so CI does
not run on the release commit; the release job tests it instead.

**Before the first release:**

1. Make the repository public. The pages of the packages on crates.io and npm link to the
   documents in it.
2. Add two secrets to the repository: `CARGO_REGISTRY_TOKEN`, a crates.io token that can
   publish new crates, and `NPM_TOKEN`, an npm token that can publish to the scope
   `@aoelite`. npm refuses the name `refpact` without a scope, as too similar to `react`.

## Documentation

Three parts of the documentation are generated, and each is checked by a test or a build:

```sh
pnpm api-docs                                   # both API references
cargo doc --workspace --no-deps --open          # the Rust API reference only, in target/doc
pnpm --filter refpact-api-docs build            # the TypeScript API reference only, in packages/api-docs/out
```

- **API references.** They come from the doc comments. Each public item must have one: the
  crates have `#![warn(missing_docs)]`, which the clippy command above makes an error, and
  TypeDoc stops on an item without a comment. TypeDoc does not run on TypeScript 7, so
  `packages/api-docs` has its own TypeScript 6. It reads the same source.
- **The Rust examples of the README and of `docs/`.** `cargo test` compiles each one. The
  examples are copied to `crates/refpact/tests/readme_examples.md` with the lines that they
  need to compile, and a test fails when that file is not what the documents give. They are
  compiled and not run, because they name the modules of an example project.
- **[`conformance/CASES.md`](../conformance/CASES.md).** It shows each conformance case as text:
  the rule, and the report that the core prints for it. The conformance test fails when
  the file is not up to date.

To write the two generated files again:

```sh
REFPACT_UPDATE_README_EXAMPLES=1 cargo test -p refpact --test readme_examples
REFPACT_UPDATE_CASES=1 cargo test -p refpact --test conformance
```

The TypeScript examples of the documents are not compiled.

Run with Rust 1.99 stable, Node 26.11, pnpm 12.16, TypeScript 7.0.2, 6.0.3 and 7.1.0-beta, and Vitest 5.1.0, on Linux.
