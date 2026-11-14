// The globals of the host, such as `process` and `fetch`: each use is a reference of the kind `global`.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, allExcept, anyOf, ArchitectureError, globalApi, loadProject, pkg, rule } from "../src/index.ts";

const directory = scratchDirectory("refpact-globals-");
const typeRoots = path.join(import.meta.dirname, "../node_modules/@types");
const files: Record<string, string[]> = {
  "tsconfig.json": [
    JSON.stringify({ compilerOptions: { module: "preserve", noEmit: true, lib: ["es2022", "dom"], types: ["node"], typeRoots: [typeRoots] }, include: ["src"] }),
  ],
  "src/config.ts": ["export const port = process.env.PORT;", "export function stop(): void {", "  process.exit(1);", "}"],
  "src/api.ts": ['export async function get(): Promise<Response> {', '  return fetch("/rows");', "}", "export const saved = window.localStorage.getItem('k') ?? localStorage.length;"],
  "src/pure.ts": ["export const now = Date.now() + Math.random();", "export const text = JSON.stringify([Promise.resolve(1), new Error('x')]);", "export const coded = encodeURIComponent(text) + globalThis.decodeURI(text);"],
  "src/local.ts": [
    "const fetch = (url: string): string => url;",
    "export const shadowed = fetch('/rows');",
    "export const declared = appName;",
    "export function local(process: string, { URL: url }: { URL: string }): string {",
    "  for (const document of [process]) {",
    "    return document + url;",
    "  }",
    "  return ({ window: 1, localStorage: 2 }).window + '';",
    "}",
  ],
  "src/shorthand.ts": ["export const handlers = { fetch, setTimeout };", "export class Store {", "  localStorage = 1;", "  process(): void {}", "}"],
  "src/types.d.ts": ["declare global {", "  var appName: string;", "}", "export {};"],
  "src/typed.ts": [
    "export let body: Response | undefined;",
    "export const made = new Response();",
    'import fs = require("node:fs");',
    'import { join as joinPath } from "node:path";',
    "export const read = [fs.readFileSync, joinPath];",
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(directory, file)), { recursive: true });
  writeFileSync(path.join(directory, file), `${lines.join("\n")}\n`);
}
const app = loadProject(directory);

function globals(file: string): string[] {
  return app.facts.facts.flatMap((fact) =>
    fact.file === file && fact.what.type === "reference" && fact.what.kind === "global" ? [`${fact.line}:${fact.column} ${printPath(fact.what.target, app.facts)}`] : [],
  );
}

it("a use of a global is a reference with the member that is used", () => {
  expect(globals("src/config.ts")).toEqual(["1:21 global:process#env", "3:3 global:process#exit"]);
  expect(globals("src/api.ts")).toEqual(["2:10 global:fetch", "4:22 global:localStorage#getItem", "4:58 global:localStorage#length"]);
});

it("the ECMAScript library has no globals", () => {
  expect(globals("src/pure.ts")).toEqual([]);
  rule("the pure code uses only itself").files(anyOf("src/pure.ts")).mustOnlyImport(anyOf("src/pure.ts")).check(app);
});

it("a name that the project declares is no global, also with declare global", () => {
  expect(globals("src/local.ts")).toEqual([]);
});

it("a property name is no use of a global, and a shorthand property is", () => {
  expect(globals("src/shorthand.ts")).toEqual(["1:27 global:fetch", "1:34 global:setTimeout"]);
});

it("a type is no use of a global, and require is an import", () => {
  expect(globals("src/typed.ts")).toEqual(["2:25 global:Response"]);
});

it("a rule forbids a global and its members", () => {
  const failure = rule("only the config reads the environment").files(allExcept("src/config.ts")).mustNotImport(anyOf(globalApi("process"), globalApi("fetch"))).verify(app);
  expect(failure?.message).toBe(
    "rule `only the config reads the environment` is violated in 2 place(s):\n" +
      "  src/api.ts:2:10: [only the config reads the environment] src/api.ts uses global:fetch (global)\n" +
      "  src/shorthand.ts:1:27: [only the config reads the environment] src/shorthand.ts uses global:fetch (global)",
  );
});

it("a member of a global is a target of its own", () => {
  const failure = rule("nothing exits").files(all()).mustNotImport(anyOf(globalApi("process.exit"))).verify(app);
  expect(failure?.message).toBe("rule `nothing exits` is violated in 1 place(s):\n  src/config.ts:3:3: [nothing exits] src/config.ts uses global:process#exit (global)");
  rule("storage is read in the api").files(allExcept("src/api.ts")).mustNotImport(anyOf(globalApi("*Storage"))).check(app);
});

it("an allow-list sees the globals, and ignoringKinds leaves them out", () => {
  const allowList = rule("the config uses only node").files(anyOf("src/config.ts")).mustOnlyImport(anyOf(pkg("node:*")));
  expect(() => allowList.check(app)).toThrow(ArchitectureError);
  allowList.ignoringKinds("global").check(app);
});
