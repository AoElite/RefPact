# refpact for TypeScript

[Back to the README](../README.md)

The TypeScript package has the same rule vocabulary. A unit is a source file, and a target is
a file, an export of a file, a package, or an export of a package.

## What TypeScript 7 offers to tools

This is what was found in November 2026, by reading the official announcements and the
contents of the packages, and by running the API.

| Version | Programmatic API |
|---|---|
| `typescript@7.0.2` (`latest`) | No stable API. `import ts from "typescript"` gives only version strings; `createProgram`, `createSourceFile` and `resolveModuleName` do not exist. The package has a client under `typescript/unstable/sync`, `/unstable/async` and `/unstable/ast`. The client starts the native compiler as a child process and asks it for projects, syntax trees and symbols. |
| `typescript@7.1.0-beta` | The client moves to `typescript/sync`, `typescript/async`, `typescript/ast`. The `unstable/*` paths are gone. `createSnapshot` replaces `updateSnapshot`, `getConfiguredProject` replaces `getProject`, the compiler options move to `project.parsedCommandLine.options`, and the numbers of `SyntaxKind` change. |
| `@typescript/typescript6` | The JavaScript API of TypeScript 6, as a package to install beside TypeScript 7. It resolves like TypeScript 6, not like TypeScript 7, so refpact does not use it to read a TypeScript 7 project. |

The 7.0 announcement says: "While TypeScript 7.0 is here, it does not ship with an API" and
"We expect TypeScript 7.1 to ship with a new (and different) API". The only sign of the 7.0
client's status is the word `unstable` in its import paths.

TypeScript 7.0 also removed options that older projects use. `baseUrl` gives
`error TS5102: Option 'baseUrl' has been removed`, and `moduleResolution: node10` and `classic`
give `error TS5108`. `paths` works without `baseUrl`, relative to the tsconfig file.

refpact reads a project with the `typescript` package that the project has installed, because
that is the only way to resolve imports exactly as the project's compiler does. It needs
TypeScript 6 or 7. `compiler.ts` in `packages/refpact/src/frontend` reads the version of the
installed package, loads the backend for it, and fails for any other major version:

- **TypeScript 7:** `native.ts` uses the client of the package, which runs the native
  compiler as a child process.
- **TypeScript 6:** `classic.ts` runs the compiler in the process of the test, through the
  stable API of the package.

Rules, options and reports are the same with both. `compilerVersion` is exported and names
the compiler that was loaded.

**Versions that were run.**

| Version | Command | Status |
|---|---|---|
| `typescript@7.0.2` (`latest`) | `pnpm --filter @aoelite/refpact-ts test` | The main target. All tests pass, and every mutation of `cargo xtask mutants ts` is caught. |
| `typescript@6.0.3` | `pnpm --filter @aoelite/refpact-ts test:ts6` | All tests pass. The mutations are not run with it. |
| `typescript@7.1.0-beta` | `pnpm --filter @aoelite/refpact-ts test:ts71` | All tests pass. Not a target: it is a beta, its final API can differ, and its calls go through three hand-written interfaces that only the tests check. |

The peer dependency is `typescript >=6.0.3 <7.2.0`.

**`baseUrl`.** With TypeScript 7, `loadProject` fails for a tsconfig that sets `baseUrl`. The
native compiler loads such a project but does not resolve with `baseUrl`, so these imports
would be read as packages and rules on the files would pass without a check. With
TypeScript 6, an import that the compiler finds below `baseUrl` leads to its file.

## Quick start

Add the package as a dev dependency, beside TypeScript and a test runner:

```sh
pnpm add --save-dev @aoelite/refpact-ts typescript vitest
```

That is the whole setup. The package needs Node and TypeScript. It does not need a Rust
toolchain: its core is included as WebAssembly.

```ts
// architecture.test.ts
import { test } from "vitest";
import { allExcept, anyOf, loadProject, pkg, rule } from "@aoelite/refpact-ts";

const app = loadProject("tsconfig.json");

test("the user interface does not import the database", () => {
  rule("the user interface does not import the database")
    .files(anyOf("src/ui/**"))
    .mustNotImport(anyOf("src/db/**", pkg("pg")))
    .ignoringTypeImports()
    .check(app);
});

test("the file system is used in the platform layer only", () => {
  rule("the file system is used in the platform layer only")
    .files(allExcept("src/platform/**"))
    .mustNotImport(anyOf(pkg("node:fs"), pkg("node:child_process")))
    .check(app);
});

test("randomness from the system is used in one file only", () => {
  rule("randomness from the system is used in one file only")
    .files(allExcept("src/platform/random.ts"))
    .mustNotImport(anyOf(pkg("node:crypto").export("randomBytes"), pkg("node:crypto").export("randomUUID")))
    .check(app);
});
```

`check` throws an error whose message is the report:

```text
rule `the user interface does not import the database` is violated in 1 place(s):
  src/ui/view.ts:3:10: [the user interface does not import the database] src/ui/view.ts uses src/db/client.ts#query (import, written as src/db/index.ts#query)
```

A rule is a value and `check` only throws, so any test runner works. These were run:
**Vitest 5.1.0** and **the test runner of Node** (`node --test`, Node 26.11), both on Linux,
both in [`fixtures/ts/app`](../fixtures/ts/app). Jest, Bun, Deno, Windows and macOS were not run.

`loadProject` starts the compiler once (about 0.1 s for a small project) and reads everything
it needs. Call it once for each test file, not once for each test.

### Speed of the tests

Most of the time of a test goes to `loadProject`: the compiler reads and resolves the whole
project. The rules are fast after that.

- **Load the project one time.** Call `loadProject` at the top of the test file, as the
  example above does, and use the result in all rules.
- **Keep the architecture rules in one test file.** Vitest and `node --test` run each test
  file in its own process or worker, so each file that calls `loadProject` loads the
  project again.

On a generated project of 10,000 files, `loadProject` takes about 15 seconds and about
0.6 GB of memory with TypeScript 7, and one rule takes about 0.1 seconds. TypeScript 6 was
not measured.

### JavaScript projects

A JavaScript project is read through its `jsconfig.json`, or with default options when it has
no config. TypeScript must still be installed: it reads the JavaScript files and resolves
their imports.

```json
{ "compilerOptions": { "module": "nodenext" }, "include": ["src"] }
```

```ts
const app = loadProject("."); // the directory: its tsconfig.json, its jsconfig.json, or no config
```

- **A directory stands for its config file:** `tsconfig.json`, or `jsconfig.json` when it
  has no tsconfig file.
- **A directory with neither is read with default options:** every file below it except
  `node_modules`, with `allowJs` and the resolution of a bundler (`"module": "preserve"`), so
  `./db` and `./db.js` both find `db.js`, and `require("./db")` does too. refpact writes this
  config to a temporary directory and deletes it after loading; `configFiles` is then empty.
  A directory without JavaScript or TypeScript files is an error. Write a `jsconfig.json` when
  the defaults do not fit, for example to leave out a `dist` directory or to use the rules
  of Node (`"module": "nodenext"`).
