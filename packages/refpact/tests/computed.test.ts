// Specifiers that the code computes: a constant is an import, a part that is known names the
// files that it can be, and the rest is a marker.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, loadProject, pkg, rule } from "../src/index.ts";
import type { ImportRule, Violation } from "../src/index.ts";

const directory = scratchDirectory("refpact-computed-");
const files: Record<string, string[]> = {
  "tsconfig.json": [JSON.stringify({ compilerOptions: { module: "preserve", noEmit: true, allowImportingTsExtensions: true, allowJs: true, types: [] }, include: ["src"] })],
  "src/locales/en.ts": ["export default 'hello';"],
  "src/locales/de.ts": ["export default 'hallo';"],
  "src/pages/home.ts": ["export const page = 1;"],
  "src/pages/admin/index.ts": ["export const page = 2;"],
  "src/worker.ts": ["export {};"],
  "src/logo.png": [""],
  "src/constant.ts": [
    'const NAME = "./worker.ts";',
    "const DIRECTORY = './locales';",
    "export const a = import(NAME);",
    "export const b = import(`${DIRECTORY}/en.ts`);",
    'export const c = import(DIRECTORY + "/" + "de" + ".ts");',
    'export const d = (flag: boolean) => import(flag ? "./pages/home.ts" : NAME);',
    'export const e = import(("left-pad" as string)!);',
  ],
  "src/pattern.ts": [
    "export const load = (name: string) => import(`./locales/${name}.ts`);",
    "export const page = (name: string) => import('./pages/' + name);",
    "export const plugin = (name: string) => import(`plugins/${name}/index.js`);",
    "export const scoped = (name: string) => import(`@scope/kit/${name}`);",
    "export const builtin = (name: string) => require(`node:fs/${name}`);",
    "export const index = (name: string) => import(`./pages/${name}/index`);",
  ],
  "src/unknown.ts": [
    "const NAME = './worker.ts';",
    "export const anything = (name: string) => import(name);",
    "export const shadowed = (NAME: string) => import(NAME);",
    "export const somePackage = (name: string) => import(`${name}/sub`);",
    "let changed = './worker.ts';",
    "export const reassigned = () => import(changed);",
  ],
  "src/places.ts": [
    'export const worker = new URL("./worker.ts", import.meta.url);',
    'export const image = new URL("./logo.png", import.meta.url);',
    'export const where = import.meta.resolve("left-pad");',
    'export const other = new URL("./worker.ts", "https://example.com");',
  ],
  "src/places.cjs": ['module.exports = require.resolve("./worker.ts");'],
  "src/glob.ts": [
    'export const locales = import.meta.glob("./locales/*.ts");',
    'export const pages = import.meta.glob(["./pages/**/*.ts", "./missing/*.ts"]);',
    'export const top = import.meta.glob("./pages/*.ts");',
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(directory, file)), { recursive: true });
  writeFileSync(path.join(directory, file), `${lines.join("\n")}\n`);
}
const app = loadProject(directory);

/** The violations of a rule that can be checked. */
function violations(checked: ImportRule): readonly Violation[] {
  const result = checked.evaluate(app);
  if (!result.ok) {
    throw new Error("the rule cannot be checked");
  }
  return result.violations;
}

function found(file: string): string[] {
  return app.facts.facts.flatMap((fact) => {
    if (fact.file !== file) {
      return [];
    }
    const note = fact.note !== undefined ? ` (${fact.note})` : "";
    if (fact.what.type === "marker") {
      return [`${fact.line}:${fact.column} !${fact.what.marker}${note}`];
    }
    const sure = fact.what.certainty === "candidate" ? " ?" : "";
    return [`${fact.line}:${fact.column} ${fact.what.kind} ${printPath(fact.what.target, app.facts)}${sure}${note}`];
  });
}

it("a specifier that the code computes from constants is an import", () => {
  expect(found("src/constant.ts")).toEqual([
    "3:18 dynamic-import src/worker.ts",
    "4:18 dynamic-import src/locales/en.ts",
    "5:18 dynamic-import src/locales/de.ts",
    // Both sides of `? :`.
    "6:37 dynamic-import src/pages/home.ts",
    "6:37 dynamic-import src/worker.ts",
    "7:18 dynamic-import left-pad",
  ]);
  const failure = rule("the worker is not loaded").files(anyOf("src/constant.ts")).mustNotImport(anyOf("src/worker.ts")).verify(app);
  expect(failure?.message).toBe(
    "rule `the worker is not loaded` is violated in 2 place(s):\n" +
      "  src/constant.ts:3:18: [the worker is not loaded] src/constant.ts uses src/worker.ts (dynamic-import)\n" +
      "  src/constant.ts:6:37: [the worker is not loaded] src/constant.ts uses src/worker.ts (dynamic-import)",
  );
});

