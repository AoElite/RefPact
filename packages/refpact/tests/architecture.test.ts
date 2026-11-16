// refpact checks its own TypeScript package.
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

import { allExcept, anyOf, globalApi, layers, loadProject, pkg, rule } from "../src/index.ts";

const self = loadProject(fileURLToPath(new URL("../tsconfig.json", import.meta.url)));
// The rules about `package.json` read what it names.
const selfWithDependencies = loadProject(fileURLToPath(new URL("../tsconfig.json", import.meta.url)), { readDependencies: true });

const COMPILER_PACKAGES = [pkg("typescript"), pkg("typescript-6"), pkg("typescript-71")];

// The core is the Rust crate `refpact-core`, run as WebAssembly. `src/core` holds its types,
// the functions that call it, and the loader of the module, which alone reads a file.
it("the core imports nothing but the core", () => {
  rule("the core imports nothing but the core")
    .files(anyOf("src/core/**").except("src/core/wasm.ts"))
    .mustNotImport(
      anyOf(
        "src/frontend/**",
        "src/rules.ts",
        "src/index.ts",
        ...COMPILER_PACKAGES,
        pkg("node:fs"),
        pkg("node:path"),
        pkg("node:module"),
        pkg("node:child_process"),
      ),
    )
    .check(self);
});

it("only the backend imports the compiler", () => {
  rule("only the backend imports the compiler")
    .files(allExcept("src/frontend/native.ts", "src/frontend/classic.ts"))
    .mustNotImport(anyOf(...COMPILER_PACKAGES))
    .check(self);
});

it("only compiler.ts imports the backend", () => {
  rule("only compiler.ts imports the backend")
    .files(allExcept("src/frontend/compiler.ts"))
    .mustNotImport(anyOf("src/frontend/native.ts", "src/frontend/classic.ts"))
    .check(self);
});

it("the front end does not import the rules", () => {
  rule("the front end does not import the rules")
    .files(anyOf("src/frontend/**"))
    .mustNotImport(anyOf("src/rules.ts", "src/index.ts"))
    .check(self);
});

it("the source does not import the tests or the test runner", () => {
  rule("the source does not import the tests or the test runner")
    .files(anyOf("src/**"))
    .mustNotImport(anyOf("tests/**", pkg("vitest")))
    .check(self);
});

// The core is public as `core`. Two tests read what the loader does before a project exists.
it("the tests use the package through its entry", () => {
  rule("the tests use the package through its entry")
    .files(anyOf("tests/**"))
    .mustNotImport(anyOf("src/**"))
    .ignoringImport(anyOf("tests/**"), anyOf("src/index.ts", "src/core/index.ts"))
    .ignoringImport(anyOf("tests/planted.test.ts", "tests/loading.test.ts"), anyOf("src/frontend/checks.ts"))
    .ignoringImport(anyOf("tests/loading.test.ts"), anyOf("src/frontend/project.ts"))
    .check(self);
});

it("only the project loader, the component files, the baseline, the diagram file, the dependencies and the loader of the core read the file system", () => {
  rule("only the project loader, the component files, the baseline, the diagram file, the dependencies and the loader of the core read the file system")
    .files(anyOf("src/**").except("src/frontend/project.ts", "src/frontend/components.ts", "src/baseline.ts", "src/diagram.ts", "src/frontend/dependencies.ts", "src/core/wasm.ts"))
    .mustNotImport(anyOf(pkg("node:fs"), pkg("node:child_process")))
    .check(self);
});

it("the core may only import the core", () => {
  rule("the core may only import the core").files(anyOf("src/core/**").except("src/core/wasm.ts")).mustOnlyImport(anyOf("src/core/**")).check(self);
  rule("the loader of the core reads one file").files(anyOf("src/core/wasm.ts")).mustOnlyImport(anyOf(pkg("node:fs"), pkg("node:url"), globalApi("TextEncoder"), globalApi("TextDecoder"), globalApi("URL")).andUnverified("wasm/refpact_core.wasm"))
    .check(self);
});

it("the source has no import cycle, type imports aside", () => {
  rule("the source has no import cycle, type imports aside").files(anyOf("src/**")).mustBeAcyclic().ignoringTypeImports().check(self);
});

it("the parts of the source form no cycle, type imports aside", () => {
  rule("the parts of the source form no cycle, type imports aside")
    .files(anyOf("src/**"))
    .mustBeAcyclicBetween(anyOf("src/*"))
    .ignoringTypeImports()
    .check(self);
});

it("the binding, the front end and the core are layers", () => {
  layers("the parts of the source").layer("binding", "src/*.ts").layer("front end", "src/frontend/**").layer("core", "src/core/**").check(self);
});

it("the core reaches no compiler", () => {
  rule("the core reaches no compiler").files(anyOf("src/core/**")).mustNotReach(anyOf(...COMPILER_PACKAGES)).check(self);
});

it("every source file is imported and reachable from the entry", () => {
  rule("every source file is imported").files(anyOf("src/**").except("src/index.ts")).mustBeImported().check(self);
  rule("every source file is reachable from the entry").files(anyOf("src/**")).mustBeReachableFrom(anyOf("src/index.ts")).check(self);
});

it("the source imports only packages that package.json declares", () => {
  rule("the source imports only declared packages").files(anyOf("src/**")).mustOnlyImportDeclaredPackages().check(selfWithDependencies);
  // The compilers under other names are for the tests. The source may name their types only.
  rule("the source imports no development package").files(anyOf("src/**")).mustNotImportDevDependencies().ignoringTypeImports().check(selfWithDependencies);
  // The package is installed with nothing of its own: the compiler is a peer that the user has.
  rule("the package names no dependencies of its own")
    .files(anyOf("package.json"))
    .mustNotImport(anyOf(pkg("*")))
    .onlyKinds("dependency", "optional-dependency")
    .check(selfWithDependencies);
});

// A rule that passes proves little. This one must find the imports that the rules above allow.
it("the rules above can fail", () => {
  const found = rule("probe")
    .files(anyOf("src/frontend/native.ts", "src/frontend/project.ts"))
    .mustNotImport(anyOf(...COMPILER_PACKAGES, pkg("node:fs")))
    .evaluate(self);
  expect(found.ok && found.violations.map((violation) => violation.file).filter((file, index, all) => all.indexOf(file) === index)).toEqual([
    "src/frontend/native.ts",
    "src/frontend/project.ts",
  ]);
  // With type imports, the backend and `compiler.ts` name each other.
  const cycles = rule("probe").files(anyOf("src/**")).mustBeAcyclic().evaluate(self);
  expect(cycles.ok && cycles.violations.length).toBeGreaterThan(0);
  // The rules reach the file system through the baseline.
  const chain = rule("probe").files(anyOf("src/layers.ts")).mustNotReach(anyOf(pkg("node:fs"))).evaluate(self);
  expect(chain.ok && chain.violations.map((violation) => violation.detail.type)).toEqual(["reaches"]);
  const development = rule("probe").files(anyOf("tests/**")).mustNotImportDevDependencies().evaluate(selfWithDependencies);
  expect(development.ok && development.violations.length).toBeGreaterThan(0);
});