- **A jsconfig file reads JavaScript by itself.** A tsconfig file reads it with `allowJs`.
- **ES modules and CommonJS both count:** `import`, `export .. from`, `import()` and
  `require(..)`, in `.js`, `.mjs`, `.cjs` and `.jsx` files.
- **Every rule works,** also the rules about items and the modifiers, which read JSDoc.
  `returning` and `taking` see only the types that a TypeScript file writes; JSDoc types
  such as `@param {Order}` are not read.
- **A project reference needs a config.** A directory that `references` names and that has
  no config is an error, as it is for the compiler.

## Rule vocabulary

```text
rule(name)
    .files(SELECTION)                  which files the rule constrains
    .mustNotImport(TARGETS)            no import of these, in any form
  | .mustOnlyImport(TARGETS)           every import leads to these
  | .mustBeAcyclic()                   the files form no import cycle
  | .mustBeAcyclicBetween(SLICES)      the slices of files form no import cycle
  | .mustBeIndependentBetween(SLICES)  no slice imports another slice
  | .mustBeImported()                  another file imports each file
  | .mustNotReach(TARGETS)             no chain of imports leads to these
  | .mustBeReachableFrom(ROOTS)        a chain of imports leads from the roots to each file
  | .mustOnlyBeImportedBy(IMPORTERS)   only these files may import the files
  | .mustOnlyImportDeclaredPackages()  each imported package is in package.json
  | .mustNotImportDevDependencies()    no import of a package that is only a devDependency
  | .mustNotContain(MARKERS)           no global console, no debugger, no unresolved import
  | .mustNotExceedLines(LINES)         no file has more lines than this
    .ignoringTypeImports()             optional filters
    .onlyKinds(..) | .ignoringKinds(..)   ignoringKinds("global") leaves out the globals
    .ignoringImport(FROM, TO)          one known import that the rule allows
    .because(REASON)                   why the rule exists, printed with its violations
    .check(project)                    throws ArchitectureError with a report
  | .verify(project)                   Failure | undefined
  | .evaluate(project)                 { ok: true, violations } | { ok: false, errors }
  | .diagram(project, "mermaid" | "dot")   the graph that the rule sees, as text
  | .checkWithBaseline(project, baseline("file"))    as check, with accepted violations

rules(name, rule, ..).check(project)   checks all, reports every rule that fails
onionArchitecture(name)                the domain at the centre, adapters outside

rule(name)
    .items(SELECTION)                  the items that these files declare
    .named(..) .ofKind(..) .withVisibility(..)
    .extending(TARGETS) .implementing(TARGETS) .decoratedWith(TARGETS)
    .mustNotExist()                    optional filters, then one demand
  | .mustResideIn(FILES)
  | .mustBeNamed(..)
  | .mustExtend(TARGETS) | .mustImplement(TARGETS) | .mustBeDecoratedWith(TARGETS)
  | .mustNotExceedLines(LINES)         no item has more lines than this
```

