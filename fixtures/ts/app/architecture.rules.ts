// The rules of the clean fixture: the TypeScript counterparts of the five example rules in
// `crates/refpact/tests/five_rules.rs`, written as a project would write them.
import { allExcept, anyOf, loadProject, pkg, rule } from "refpact";
import type { ImportRule } from "refpact";

export const app = loadProject(new URL("./tsconfig.json", import.meta.url).pathname);

export const rules: ImportRule[] = [
  rule("the user interface does not import the database")
    .files(anyOf("src/ui/**"))
    .mustNotImport(anyOf("src/db/**"))
    .ignoringTypeImports(),

  rule("the database does not import the layers above it")
    .files(anyOf("src/db/**"))
    .mustNotImport(anyOf("src/ui/**", "src/app/**")),

  rule("the operating system is used in the platform layer only")
    .files(allExcept("src/platform/**"))
    .mustNotImport(anyOf(pkg("node:fs"), pkg("node:child_process"), pkg("node:net"))),

  rule("randomness from the system is used in one file only")
    .files(allExcept("src/platform/random.ts"))
    .mustNotImport(
      anyOf(
        pkg("node:crypto").export("randomBytes"),
        pkg("node:crypto").export("randomUUID"),
        pkg("node:crypto").export("getRandomValues"),
      ),
    ),
];
