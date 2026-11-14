// A member that the code takes from a namespace or from a global through a destructuring or
// through a `const` that stands for it.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, globalApi, loadProject, rule } from "../src/index.ts";

const directory = scratchDirectory("refpact-aliases-");
const typeRoots = path.join(import.meta.dirname, "../node_modules/@types");
const files: Record<string, string[]> = {
  "tsconfig.json": [JSON.stringify({ compilerOptions: { module: "preserve", noEmit: true, types: ["node"], typeRoots: [typeRoots] }, include: ["src"] })],
  "src/db.ts": ["export const query = 1;", "export const close = 2;", "export const pool = 3;"],
  "src/namespace.ts": [
    'import * as db from "./db.ts";',
    "const { query, close: end, ...rest } = db;",
    "const store = db;",
    "export const used = [query, end, rest, store.pool];",
    "export function shadow(store: { pool: number }): number {",
    "  return store.pool;",
    "}",
    "let other = db;",
    "export const changed = other.query;",
  ],
  "src/global.ts": [
    "const { env, exit: stop } = process;",
    "const p = process;",
    "export const used = [env, stop, p.argv, p];",
    "export function shadow(p: { argv: string[] }): string[] {",
    "  const process = { cwd: 1 };",
    "  const q = process;",
    "  return [...p.argv, String(q.cwd)];",
    "}",
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(directory, file)), { recursive: true });
  writeFileSync(path.join(directory, file), `${lines.join("\n")}\n`);
}
const app = loadProject(directory);

function references(file: string): string[] {
  return app.facts.facts.flatMap((fact) =>
    fact.file === file && fact.what.type === "reference" ? [`${fact.line}:${fact.column} ${fact.what.kind} ${printPath(fact.what.target, app.facts)}`] : [],
  );
}

it("a destructuring and a const of a namespace take its members", () => {
  expect(references("src/namespace.ts")).toEqual([
    "1:13 import src/db.ts",
    // `...rest` takes no name.
    "2:9 import src/db.ts#query",
    "2:16 import src/db.ts#close",
    // `store.pool`, where `const store = db`. The parameter `store` of line 5 is another
    // variable, and `other` of line 8 is not a `const`.
    "4:46 import src/db.ts#pool",
  ]);
  const failure = rule("the pool is not used").files(anyOf("src/namespace.ts")).mustNotImport(anyOf("src/db.ts#pool")).verify(app);
  expect(failure?.message).toBe(
    "rule `the pool is not used` is violated in 1 place(s):\n  src/namespace.ts:4:46: [the pool is not used] src/namespace.ts uses src/db.ts#pool (import)",
  );
});

it("a destructuring and a const of a global take its members", () => {
  expect(references("src/global.ts")).toEqual([
    "1:9 global global:process#env",
    "1:14 global global:process#exit",
    "2:11 global global:process",
    // `p.argv`, where `const p = process`. In `shadow`, `p` and `process` are local.
    "3:33 global global:process#argv",
  ]);
  const failure = rule("nothing exits").files(all()).mustNotImport(anyOf(globalApi("process.exit"))).verify(app);
  expect(failure?.message).toBe("rule `nothing exits` is violated in 1 place(s):\n  src/global.ts:1:14: [nothing exits] src/global.ts uses global:process#exit (global)");
});