`rules` works as in Rust: see [several rules at once](rust.md#several-rules-at-once). One
failure is returned as itself; more are a failure of type `several`.

| Selection of files | Meaning |
|---|---|
| `anyOf("src/ui/**", "src/app/**")` | files that match one of the patterns |
| `all()` | all files of the project |
| `allExcept("src/platform/**")` | all files that match none of the patterns |
| `anyOf("src/**").except("src/gen/**")` | files that match the first list and not the second |

| Target | Meaning |
|---|---|
| `"src/db/**"`, `"src/db/client.ts"` | files of the project, by path from the root |
| `"src/db/client.ts#query"` | one export of a file |
| `pkg("lodash")`, `pkg("@scope/name")` | a package, with all its subpaths and exports |
| `pkg("lodash/fp")` | one subpath of a package |
| `pkg("lodash").export("debounce")` | one export of a package; `export("default")` is the default export |
| `pkg("node:fs")`, `pkg("fs")` | a module of Node, with or without the prefix |
| `pkg("@aws-sdk/*")` | packages by a part of the name |
| `globalApi("process")`, `globalApi("fetch")` | a global of the host, with all its members: see [globals](#globals-in-typescript) |
| `globalApi("process.env")` | one member of a global |
| `globalApi("*Storage")` | globals by a part of the name |
| `unverified("src/generated/**")`, `anyOf(..).andUnverified(..)` | targets that are not checked for existence |

Patterns are paths from the root with `/`. `*` and `**` work as in the Rust patterns. File
patterns match exactly, and targets cover what is inside them: a file covers its exports, and
a package covers its subpaths and exports. The root is the directory of the tsconfig file, or
the `root` option of `loadProject`.

A rule over a package of a monorepo is a rule over its files: `anyOf("packages/ui/**")`.

The checks of [a rule that cannot fail](rust.md#a-rule-that-cannot-fail-is-an-error) are the same.
A file pattern that matches no file is an error, also inside a list and as an exception. A
target file that the project does not have is an error, and so is an export that the file
does not have. Packages cannot be checked for existence.

## Allow-lists, cycles, workspaces and baselines in TypeScript

These work as in Rust: see [allow-lists](rust.md#allow-lists), [cycles](rust.md#cycles) and
[baselines](rust.md#baselines). The differences are here.

```ts
// Every import of the domain leads to the domain or to one package.
rule("the domain imports little")
  .files(anyOf("src/domain/**"))
  .mustOnlyImport(anyOf("src/domain/**", pkg("zod")))
  .check(app);

// No import cycle between files, and none between features.
rule("no cycles").files(anyOf("src/**")).mustBeAcyclic().ignoringTypeImports().check(app);
rule("no cycles between features")
  .files(anyOf("src/features/**"))
  .mustBeAcyclicBetween(anyOf("src/features/*"))
  .check(app);

// A rule that is turned on before the code is clean.
const accepted = baseline(path.join(import.meta.dirname, "architecture.baseline"));
rule("the ui does not import the database")
  .files(anyOf("src/ui/**"))
  .mustNotImport(anyOf("src/db/**"))
  .checkWithBaseline(app, accepted);
```

- **Allow-lists.** `pkg("*")` is every package and `pkg("node:*")` is every module of Node.
  An import through a barrel file is allowed when the barrel is allowed, or when the file
  that declares the name is.
- **Cycles.** An edge is an import of a file of the project, in any kind: also
  `export .. from`, `require` and `import()`. A cycle that exists through type imports only
  does no harm at run time; `ignoringTypeImports()` leaves those edges out. An import through
  a barrel file gives two edges: one to the barrel, and one to the file that declares the
  name, also when the rule does not select the barrel.
- **Workspaces.** Give `loadProject` the tsconfig file of each package and the root of the
  workspace: `loadProject(["packages/a/tsconfig.json", "packages/b/tsconfig.json"], { root: "." })`.
  The files of all packages are then units. An import of a package of the workspace that
  resolves to a source file under the root is an import of that file, so a rule can name the
  package (`pkg("@shop/core")`) or its files (`"packages/core/**"`). With
  `mustBeAcyclicBetween(anyOf("packages/*"))`, each package is a slice. The list of packages
  is not read from `pnpm-workspace.yaml` or `package.json`; the test names the tsconfig files.
- **Baselines.** `REFPACT_BASELINE=update` works as in Rust, and `rule.writeBaseline(project,
  baseline)` writes without it. Test files that run in separate processes must not share one
  baseline file while it is written: use one file for each test file.

## Layers, slices, unused files and chains in TypeScript

These work as in Rust: see [layers](rust.md#layers), [independent slices](rust.md#independent-slices),
[unused and unreachable modules](rust.md#unused-and-unreachable-modules) and [chains](rust.md#chains).

```ts
layers("the layers of the shop")
  .layer("ui", "src/ui/**")              // the top
  .layer("domain", "src/domain/**")
  .layer("db", "src/db/**")              // the bottom
  .check(app);

rule("no feature imports another feature")
  .files(allExcept("src/features/shared/**"))
  .mustBeIndependentBetween(anyOf("src/features/*"))
  .check(app);

rule("every file is imported").files(allExcept("src/main.ts", "**/*.test.ts")).mustBeImported().check(app);
rule("every file is reachable").files(anyOf("src/**")).mustBeReachableFrom(anyOf("src/main.ts")).check(app);

rule("the ui does not reach the file system")
  .files(anyOf("src/ui/**"))
  .mustNotReach(anyOf(pkg("node:fs")))
  .check(app);
```

- **Layers.** A layer takes one or more file patterns. Layers and an onion architecture have
  every filter of a rule: `ignoringTypeImports()`, `onlyKinds(..)`, `ignoringKinds(..)`,
  `ignoringCandidates()`, `ignoringImport(..)`, `because(..)` and `asWarning()`. A filter
  applies to all layers and to their diagram, and `rules()` gives the rules as values. The
  options are `strict()`,
  `mayOnlyImport(layer, ...layers)`, `mayOnlyBeImportedBy(layer, ...layers)` and
  `everyFileInALayer(selection)`, as `strict`, `may_only_use`, `may_only_be_used_by` and
  `every_module_in_a_layer` in Rust. A restricted layer has the rule
  `<layers>: <layer> imports only the layers it may import`.
- **Exemptions.** `ignoringImport(anyOf("src/config.ts"), anyOf(pkg("node:fs")))` lets the
  imports of `node:fs` in `src/config.ts` pass, as
  [`ignoring_use`](rust.md#exemptions) does in Rust. An exemption that lets no import pass is
  an error.
- **Only some files may import a file.** `rule("only the domain imports the db")
  .files(anyOf("src/db/**")).mustOnlyBeImportedBy(anyOf("src/domain/**"))` works as
  [`must_only_be_used_by`](rust.md#only-some-modules-may-use-a-module). An import through a
  barrel file counts as an import of the file that declares the name.
- **Unused files.** A file is a unit of its own: a directory is not used through the files in
  it. Leave out the entry points and the test files, which nothing imports. An import that
  only has a side effect (`import "./setup"`) is an import.
- **Chains.** With `ignoringTypeImports()`, a chain is one that exists at run time.

## Rules about items in TypeScript

They work as [rules about items](rust.md#rules-about-items) in Rust, with the items of
TypeScript:

```ts
rule("repositories live in the db").items(all()).implementing(anyOf("src/domain/port.ts#Repository")).mustResideIn(anyOf("src/db/**")).check(app);
rule("services are named so").items(anyOf("src/services/**")).ofKind("class").mustBeNamed("*Service").check(app);
rule("nothing in internal is exported").items(anyOf("src/internal/**")).withVisibility("exported").mustNotExist().check(app);
rule("no emitters in the domain").items(anyOf("src/domain/**")).extending(anyOf(pkg("node:events"))).mustNotExist().check(app);
```

- The items are the top-level declarations of a file: `function`, `class`, `interface`,
  `type`, `enum`, `variable` (each name of a `const`, `let` or `var`) and `namespace`. A
  `default` function or class without a name is named `default`.
- The visibility is `exported` when the file exports the item, also under another name
  (`export { a as b }`), and `local` otherwise.
- `extending` and `implementing` read the `extends` and `implements` clauses of classes and
  interfaces, and `decoratedWith` the decorators of a class. A target is the declaration of
  the name, as in `mustNotImport`: `src/domain/port.ts#Repository`, or `pkg("node:events")`
  for a class of a package.
- **The members of a class are not items.** [Fields and methods](rust.md#fields-and-methods)
  are items in Rust only: the TypeScript front end reads the top-level declarations.
- **No rule about what the code of an item imports.** In Rust,
  [`items(..).must_not_use`](rust.md#what-the-code-of-an-item-uses) is about the references
  inside the selected items. The TypeScript front end does not record the item that a
  reference is in, so a rule about imports is about files.

### Types of items in TypeScript

```ts
rule("entities live in the domain").items(all()).assignableTo(anyOf("src/domain/entity.ts#Entity")).mustResideIn(anyOf("src/domain/**")).check(app);
rule("the ui takes no rows").items(anyOf("src/ui/**")).taking(anyOf("src/db/**")).mustNotExist().check(app);
rule("only the db returns rows").items(allExcept("src/db/**")).returning(anyOf("src/db/row.ts#Row")).mustNotExist().check(app);
rule("no pools outside the db").items(allExcept("src/db/**")).ofType(anyOf("src/db/pool.ts#Pool")).mustNotExist().check(app);
```

- **`assignableTo` and `mustBeAssignableTo`** follow `extends` and `implements` to any
  depth through the declarations of the project. When `Order extends Entity` and `Entity
  implements Saved`, `Order` is assignable to `Entity` and to `Saved`. An item is not
  assignable to itself.
- **The chain stops at a package.** A class that extends `pkg("node:events")` is assignable
  to `node:events#EventEmitter`, but not to the types that `EventEmitter` extends.
- **It is not the structural check of the compiler.** A class that has all the members of
  `Saved` but does not name it in `implements` is not assignable to `Saved` here.
- **`returning`, `taking` and `ofType`** see each type that is named in the written type:
  `(order: Order, pool: db.Pool): Promise<Total>` takes `Order` and `db.Pool` and returns
  `Total`. `typeof value` and `import("./x").Y` name types too.
- **What counts as a function:** a `function`, and a variable whose value is an arrow
  function or a function expression. `ofType` reads the written type of a variable.
- **What is no type here:** an inferred type, a type parameter, `this`, the global types
  such as `Promise` and `Map`, and the keywords such as `string`.
- **A name has all its paths,** as for `extends`. A type imported through a barrel file is
  at the barrel and at the file that declares it.

### Modifiers in TypeScript

`withModifier` selects items by what their declaration says besides the kind and the
visibility, and `mustHaveModifier` demands one of the modifiers.

```ts
rule("no default exports").items(all()).withModifier("default").mustNotExist().check(app);
rule("the exports are documented").items(anyOf("src/**")).withVisibility("exported").mustHaveModifier("documented").check(app);
rule("no async in the domain").items(anyOf("src/domain/**")).withModifier("async").mustNotExist().check(app);
```

| Modifier | The item |
|---|---|
| `async` | an `async function`, or a variable that holds an `async` arrow function or function expression |
| `abstract` | an `abstract class` |
| `declare` | a declaration with `declare`, such as `declare function f(): void;` |
| `const` | a `const` variable, or a `const enum` |
| `default` | an `export default` function or class. `export default name;` gives the item no modifier. |
| `documented` | has a JSDoc comment (`/** .. */`) before it. A `//` comment does not count. |

- A statement with several names, as `/** .. */ export const a = 1, b = 2;`, gives each name
  its modifiers.
- The members of a class are not items, so they have no modifiers here.

## Component files: Svelte, Vue and Astro

`.tsx` and `.jsx` files are TypeScript and JavaScript, and are read like any other file, so
the rules hold for React components. An import is a reference, and so is a name that the JSX
uses, as `lib.a` in `<Button label={lib.a} />`. A
`.svelte`, `.vue` or `.astro` file is read with the option `components`:

```ts
const app = loadProject("tsconfig.json", { components: true });

rule("a component does not call the api")
  .files(anyOf("src/**/*.svelte", "src/**/*.vue"))
  .mustNotImport(anyOf("src/api/**"))
  .check(app);
```

```text
src/App.svelte:2:12: [a component does not call the api] src/App.svelte uses src/api/client.ts#load (import)
```

- **A component file is a file of the project under its own name,** `src/App.svelte`. Every
  rule sees it: cycles, layers, chains, unused files and your own checks.
- **Its code is what its `<script>` blocks hold,** all of them, and for Astro also the part
  between the `---` lines at the start. The lines and the columns are those of the
  component file.
- **An import of `./App.svelte` leads to the component,** from TypeScript files and from
  other components. Without the option, such an import is a path that no file has.
- **The markup is not read.** A component that the markup uses is imported in a script, so
  no import is missed. A global or a member that only the markup uses, as in
  `on:click={() => localStorage.clear()}`, is not seen.
- **`components: [".svelte"]`** names the extensions to read.
- **A file that is on the disk as `name.svelte.ts`,** such as a module of Svelte 5, is read as
  the TypeScript file that it is; a `name.svelte` beside it is then not read.
- **A script with `src="..."`** and a script of a type that is not code, such as JSON, are
  left out.
- **The load is slower:** the compiler asks refpact for each file that it reads. No package
  of Svelte, Vue or Astro is needed.

## Computed specifiers

`import(..)`, `require(..)`, `require.resolve(..)`, `import.meta.resolve(..)` and
`new URL(.., import.meta.url)` can take a specifier that is not one string. refpact reads what
the file says of it:

| The code | Gives |
|---|---|
| a template, strings joined with `+`, both sides of `? :`, and a `const` of the same file with such a value: `` import(`${DIRECTORY}/en.ts`) `` | the import of each string that it can be, as if the string were written there |
| a part that is not known: `` import(`./locales/${name}.ts`) `` | the marker `computed-import`, and one **candidate** reference to each file of the project that the specifier can be (`./locales/*.ts`). For a package whose name is known, `` import(`plugins/${name}`) ``, one candidate reference to the package. |
| nothing that is known: `import(name)` | the marker `computed-import` |

```ts
rule("every import is written out").files(all()).mustNotContain("computed-import").check(app);
rule("only the i18n code loads the locales").files(allExcept("src/i18n/**")).mustNotImport(anyOf("src/locales/**")).check(app);
```

- A candidate is a violation by default. It is marked `candidate` in the report, and
  `ignoringCandidates()` leaves it out of a rule.
- A part that is not known can be any text, also with a `/`. The specifier can leave out the
  extension or name a directory with an index file.
- A `const` counts only when the file declares its name one time and the compiler says that
  the name in the specifier is that variable. A parameter of the same name is not known.
- The file of `new URL(..)` need not be a module: `new URL("./logo.png", import.meta.url)` is a
  reference to `src/logo.png`, and it is never an `unresolved-import`.
- A computed specifier that is a `paths` alias or a `#name` is not resolved.
- `import.meta.glob(..)` of Vite takes a glob pattern, or a list of them. Each file that it
  matches is an exact `dynamic-import`.

## Globals in TypeScript

Many rules are about globals, not imports: only the config reads `process.env`, only the api
client calls `fetch`, the domain does not touch `window`. A use of a global is a reference of
the kind `global`, so every rule about imports sees it. A target names it with `globalApi`.

```ts
rule("only the config reads the environment")
  .files(allExcept("src/config.ts"))
  .mustNotImport(anyOf(globalApi("process.env")))
  .check(app);

rule("only the api client talks to the network")
  .files(allExcept("src/api/**"))
  .mustNotImport(anyOf(globalApi("fetch"), globalApi("XMLHttpRequest"), globalApi("WebSocket")))
  .check(app);
```

```text
rule `only the config reads the environment` is violated in 1 place(s):
  src/jobs/mail.ts:4:15: [only the config reads the environment] src/jobs/mail.ts uses global:process#env (global)
```

- **What a global is:** a name that the compiler declares outside the project and outside the
  ECMAScript library. So the globals of Node (`process`, `Buffer`, `__dirname`), of the
  browser (`window`, `document`, `fetch`, `localStorage`) and of other hosts, and the globals
  that a package declares, such as `describe` of a test runner. `Math`, `Date`, `JSON`,
  `Promise` and `Error` are part of the language and are no globals here, so `Date.now()` and
  `Math.random()` are not seen. A global that the project declares itself, with
  `declare global`, is no global either.
- **The compiler must know the global.** The globals of Node need `@types/node`, and those of
  the browser need `"lib": ["dom"]` (the default when `lib` is not set). A name that the
  compiler does not know is not a reference.
- **The target has the member that is used,** `global:process#env` for `process.env.PORT`, and
  `globalApi("process")` covers all members. `window.x`, `globalThis.x`, `self.x` and
  `global.x` are the global `x`: `window.localStorage.getItem(..)` is
  `global:localStorage#getItem`.
- **Only values count.** A type such as `let body: Response` is no use; `new Response()` is.
  A local name that hides a global, such as a parameter `process`, is no use of the global.
- **Allow-lists see the globals.** `mustOnlyImport` reports a global that its list does not
  name. Add the globals to the list, or leave them out with `.ignoringKinds("global")`.
  `console` is a global too, and it stays a [marker](#markers-reasons-and-diagrams-in-typescript) as well.
- **The cost** is one more pass over the syntax of each file; the compiler is asked only about
  names that no scope around them declares.

## Members in TypeScript

An import says which names a file takes from another file. It does not say which members of
a class the file then uses: what `order.add(1)` calls depends on the type of `order`. With
`resolveMembers`, refpact asks the compiler, and each use of a member is a reference.

```ts
const app = loadProject("tsconfig.json", { resolveMembers: true });

rule("the ui does not change orders")
  .files(anyOf("src/ui/**"))
  .mustNotImport(anyOf("src/domain/order.ts#Order#add"))
  .check(app);
```

```text
src/ui/view.ts:5:9: [the ui does not change orders] src/ui/view.ts uses src/domain/order.ts#Order#add (member)
```

- **A member is at `file.ts#Owner#name`:** the file that declares the class, the interface or
  the enum, its name, and the name of the member. A target `src/domain/order.ts#Order`
  covers all members of `Order`, and `src/domain/order.ts` all of the file.
- **Properties, methods, accessors and enum members count,** also static ones. All have the
  kind `member`: `.onlyKinds("member")` keeps them, and `.ignoringKinds("member")` leaves
  them out.
- **An inherited member is a member of the type that declares it.** When `Order extends
  Entity` and `save` is a method of `Entity`, `order.save()` is a use of
  `src/domain/entity.ts#Entity#save`. So a file can use a file that it does not import.
- **Every rule about imports sees the members:** `mustNotImport`, the cycles, the layers,
  `mustBeImported` and your own checks. A rule that names the kind for a project that was
  loaded without the option is an error.
- **Only members of the files of the project count.** A member of a package or of the
  standard library (`items.length`, `emitter.on`) is no reference.
- **`followImplementations` follows a member into the classes below its type:**
  `loadProject("tsconfig.json", { followImplementations: true })`, which turns on
  `resolveMembers`. `store.save(order)`, where `store` is a `Store`, is then also a
  **candidate** use of `src/infra/pg.ts#PgStore#save`, for each class of another file that
  implements or extends `Store`, directly or through other classes, with the note
  `may be called through Store.save`. A rule about what code can reach (`mustNotReach`) then
  follows the call. It is not the default: with it, the code that calls an interface uses
  the files that implement it. `ignoringCandidates()` leaves these uses out of a rule.
  refpact does not check that the class declares the member itself; a class that inherits
  it is named too.
- **A use in the file that declares the owner is no reference,** such as `this.total`.
- **A member of a type without a name is no reference:** an object literal, or a type
  written in place, as in `box: { value: number }`.
- **Only `object.name` is read.** `object["name"]` and a destructuring (`const { total } =
  order`) are not.
- **The load takes longer:** the front end then reads every function body. A generated
  project of 3,000 files with 180,000 uses of members loads in 6.6 seconds with the option
  and in 2.0 seconds without it.

## Lengths in TypeScript

`mustNotExceedLines` puts a limit on the lines of a file, or of an item. It works as
[lengths](rust.md#lengths) in Rust.

```ts
rule("no file is longer than 400 lines").files(allExcept("**/*.test.ts")).mustNotExceedLines(400).check(app);
rule("no function is longer than 60 lines").items(anyOf("src/**")).ofKind("function").mustNotExceedLines(60).check(app);
```

- The lines of a file are all its lines, with the blank lines and the comments. A line
  break at the end of the file starts no line.
- The lines of an item run from the line of its name to the line on which its declaration
  ends. The decorators and the comments above the name do not count.
- The items are the top-level declarations of a file. The methods of a class and the
  functions inside other functions are not measured on their own.
- A variable ends where its own declaration ends: `const handler = () => { ... }` has the
  lines of the function.

## Markers, reasons and diagrams in TypeScript

`mustNotContain` forbids constructs that have no path. The TypeScript front end has four
markers:

| Marker | Is |
|---|---|
| `console` | a use of the global `console`, as in `console.log(..)`. A local variable or an import named `console` is not one. |
| `debugger` | a `debugger` statement |
| `unresolved-import` | an import of a relative path, a `paths` alias or a `#name` of `imports` that leads to no file. The note of the violation is the specifier. An import of a package is never one: refpact does not check that a package is installed. |
| `computed-import` | an `import(..)`, a `require(..)` or another place that names a module with a specifier that the code computes from a value that refpact does not know, as in `import(name)`. The note is what is known of the specifier, with `*` for the rest. See [computed specifiers](#computed-specifiers). |

```ts
rule("the library does not print").files(anyOf("src/**")).mustNotContain("console").check(app);
rule("every import resolves").files(all()).mustNotContain("unresolved-import").check(app);
```

```text
rule `every import resolves` is violated in 1 place(s):
  src/main.ts:1:22: [every import resolves] src/main.ts contains unresolved-import (./missing)
```

refpact has no rules of its own for the conventions of code, as in
[Rust](rust.md#conventions-of-the-code). A linter such as oxlint has `no-console` and
`no-debugger` for a whole project. A rule with a marker is for a convention that depends on
the file: "only the command line prints".

`onionArchitecture` works as in [Rust](rust.md#the-onion-architecture), with files:

```ts
onionArchitecture("the shop")
  .domainModels("src/domain/model/**")
  .domainServices("src/domain/service/**")
  .applicationServices("src/application/**")
  .adapter("web", "src/adapter/web/**")
  .adapter("database", "src/adapter/db/**")
  .check(app);
```

`because` gives a rule a reason, as in [Rust](rust.md#why-a-rule-exists). The report prints
it below the heading, and a failure of type `violated` has it as `reason`. `layers(..)` and
`onionArchitecture(..)` have `because` too.

`asWarning` makes a rule a warning, as in [Rust](rust.md#warnings): its violations are
reported with `console.warn`, and the check passes. Vitest and `node --test` show that output
for a test that passes.

```ts
rule("new code does not log").files(all()).mustNotContain("console").asWarning().check(app);
```

```text
warning: rule `new code does not log` is violated in 1 place(s):
  src/cli.ts:4:3: [new code does not log] src/cli.ts contains console
```

A warning that cannot fail as written is still an error, a stale baseline entry still fails,
and `evaluate` gives the violations of a warning as of any rule.

`diagram` draws what a rule sees, as in [Rust](rust.md#diagrams): the files of the rule, or
its slices, and the imports between them, with the files and imports of a cycle in red. The
format is `"mermaid"` (the default) or `"dot"`. A rule that cannot fail as written throws an
`ArchitectureError`.

```ts
const text = rule("the features").files(all()).mustBeAcyclicBetween(anyOf("src/features/*")).diagram(app);
writeFileSync("docs/features.md", `\`\`\`mermaid\n${text}\`\`\`\n`);
```

`layers(..)` and `onionArchitecture(..)` have `diagram` too: one node for each layer, from the
top to the bottom, and the imports between them. An import that the layers forbid is red. A
file is in the first layer that matches it, and a file in no layer is not drawn.

```ts
const text = layers("the layers").layer("ui", "src/ui/**").layer("db", "src/db/**").diagram(app);
```

Rules about what the code of an item imports, and about the members of classes, are Rust
only: the TypeScript front end records imports at the top of a file, not the uses of names
inside the items.

## Rules of your own in TypeScript

`mustSatisfy` takes a check of your own, as in [Rust](rust.md#rules-of-your-own): a function
that gets the `Scope` of the rule and returns the violations. The core selects the files and
applies the filters and the exemptions. The rule reports the violations with its reason and
baselines them. The rules about `package.json` below are checks of this kind.

```ts
rule("a file imports at most ten files")
  .files(anyOf("src/**"))
  .mustSatisfy((scope) =>
    scope.subjects.flatMap((file) => {
      const imports = scope.imports.filter((seen) => seen.file === file);
      return imports.length > 10 ? [scope.violationOfFile(file, `imports ${imports.length} files`, "imports too many files")] : [];
    }),
  )
  .ignoringTypeImports()
  .check(app);
```

| `Scope` has | |
|---|---|
| `subjects` | the files that the rule selects |
| `imports` | the imports that the rule sees, of all files, in the order of the facts. Each has `fact`, `reference`, `file`, `subject` (whether the rule selects the file), `paths` and `owners`, the file of each path or `undefined` for a package. |
| `markers`, `declarations` | the markers and the declarations of the selected files that the rule sees |
| `graph()`, `subjectGraph()` | the graph of all files, and the graph of the selected files only: `unit_graph()` and `graph(None)` of a [Rust check](rust.md#graphs-in-a-check). `edgesOf(file)` are the files that a file imports, `chain(from, isEnd)` is a shortest chain of imports to the nearest file that is an end, and `tangles()` are the groups of files that all reach each other, each with its `members` and a shortest `cycle`. The tangles of `subjectGraph()` are the cycles that `mustBeAcyclic` reports. A check in TypeScript has no graph of slices |
| `matched(name)` | the files that a list of file patterns of the check matches |
| `violationAt(fact, message, key?)`, `violationOfItem(item, message, key?)`, `violationOfFile(file, message, key?)` | a violation at the place of a fact or of an item, or about a file as a whole |

- A message prints after the file. A baseline counts the violation by `key`, or by the
  message when there is no key.
- Instead of a message, a check may give any `Detail`, such as one of type `uses`.

### Metrics in a check

The graph of a check gives the numbers that metrics are made of, as in
[Rust](rust.md#metrics-in-a-check): how many files a file imports, how many files import it,
and its instability, the share of the first among both. The filters of the rule decide what
counts: with `ignoringTypeImports()`, the numbers are those of the code that runs.

```ts
rule("a file that many files import is stable")
  .files(anyOf("src/**"))
  .mustSatisfy((scope) => {
    const graph = scope.graph();
    const importers = new Map<string, number>();
    for (const file of scope.facts.units) {
      for (const edge of graph.edgesOf(file)) {
        importers.set(edge.to.join("/"), (importers.get(edge.to.join("/")) ?? 0) + 1);
      }
    }
    return scope.subjects.flatMap((file) => {
      const imports = graph.edgesOf(file).length;
      const importedBy = importers.get(file.join("/")) ?? 0;
      const instability = imports / Math.max(imports + importedBy, 1);
      if (importedBy <= 20 || instability < 0.5) {
        return [];
      }
      const text = `is imported by ${importedBy} files and imports ${imports}: instability ${instability.toFixed(2)}`;
      return [scope.violationOfFile(file, text, "is not stable")];
    });
  })
  .ignoringTypeImports()
  .check(app);
```

### Patterns in a check

A function cannot match a pattern, so a check names the lists of patterns that it needs in
the second argument of `mustSatisfy`. The core checks and matches them. A pattern that
matches nothing is an error, as it is for `mustNotImport`.

```ts
rule("the database is imported for its types only, outside the tests")
  .files(anyOf("src/**"))
  .mustSatisfy(
    (scope) => {
      const tests = new Set(scope.matched("tests"));
      return scope.imports
        .filter((seen) => seen.subject && !tests.has(seen.file))
        .filter((seen) => seen.hits["db"] !== undefined && seen.reference.kind !== "import-type")
        .map((seen) => scope.violationAt(seen.fact, "imports a value of the database"));
    },
    { targets: { db: anyOf("src/db/**", pkg("pg")) }, files: { tests: anyOf("src/**/*.test.ts") } },
  )
  .check(app);
```

- `targets` are lists of targets, as in `mustNotImport`. Each import has `hits`: for each
  list, by its name, the first path of the import that the list covers.
- `files` are lists of file patterns, which match as the files of a rule do.
  `scope.matched(name)` gives the files of a list. They are the paths of `scope.subjects` and
  of the imports, so they compare with `===`.

### Items in a check

`items(..)` takes a check as well. The filters of the rule select the items, and the check
gets them as `scope.items`, with all that the scope of a rule about files has.

```ts
rule("a subclass is exported")
  .items(anyOf("src/**"))
  .ofKind("class")
  .mustSatisfy((scope) =>
    scope.items
      .filter((item) => item.visibility === "local" && item.relations.some((tie) => tie.relation === "extends"))
      .map((item) => scope.violationOfItem(item, `declares ${item.name}, which extends a class and is not exported`)),
  )
  .check(app);
```

### Rule sets of your own

`rules(..)`, `layers(..)` and `onionArchitecture(..)` are rule sets: several rules that one
check reports together. A class of your own is a rule set when it extends `RuleSet`. It then
has `check`, `verify` and the baseline methods of the built-in sets.

```ts
class NoDatabase extends RuleSet {
  readonly name = "no part imports the database";
  private readonly parts: readonly string[];

  constructor(parts: readonly string[]) {
    super();
    this.parts = parts;
  }

  rules(): ImportRule[] {
    return this.parts.map((part) =>
      rule(`${part} does not import the database`).files(anyOf(`src/${part}/**`)).mustNotImport(anyOf("src/db/**")),
    );
  }
}

new NoDatabase(["ui", "domain"]).check(app);
```

A set that cannot fail as written throws an `ArchitectureError` of type `malformed` from
`rules()`. The check reports it as the failure of the set.

## Packages and package.json

With `readDependencies`, a load reads what the `package.json` files name, as
[`reading_dependencies()`](rust.md#dependencies-of-the-manifest) reads `Cargo.toml` in Rust.
Each `package.json` that a file of the project belongs to is then a file of the project, and
each dependency is a reference from it to the package, at the line of its key.

```ts
const app = loadProject("tsconfig.json", { readDependencies: true });

rule("the app is installed without lodash")
  .files(anyOf("package.json"))
  .mustNotImport(anyOf(pkg("lodash")))
  .onlyKinds("dependency", "transitive-dependency")
  .check(app);
```

```text
rule `the app is installed without lodash` is violated in 1 place(s):
  package.json:12:5: [the app is installed without lodash] package.json uses lodash (transitive-dependency, through `charts`, `d3-kit`)
```

| Kind | What it is |
|---|---|
| `dependency`, `dev-dependency`, `peer-dependency`, `optional-dependency` | A package that `dependencies`, `devDependencies`, `peerDependencies` or `optionalDependencies` names. |
| `transitive-dependency` | A package that a dependency, a peer dependency or an optional dependency needs, and that these three lists do not name. It is at the line of that dependency, and its note names the packages on the way. |
| `transitive-dev-dependency` | A package that only the `devDependencies` need. |

- **The file is `package.json`, or `packages/app/package.json` in a monorepo:** its path from
  the root. `all()` selects it, so a rule such as `mustBeImported` leaves it out with
  `allExcept("**/package.json")`.
- **A package of the project is also its `package.json`.** When `packages/app/package.json`
  names `@shop/ui`, and `packages/ui/package.json` declares it, the dependency is also a
  reference to that file. So `mustBeAcyclic`, `layers` and the other rules about files see
  the dependencies between the packages of a monorepo:
  `rule("..").files(anyOf("packages/*/package.json")).mustBeAcyclic()`.
- **What a dependency needs is read from `node_modules`,** as Node finds it, through the
  links of pnpm too. A package that is not installed is named and not followed. Another
  version of a package, installed below the package that needs it, is a reference of its own. The
  `devDependencies` of other packages are not needed.
- **A `package.json` with a name declares one item of the kind `package`:**
  `rule("..").items(anyOf("packages/*/package.json")).ofKind("package").mustBeNamed("@shop/*")`.
- It is not the default: a rule with a list of what all files may import would have to name
  every package, and the load reads the `package.json` of each installed package that the
  dependencies lead to. Without the option, a rule that names one of these kinds is an error.

### Versions and licences in TypeScript

Each of these references has what the `package.json` of the installed package says, as
`properties`, as in [Rust](rust.md#versions-and-licences). No built-in rule reads them: a
[rule of your own](#rules-of-your-own-in-typescript) does. The rule names the properties
that its check reads, with `readingProperties(..)`. A name that the load does not have is
then an error, as a kind that it does not have is: a check does not pass because it reads
`licence` for `license`, or because the load does not read the dependencies.

| Property | What it is |
|---|---|
| `version` | The version of the installed package: `4.17.21`. Two versions of one package are two references. |
| `license` | The licence, as the package writes it: `MIT`, or `(MIT OR Apache-2.0)`. The old form `{ "type": "MIT" }` gives `MIT`. A package that names no licence has no such property. |

There is no `source` property, as Rust has: the `package.json` of an installed package does
not say where the package came from. Only the lockfile does, in a format of its own for each
package manager.

```ts
const accepted = ["MIT", "ISC", "Apache-2.0"];

rule("every package has a licence that we accept")
  .files(anyOf("**/package.json"))
  .mustSatisfy((scope) =>
    scope.imports
      .filter((seen) => seen.subject)
      // a package of the project is no package of others
      .filter((seen) => seen.owners.every((owner) => owner === undefined))
      .flatMap((seen) => {
        const licence = seen.reference.properties?.["license"];
        if (licence !== undefined && accepted.includes(licence)) {
          return [];
        }
        const name = core.printPath(seen.reference.target, scope.facts);
        return [scope.violationAt(seen.fact, `is installed with ${name}, which has ${licence !== undefined ? `the licence ${licence}` : "no licence"}`)];
      }),
  )
  .readingProperties("license")
  .onlyKinds("dependency", "transitive-dependency")
  .check(app);
```

```text
rule `every package has a licence that we accept` is violated in 1 place(s):
  package.json:4:5: [every package has a licence that we accept] package.json is installed with shared, which has the licence GPL-3.0-only (through `direct`)
```

- The licence is text, and the check compares it as text. An expression such as
  `(MIT OR Apache-2.0)` is one value: name each expression that you accept, or split it in
  the check.
- A package that is not installed has no properties, and the check decides what a package
  without a licence is. Here it is a violation.
- The filters of the rule say which packages count: the kinds above leave out what only the
  development needs.

Two versions of one package are found from the versions of all references:

```ts
rule("one version of each package is installed")
  .files(anyOf("**/package.json"))
  .mustSatisfy((scope) => {
    const versions = new Map<string, Set<string>>();
    const versioned = scope.imports.flatMap((seen) => {
      const version = seen.reference.properties?.["version"];
      return version !== undefined ? [{ seen, version, name: core.printPath(seen.reference.target, scope.facts) }] : [];
    });
    for (const { name, version } of versioned) {
      versions.set(name, (versions.get(name) ?? new Set()).add(version));
    }
    return versioned
      .filter(({ seen, name }) => seen.subject && (versions.get(name) as Set<string>).size > 1)
      .map(({ seen, name, version }) => {
        const all = [...(versions.get(name) as Set<string>)].sort().join(", ");
        return scope.violationAt(seen.fact, `is installed with ${name} ${version}, one of ${all}`, `is installed with several versions of ${name}`);
      });
  })
  .readingProperties("version")
  .check(app);
```

Not read: the advisories about a package, and whether its authors deprecated it. Keep
`npm audit` or the check of your package manager for these.

### Rules about the imports and package.json

Two ready-made rules compare the imports with these references. They are checks that a
[rule of your own](#rules-of-your-own-in-typescript) can make from the same facts, and they
need a project that is loaded with `readDependencies`. They are in the TypeScript binding
only. In Rust, Cargo does this work: a crate that `Cargo.toml` does not name does not build,
and a dev-dependency is not there outside test code.

```ts
const app = loadProject("tsconfig.json", { readDependencies: true });

rule("the source imports only declared packages")
  .files(anyOf("src/**"))
  .mustOnlyImportDeclaredPackages()
  .check(app);

rule("shipped code imports no development package")
  .files(allExcept("**/*.test.ts", "tests/**"))
  .mustNotImportDevDependencies()
  .check(app);
```

- **The `package.json` of a file is the nearest one above it.** In a monorepo, this is the
  manifest of the package, not that of the workspace root: a package that only the root
  declares is not declared. A `package.json` without a name and without dependencies, such
  as one that only sets `"type"`, is passed over.
- `mustOnlyImportDeclaredPackages` accepts a package in `dependencies`, `devDependencies`,
  `peerDependencies` or `optionalDependencies`. A package may import itself by its own name.
- `mustNotImportDevDependencies` reports a package that is in `devDependencies` and in none
  of the other three lists.
- **Types.** A type import of `name` is also declared by `@types/name`, and of `@scope/name`
  by `@types/scope__name`. A value import is not. So `import { Node } from "estree"` with
  only `@types/estree` is reported as not declared: write `import type`. For the second
  rule, a type import through a `@types` package in `devDependencies` is a violation;
  `ignoringTypeImports()` leaves these out.
- A package is known by the name in the import, also when it is not installed. Modules of
  Node, `paths` aliases and `#imports` are not packages.
- The filters and [baselines](rust.md#baselines) work as in other rules.
- A project in which no `package.json` has `devDependencies` cannot break the second rule,
  and that is an error.

A package that is declared and never imported is a rule of your own: the `dependency`
references of `package.json` whose package no import names. Not checked: the version ranges
that a `package.json` asks for,
and a package that is installed under another name (`"alias": "npm:real@1"` is known as `alias`,
which is the declared name).

## What counts as an import in TypeScript

| Kind | Form |
|---|---|
| `import` | `import x from`, `import { a as b } from`, `import * as ns from`, `import "m"` |
| `import-type` | `import type ..`, `import { type T }`, `import("m").T` and `typeof import("m")` in a type, `import type x = require(..)` |
| `dynamic-import` | `import("m")`, and each file that `import.meta.glob("./pages/*.ts")` matches |
| `require` | `require("m")`, `import x = require("m")` |
| `export-from` | `export { a } from`, `export * from`, `export * as ns from` |
| `export-type-from` | `export type { T } from`, `export { type T } from` |
| `resolve` | a place that asks where a module is: `require.resolve("m")`, `import.meta.resolve("m")`, `new URL("./worker.ts", import.meta.url)` |
| `member` | `object.name`, where the name is a member of a class, an interface or an enum of another file; with [`resolveMembers`](#members-in-typescript) only |
| `global` | a use of a global of the host as a value: `fetch(..)`, `process.env`, `window.localStorage`; see [globals](#globals-in-typescript) |

Each imported name is one reference to `module#name`, at the place of the name. An import of
the module as a whole (`import "m"`, a namespace import, `export *`, `import()`, `require`)
is one reference to the module.

After `import * as ns from "m"`, `const ns = require("m")` or `import ns = require("m")`, each
`ns.name` is a reference to `m#name`. A rule against one export of a package finds the named
import and the use through the namespace. `const { a } = require("m")` is a reference to `m#a`.
A local variable or a parameter with the same name as the namespace is not counted: the
compiler says which is which. The same holds for a local function named `require`.

## How imports are resolved

The compiler resolves every import with the options of the project that owns the file:
`paths`, package `exports` and `imports`, project references (the tsconfig files in
`references` are loaded too, at any depth), and the `.js` to `.ts` mapping.

- An import that leads to a **file of the project** is a reference to that file. This includes
  relative imports, `paths` aliases, and `#name` imports from the `imports` field of
  `package.json`.
- An import that leads **through barrel files** is also a reference to each file that
  re-exports the name by name, and to the file that declares it. The report shows the
  declaring file and the path as written. So a rule against `src/db/client.ts` finds
  `import { query } from "./db"`.
- Any other bare specifier is a **package**: `lodash/fp` is the package `lodash` with the
  subpath `fp`. If the package is in the same workspace and resolves to a source file under
  the root, the import is also a reference to that file. A rule can name the package or the
  path.
- `fs` and `node:fs` are the same module of Node.

## Limits of the TypeScript analysis

- **The compiler API of TypeScript 7 is not stable.** See [above](#what-typescript-7-offers-to-tools). The
  versions that were run are listed there. A new minor version of TypeScript 7 can break
  `native.ts`. The native compiler also stops with a panic when it is asked for the target of a
  symbol that is not an alias, in 7.0.2 and in 7.1.0-beta; `native.ts` checks the flag first.
- **TypeScript 5 and older are not supported.** `loadProject` fails when the installed
  `typescript` is not version 6 or 7.
- **`baseUrl` is an error with TypeScript 7,** which removed it. It is not a silent miss.
- **A specifier that the code computes** is followed as far as the file says what it is; see
  [computed specifiers](#computed-specifiers). Triple-slash references and
  `declare module "m"` are not references.
- **Imports in JSDoc** are references in JavaScript files only: an `import("m")` type in a
  JSDoc comment of a JavaScript file, because the compiler makes syntax nodes from such
  comments.
- **`require("./x")` in a TypeScript file** is not a module reference for the compiler. For a
  relative path, refpact looks in the file system with the usual extensions and `index`
  files. A package is known by its name in any case. A relative import that leads nowhere is
  reported under the path it names, with the note `not resolved`; only an `unverified` target
  can match it.
- **A global or a namespace behind another name is followed one step, in one file:**
  - `const { env } = process` is a use of `process.env`, and `const { query } = db`, where
    `db` is a namespace import, is a use of `db#query`.
  - After `const p = process` or `const store = db`, `p.exit` and `store.query` are uses of
    `process.exit` and of `db#query`. The variable must be a `const` that the file declares
    one time.
  - `const f = fetch` is a use of `fetch` where it is written. The later calls of `f` are in
    the same file and add nothing.
  - Not followed: `ns["name"]`, `globalThis["fetch"]`, a `let`, a second step
    (`const a = db; const b = a;`), a value that is passed to other code, and the result of
    `await import("m")`. The namespace import itself is a reference to the whole module.
- **A barrel that passes a name on with `export *`** is not listed between the file that the
  import names and the file that declares the name. A barrel that re-exports by name is.
- **An import of a barrel as a whole** (`import * as db from "./db"`) is a reference to the
  barrel file, not to the files behind it. The members that the code uses are followed.
- **Type-only is decided by syntax:** `import type` and the `type` modifier. An import that is
  used only as a type and has no modifier is an `import`. With `verbatimModuleSyntax`, the
  syntax is the truth.
- **A package is known by the name in the import.** A package that `package.json` installs
  under another name has that other name.
- **Files outside the root are not units.** A file that two projects have is analysed once,
  with the options of the first project.
- **No tags for test code.** Leave test files out by path: `allExcept("**/*.test.ts")`.
- **With TypeScript 7, the compiler runs as a child process** that the client talks to with
  blocking calls. This works in Vitest workers and in Node. It needs a platform that has a
  native TypeScript binary.

Every row of the tables above has a fixture file in
[`fixtures/ts/planted/src/forms`](../fixtures/ts/planted/src/forms) with planted violations, and a
test that fails when the detection is removed (`cargo xtask mutants ts`).
