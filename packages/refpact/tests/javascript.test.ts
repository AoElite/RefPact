// JavaScript projects: read through a jsconfig.json or with default options, with ES modules and CommonJS.
import { mkdirSync, readdirSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, loadProject, pkg, rule } from "../src/index.ts";

const directory = scratchDirectory("refpact-javascript-");
const root = path.join(directory, "app");
const files: Record<string, string[]> = {
  "jsconfig.json": ['{ "compilerOptions": { "module": "nodenext" }, "include": ["src"] }'],
  "package.json": ['{ "name": "app", "type": "module" }'],
  "src/api.js": ['import { query } from "./db.js";', "/** Gets the rows. */", "export async function get() { return query(); }"],
  "src/db.js": ['import { readFileSync } from "node:fs";', "export function query() { return readFileSync; }"],
  "src/cli.cjs": ['const { get } = require("./api.js");', "module.exports = { run: get };"],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}

it("a directory with a jsconfig.json is a JavaScript project", () => {
  const app = loadProject(root);
  expect(app.configFiles).toEqual([path.join(root, "jsconfig.json")]);
  expect(app.files).toEqual(["src/api.js", "src/cli.cjs", "src/db.js"]);
  const imports = app.facts.facts.map((fact) => (fact.what.type === "reference" ? `${fact.file}:${fact.line} ${printPath(fact.what.target, app.facts)} (${fact.what.kind})` : "?"));
  expect(imports).toEqual(["src/api.js:1 src/db.js#query (import)", "src/cli.cjs:1 src/api.js#get (require)", "src/db.js:1 node:fs#readFileSync (import)"]);

  const result = rule("only the db reads files").files(all()).mustNotImport(anyOf(pkg("node:fs"))).evaluate(app);
  expect(result.ok && result.violations.map((violation) => violation.file)).toEqual(["src/db.js"]);
  rule("the cli uses the api").files(anyOf("src/cli.cjs")).mustOnlyImport(anyOf("src/api.js")).check(app);
  rule("functions are documented").items(anyOf("src/api.js")).ofKind("function").mustHaveModifier("documented").check(app);
});

it("a directory without a config is read with default options", () => {
  const plain = path.join(directory, "plain");
  const plainFiles: Record<string, string> = {
    "src/api.js": 'import { query } from "./db";\nimport { log } from "./log.js";\nexport const get = () => query(log);\n',
    "src/db.mjs": "export function query() {}\n",
    "src/log.js": "export const log = 1;\n",
    "src/cli.cjs": 'const { get } = require("./api");\nmodule.exports = get;\n',
    "node_modules/lib/index.js": "export const lib = 1;\n",
  };
  for (const [file, text] of Object.entries(plainFiles)) {
    mkdirSync(path.dirname(path.join(plain, file)), { recursive: true });
    writeFileSync(path.join(plain, file), text);
  }
  const configs = () => readdirSync(tmpdir()).filter((name) => name.startsWith("refpact-config-"));
  const before = configs();
  const project = loadProject(plain);
  expect(project.root).toBe(plain);
  expect(project.configFiles).toEqual([]);
  expect(project.files).toEqual(["src/api.js", "src/cli.cjs", "src/db.mjs", "src/log.js"]);
  const imports = project.facts.facts.map((fact) => (fact.what.type === "reference" ? `${fact.file} ${printPath(fact.what.target, project.facts)}` : "?"));
  expect(imports).toEqual(["src/api.js src/db.mjs#query", "src/api.js src/log.js#log", "src/cli.cjs src/api.js#get"]);
  // The config that refpact wrote is gone.
  expect(configs()).toEqual(before);
});

it("a directory without a config and without files is an error", () => {
  const empty = path.join(directory, "empty");
  mkdirSync(empty, { recursive: true });
  expect(() => loadProject(empty)).toThrow("has no tsconfig.json, no jsconfig.json and no JavaScript or TypeScript files");
});