it("a specifier with a part that is not known names the files that it can be, as candidates", () => {
  expect(found("src/pattern.ts")).toEqual([
    "1:39 !computed-import (./locales/*.ts)",
    "1:39 dynamic-import src/locales/de.ts ? (the specifier is computed: ./locales/*.ts)",
    "1:39 dynamic-import src/locales/en.ts ? (the specifier is computed: ./locales/*.ts)",
    // Without an extension, and a directory with an index file.
    "2:39 !computed-import (./pages/*)",
    "2:39 dynamic-import src/pages/admin/index.ts ? (the specifier is computed: ./pages/*)",
    "2:39 dynamic-import src/pages/home.ts ? (the specifier is computed: ./pages/*)",
    // A package whose name is known.
    "3:41 !computed-import (plugins/*/index.js)",
    "3:41 dynamic-import plugins ? (the specifier is computed: plugins/*/index.js)",
    "4:41 !computed-import (@scope/kit/*)",
    "4:41 dynamic-import @scope/kit ? (the specifier is computed: @scope/kit/*)",
    "5:42 !computed-import (node:fs/*)",
    "5:42 require node:fs ? (the specifier is computed: node:fs/*)",
    // A part in the middle: the end has no extension.
    "6:40 !computed-import (./pages/*/index)",
    "6:40 dynamic-import src/pages/admin/index.ts ? (the specifier is computed: ./pages/*/index)",
  ]);
  const noLocales = rule("the locales are not loaded").files(anyOf("src/pattern.ts")).mustNotImport(anyOf("src/locales/**"));
  expect(noLocales.verify(app)?.message).toBe(
    "rule `the locales are not loaded` is violated in 2 place(s):\n" +
      "  src/pattern.ts:1:39: [the locales are not loaded] src/pattern.ts uses src/locales/de.ts (dynamic-import, candidate, the specifier is computed: ./locales/*.ts)\n" +
      "  src/pattern.ts:1:39: [the locales are not loaded] src/pattern.ts uses src/locales/en.ts (dynamic-import, candidate, the specifier is computed: ./locales/*.ts)",
  );
  noLocales.ignoringCandidates().check(app);
  expect(violations(rule("no plugins").files(all()).mustNotImport(anyOf(pkg("plugins")))).length).toBe(1);
});

it("a specifier that is not known is a marker and no import", () => {
  expect(found("src/unknown.ts")).toEqual([
    "2:43 !computed-import",
    // The parameter is not the constant of the same name.
    "3:43 !computed-import",
    // The name of the package is not known.
    "4:46 !computed-import (*/sub)",
    // A variable that is not `const` can change.
    "6:33 !computed-import",
  ]);
  const written = rule("every import is written out").files(anyOf("src/unknown.ts", "src/constant.ts")).mustNotContain("computed-import");
  expect(violations(written).map((found) => found.line)).toEqual([2, 3, 4, 6]);
  expect(written.verify(app)?.message.split("\n")[1]).toBe("  src/unknown.ts:2:43: [every import is written out] src/unknown.ts contains computed-import");
});

it("a place that asks where a module is refers to it", () => {
  expect(found("src/places.ts").filter((text) => !text.includes("global:"))).toEqual([
    "1:23 resolve src/worker.ts",
    // A file that is no module of the project.
    "2:22 resolve src/logo.png",
    "3:22 resolve left-pad",
    // The fourth is not relative to this file.
  ]);
  expect(found("src/places.cjs")).toEqual(["1:18 resolve src/worker.ts"]);
  const failure = rule("only the pool starts the worker").files(all()).mustNotImport(anyOf("src/worker.ts")).onlyKinds("resolve");
  expect(violations(failure).map((found) => found.file)).toEqual(["src/places.cjs", "src/places.ts"]);
});

it("import.meta.glob imports each file that its pattern matches", () => {
  expect(found("src/glob.ts")).toEqual([
    "1:41 dynamic-import src/locales/de.ts (matches ./locales/*.ts)",
    "1:41 dynamic-import src/locales/en.ts (matches ./locales/*.ts)",
    "2:40 dynamic-import src/pages/admin/index.ts (matches ./pages/**/*.ts)",
    "2:40 dynamic-import src/pages/home.ts (matches ./pages/**/*.ts)",
    // One `*` stays in its directory.
    "3:37 dynamic-import src/pages/home.ts (matches ./pages/*.ts)",
  ]);
});
