# refpact

Architecture tests for **Rust** and **TypeScript**. You write rules about what code may depend
on, and your usual test runner checks them.

```rust
rule("the domain does not use storage")
    .modules(any_of(["crate::domain::**"]))
    .must_not_use(any_of(["crate::storage", "std::fs"]))
    .check(&app);
```

- **Rules are plain values in a test file.** There is no configuration file and no CLI.
- **One rule vocabulary for both languages.** One specification defines how rules are evaluated.
- **A rule that cannot fail is an error.** A misspelled path fails the test. It does not pass.
- **Rust:** runs under `cargo test` on stable Rust. It needs no compiler plugin.
- **TypeScript 6 and 7:** runs under Vitest or `node --test`. The compiler of your project resolves each import. A JavaScript project is read through its `jsconfig.json`, or with default options when it has no config.

## Contents

- [Rust quick start](#rust-quick-start)
- [TypeScript quick start](#typescript-quick-start)
- [What you can check](#what-you-can-check)
- [Common rules](#common-rules)
- [Documentation](#documentation)
- [License](#license)

## Rust quick start

Add the crate as a dev-dependency:

```sh
cargo add --dev refpact
```

Write the rules in a test file, for example `tests/architecture.rs`:

```rust
use std::sync::LazyLock;

use refpact::{all_except, any_of, rule, Crate};

static APP: LazyLock<Crate> = LazyLock::new(|| Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap());

#[test]
fn files_are_opened_in_the_platform_layer_only() {
    rule("files are opened in the platform layer only")
        .modules(all_except(["crate::platform::**"]))
        .must_not_use(any_of(["std::fs"]))
        .except_in_test_code()
        .check(&APP);
}
```

Run `cargo test`. A violation fails the test with one line for each place:

```text
rule `files are opened in the platform layer only` is violated in 2 place(s):
  src/cache.rs:3:5: [files are opened in the platform layer only] crate::cache uses std::fs (use)
  src/cache.rs:17:9: [files are opened in the platform layer only] crate::cache uses std::fs::write (expr, through the `use` at line 3)
```

A pattern is a module path. `crate::platform` is one module. `crate::platform::**` is that
module and all modules below it.

## TypeScript quick start

Add the package as a dev dependency, beside TypeScript 6 or 7 and a test runner:

```sh
pnpm add --save-dev @aoelite/refpact
```

Write the rules in a test file, for example `architecture.test.ts`:

```ts
import { test } from "vitest";
import { anyOf, loadProject, pkg, rule } from "@aoelite/refpact";

const app = loadProject("tsconfig.json");

test("the user interface does not import the database", () => {
  rule("the user interface does not import the database")
    .files(anyOf("src/ui/**"))
    .mustNotImport(anyOf("src/db/**", pkg("pg")))
    .check(app);
});
```

`check` throws an error, and the message of the error is the report:

```text
rule `the user interface does not import the database` is violated in 1 place(s):
  src/ui/view.ts:3:10: [the user interface does not import the database] src/ui/view.ts uses src/db/client.ts#query (import, written as src/db/index.ts#query)
```

The report shows that refpact follows a name through a barrel file to the file that declares it.

## What you can check

| Rule | Rust | TypeScript |
|---|---|---|
| The code does not use some paths | `must_not_use` | `mustNotImport` |
| The code uses only the paths in a list | `must_only_use` | `mustOnlyImport` |
| Layers do not use the layers above them | `layers` | `layers` |
| Each module is in a layer; strict layers; layers that may use some layers only | `layers` options | `layers` options |
| Only some modules may use a module | `must_only_be_used_by` | `mustOnlyBeImportedBy` |
| Allow one known use in a rule | `ignoring_use` | `ignoringImport` |
| Where items live, how they are named, what they implement or derive | `items(..)` | `items(..)` |
| Features do not use each other | `must_be_independent_between` | `mustBeIndependentBetween` |
| No dependency cycles | `must_be_acyclic` | `mustBeAcyclic` |
| No chain of dependencies leads to a path | `must_not_reach` | `mustNotReach` |
| No unused code | `must_be_used` | `mustBeImported` |
| An entry point reaches all code | `must_be_reachable_from` | `mustBeReachableFrom` |
| No module, file, function or type is longer than a limit | `must_not_exceed_lines` | `mustNotExceedLines` |
| Method calls and field accesses count as uses, also inside macros, resolved by rust-analyzer or by the TypeScript compiler | `resolving_members` | `resolveMembers` |
| No `unsafe` code | `must_not_contain` | |
| No global `console`, no `debugger`, no import that leads to no file | | `mustNotContain` |
| Uses of globals such as `process.env`, `fetch` or `window` count as imports | paths such as `std::env::var` | `globalApi("process.env")` |
| A path through a re-export or a glob import of another crate is also a use of the definition | `resolving_members` | through barrel files, by the compiler |
| The code that derives, attribute macros and other macros write | `expanding_macros` | |
| One target or one set of features, or all `cfg` branches at once | `target`, `features`, `except_under_cfg` | |
| A package does not depend on another, directly or through other packages | `reading_dependencies` | `readDependencies` |
| The version and the licence of each package, and two versions of one package | a [rule of your own](docs/rust.md#versions-and-licences), with `reading_properties` | a [rule of your own](docs/typescript.md#versions-and-licences-in-typescript), with `readingProperties` |
| Metrics: how many modules use a module, and its instability | a [rule of your own](docs/rust.md#metrics-in-a-check) | a [rule of your own](docs/typescript.md#metrics-in-a-check) |
| Specifiers that the code computes: `` import(`./locales/${name}.ts`) `` | | candidates, and `mustNotContain("computed-import")` |
| `.svelte`, `.vue` and `.astro` files | | `components` |
| A call through a trait or an interface also counts for each implementation | `following_implementations` | `followImplementations` |
| Fields and methods: visibility, parameter, return and field types | `items(..).of_kind([Field, Method])` | |
| The types that a function takes or returns, or that a field or a variable has | `items(..).returning`, `taking`, `of_type` | `items(..).returning`, `taking`, `ofType` |
| Types that extend or implement a type, at any depth | `items(..).implementing`: a type has an `impl` for each trait | `items(..).assignableTo` |
| Modifiers: `async`, `unsafe`, documented, and more | `items(..).with_modifier`, `must_have_modifier` | `items(..).withModifier`, `mustHaveModifier` |
| What the code of an item may use | `items(..).must_not_use`, `must_only_use` | |
| An onion (hexagonal) architecture | `onion_architecture` | `onionArchitecture` |
| Say why a rule exists, in its report | `because` | `because` |
| Report a rule without failing the check | `as_warning` | `asWarning` |
| Draw what a rule or a set of layers sees as a Mermaid or Graphviz diagram, with its cycles or forbidden uses in red | `diagram` | `diagram` |
| A rule of your own about modules or items, with the subjects, filters, exemptions, reports and baselines of every rule | `must_satisfy`, `must_meet` | `mustSatisfy` |
| A set of rules of your own, checked as layers are | `RuleSet` | `RuleSet` |
| Imported packages are in `package.json` | | `mustOnlyImportDeclaredPackages` |
| Shipped code imports no dev dependency | | `mustNotImportDevDependencies` |

## Common rules

The examples use `app` from the quick starts.

### Keep layers in order

Put the top layer first. A layer must not use a layer above it.

```rust
layers("the layers of the shop")
    .layer("api", any_of(["crate::api::**"]))
    .layer("domain", any_of(["crate::domain::**"]))
    .layer("storage", any_of(["crate::storage::**"]))
    .check(&app);
```

```ts
layers("the layers of the shop")
  .layer("ui", "src/ui/**")
  .layer("domain", "src/domain/**")
  .layer("db", "src/db/**")
  .check(app);
```

### Keep features apart

Each match of the slice pattern is one feature. The rule does not select the shared code, so
all features can use it.

```rust
rule("no feature uses another feature")
    .modules(all_except(["crate::features::shared::**"]))
    .must_be_independent_between(any_of(["crate::features::*"]))
    .check(&app);
```

```ts
rule("no feature imports another feature")
  .files(allExcept("src/features/shared/**"))
  .mustBeIndependentBetween(anyOf("src/features/*"))
  .check(app);
```

### Find dependency cycles

```rust
rule("the parts of the crate form no cycle")
    .modules(all_except(["crate"]))
    .must_be_acyclic_between(any_of(["crate::*"]))
    .check(&app);
```

```ts
rule("no import cycles")
  .files(anyOf("src/**"))
  .mustBeAcyclic()
  .ignoringTypeImports()
  .check(app);
```

### Find indirect dependencies

`must_not_reach` follows the dependencies at any distance. The report shows the shortest
chain, with the file and line of each step.

```rust
rule("the api does not reach the file system")
    .modules(any_of(["crate::api::**"]))
    .must_not_reach(any_of(["std::fs"]))
    .check(&app);
```

```ts
rule("the ui does not reach the file system")
  .files(anyOf("src/ui/**"))
  .mustNotReach(anyOf(pkg("node:fs")))
  .check(app);
```

### Find unused code

Leave out the code that nothing uses by design, such as entry points and tests.

```rust
rule("every module is used")
    .modules(all_except(["crate", "**::tests"]))
    .must_be_used()
    .check(&app);
```

```ts
rule("every file is imported")
  .files(allExcept("src/main.ts", "**/*.test.ts"))
  .mustBeImported()
  .check(app);
```

### Check where items live

Rules about items look at what the code declares: names, kinds, visibility, and what an item
implements, extends or derives.

```rust
rule("repositories live in the storage")
    .items(all())
    .implementing(any_of(["crate::domain::Repository"]))
    .must_reside_in(any_of(["crate::storage::**"]))
    .check(&app);
```

```ts
rule("services are named so")
  .items(anyOf("src/services/**"))
  .ofKind("class")
  .mustBeNamed("*Service")
  .check(app);
```

### Check imports against `package.json`

This rule is for TypeScript only. In Rust, Cargo does this work. The project is loaded
with `readDependencies`, which makes what `package.json` names a reference that every rule
sees.

```ts
const app = loadProject("tsconfig.json", { readDependencies: true });

rule("the source imports only declared packages")
  .files(anyOf("src/**"))
  .mustOnlyImportDeclaredPackages()
  .check(app);
```

### Add a rule to code that is not clean

A baseline is a file of violations that you accept for now. The test fails for each new
violation. It also fails when you fix a violation, until you write the file again. Thus a
baseline can only shrink.

```rust
let baseline = Baseline::file(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/architecture.baseline"));

rule("the domain does not use storage")
    .modules(any_of(["crate::domain::**"]))
    .must_not_use(any_of(["crate::storage"]))
    .check_with_baseline(&app, &baseline);
```

```sh
REFPACT_BASELINE=update cargo test    # writes the current violations to the file
```

TypeScript has the same function: `.checkWithBaseline(app, baseline("architecture.baseline"))`.

## Documentation

| Document | Contents |
|---|---|
| [Rust guide](docs/rust.md) | All rules, patterns, filters, workspaces, baselines, and the limits of the analysis |
| [TypeScript guide](docs/typescript.md) | All rules, targets, monorepos, supported compiler versions, and the limits of the analysis |
| [Comparison](docs/comparison.md) | refpact and cargo-pup, ArchUnitRust, clippy, dependency-cruiser and the ESLint rules |
| [Design](docs/design.md) | The parts of the repository, the dependencies, and how refpact checks itself |
| [Development](docs/development.md) | How to run the tests and build the API references |
| [Adding a language](docs/new-language.md) | The one core, its JSON, and what a front end and a binding of a new language must do |
| [Specification](conformance/SPEC.md) | The exact definition of each rule |
| [Conformance cases](conformance/CASES.md) | Each rule on example input, with the report that it gives |

> [!IMPORTANT]
> refpact reads source code and does not run the Rust compiler. It does not see code that a
> macro produces, and it resolves method calls only when you ask for it, with rust-analyzer. The TypeScript 7 backend uses a
> compiler API that TypeScript calls unstable. Read the limits in the
> [Rust guide](docs/rust.md#limits-of-the-rust-analysis) and the
> [TypeScript guide](docs/typescript.md#limits-of-the-typescript-analysis) before you rely on a rule.

## License

MIT. See [LICENSE](LICENSE).
