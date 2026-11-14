// Loading: imports that the compiler does not resolve, and reading tsconfig files.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, compilerVersion, loadProject, pkg, rule, unverified } from "../src/index.ts";
import { parseJsonWithComments } from "../src/frontend/project.ts";
import { checks } from "../src/frontend/checks.ts";

// The cross-checks are on in this file: each short way of the front end is compared with the
// long way for every project loaded here, and a difference is an error.
checks.aliases = true;
checks.walk = true;

const dir = scratchDirectory("refpact-loading-");

function write(file: string, text: string): void {
  mkdirSync(path.dirname(path.join(dir, file)), { recursive: true });
  writeFileSync(path.join(dir, file), text);
}

{
  write("tsconfig.json", '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }');
  write("src/main.ts", 'import { gone } from "./missing";\nimport { untyped } from "untyped-package";\nimport fs from "node:fs";\nexport const all = [gone, untyped, fs];\n');
  // A package with types inside the root: its files are not files of the project.
  write("node_modules/typed-package/package.json", '{ "name": "typed-package", "types": "index.d.ts" }');
  write("node_modules/typed-package/index.d.ts", "export declare const typed: number;\n");
  write("src/other.ts", 'import { typed } from "typed-package";\nexport const value = typed;\n');
  const project = loadProject(path.join(dir, "tsconfig.json"));

  it("imports that the compiler does not resolve", () => {
    // a package without types is still that package
    {
      const result = rule("r").files(all()).mustNotImport(anyOf(pkg("untyped-package"), pkg("node:fs"))).evaluate(project);
      expect(result).toMatchObject({ ok: true, violations: [{ line: 2 }, { line: 3 }] });
    }

    // a relative import of a file that is not there is reported under its path, with a note
    {
      const result = rule("r").files(all()).mustNotImport(unverified("src/missing")).evaluate(project);
      expect(result).toMatchObject({ ok: true, violations: [{ line: 1, note: "not resolved" }] });
    }

    // a ready-made rule finds it, and not the package without types
    {
      const failure = rule("every import resolves").files(all()).mustNotContain("unresolved-import").verify(project);
      expect(failure?.message).toBe(
        "rule `every import resolves` is violated in 1 place(s):\n  src/main.ts:1:22: [every import resolves] src/main.ts contains unresolved-import (./missing)",
      );
    }
  });
}

it("tsconfig files", () => {
  // are read with comments and trailing commas
  {
    const text = '{\n  // a comment\n  "a": "// not a comment", /* block */\n  "b": [1, 2,],\n}';
    expect(parseJsonWithComments(text, "x.json")).toEqual({ a: "// not a comment", b: [1, 2] });
  }

  // that cannot be read are an error
  {
    expect(() => parseJsonWithComments("{ nope", "x.json")).toThrow(/cannot read x.json/);
  }

  // can be named by their directory
  {
    expect(loadProject(dir).files).toEqual(["src/main.ts", "src/other.ts"]);
  }
});

{
  // Each test script names the compiler it wants. This fails if another one is loaded.
  const wanted: Record<string, string> = { typescript: "7.0.", "typescript-6": "6.0.", "typescript-71": "7.1." };

  it("the compiler", () => {
    // is the version that the test run asked for
    {
      expect(compilerVersion.startsWith(wanted[process.env["REFPACT_TYPESCRIPT"] || "typescript"] as string)).toBe(true);
    }
  });
}

// TypeScript 7 removed `baseUrl` and does not resolve with it. TypeScript 6 resolves with it.
{
  const base = scratchDirectory("refpact-baseurl-");
  mkdirSync(path.join(base, "src"), { recursive: true });
  writeFileSync(path.join(base, "tsconfig.json"), '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [], "baseUrl": "./src" }, "include": ["src"] }');
  writeFileSync(path.join(base, "src/page.ts"), "export const page = 1;\n");
  writeFileSync(path.join(base, "src/main.ts"), 'import { page } from "page";\nexport const main = page;\n');

  it("baseUrl", () => {
    // is an error with TypeScript 7
    if (!compilerVersion.startsWith("6.")) {
      expect(() => loadProject(base)).toThrow(/sets baseUrl, which TypeScript 7 removed/);
      return;
    }

    // leads to the file with TypeScript 6
    {
      const targets = loadProject(base).facts.facts.map((fact) => (fact.what.type === "reference" ? fact.what.target.join("/") : ""));
      expect(targets).toEqual(["src/page.ts/#page"]);
    }
  });
}

// The first step from an imported name to its export is read from the exports of the module
// and not asked of the compiler for each name. The test setup compares the two for every
// name, so each load here is also that comparison, on modules where it is most likely to fail.
{
  const cjs = scratchDirectory("refpact-cjs-");
  const put = (file: string, text: string): void => {
    mkdirSync(path.dirname(path.join(cjs, file)), { recursive: true });
    writeFileSync(path.join(cjs, file), text);
  };
  put("tsconfig.json", '{ "compilerOptions": { "module": "nodenext", "noEmit": true, "types": [], "allowJs": true, "strict": true }, "include": ["src"] }');
  put("src/space.cts", "namespace box { export const inner = 1; }\nexport = box;\n");
  put("src/object.cts", "const box = { prop: 1 };\nexport = box;\n");
  put("src/class.cts", "class Thing { static made = 1; }\nnamespace Thing { export const extra = 2; export type T = number; }\nexport = Thing;\n");
  put("src/function.cts", "function build() { return 1; }\nbuild.attached = 2;\nexport = build;\n");
  put("src/script.cjs", "module.exports = { made: 1 };\n");
  put("src/both.cts", "export const named = 1;\nconst value = { named: 2 };\nexport default value;\n");
  put("src/esm.mts", "export const plain = 1;\nexport default plain;\n");
  put(
    "src/main.mts",
    [
      'import { inner } from "./space.cjs";',
      'import { prop } from "./object.cjs";',
      'import { extra, made, type T } from "./class.cjs";',
      'import { attached } from "./function.cjs";',
      'import { made as scripted } from "./script.cjs";',
      'import { default as whole, named } from "./both.cjs";',
      'import { default as value, plain } from "./esm.mjs";',
      "export const all: T[] = [inner, prop, extra, made, attached, scripted, whole.default.named, named, value, plain];",
      "",
    ].join("\n"),
  );
  put("src/two.mts", "export const first = 1;\nexport const second = 2;\n");
  put("src/renamed.mts", 'import { first as second } from "./two.mjs";\nexport const all = [second];\n');
  put("src/main.cts", 'import { default as value, named } from "./both.cjs";\nexport const all = [value, named];\n');

  const targets = (file: string): string[] =>
    loadProject(cjs)
      .facts.facts.filter((fact) => fact.file === file && fact.what.type === "reference")
      .map((fact) => (fact.what.type === "reference" ? `${fact.line} ${fact.what.target.join("/")}` : ""));


  // The module also exports the local name, so a lookup by the local name finds an export.

  // In an ES module, the default import of a CommonJS module is the module as a whole, and
  // not the export with the name `default`.

  it("names of CommonJS modules", () => {
    // with `export =` are the names that the compiler gives
    {
      expect(targets("src/main.mts").slice(0, 7)).toEqual([
        "1 src/space.cts/#inner",
        "2 src/object.cts/#prop",
        "3 src/class.cts/#extra",
        "3 src/class.cts/#made",
        "3 src/class.cts/#T",
        "4 src/function.cts/#attached",
        "5 src/script.cjs/#made",
      ]);
    }

    // a renamed import is the export with the first name
    {
      expect(targets("src/renamed.mts")).toEqual(["1 src/two.mts/#first"]);
    }

    // a default import of a CommonJS module is the answer of the compiler
    {
      expect(targets("src/main.mts").slice(7)).toEqual([
        "6 src/both.cts/#default",
        "6 src/both.cts/#named",
        "7 src/esm.mts/#default",
        "7 src/esm.mts/#plain",
      ]);
      expect(targets("src/main.cts")).toEqual(["1 src/both.cts/#default", "1 src/both.cts/#named"]);
    }
  });
}

// A node whose text has no word that can name a module is not visited. The test setup also
// visits all nodes and compares, so each load here is that comparison, on files where a
// module is named far from the top level or in an unusual way.
{
  const deep = scratchDirectory("refpact-deep-");
  const put = (file: string, text: string): void => {
    mkdirSync(path.dirname(path.join(deep, file)), { recursive: true });
    writeFileSync(path.join(deep, file), text);
  };
  put("tsconfig.json", '{ "compilerOptions": { "module": "nodenext", "noEmit": true, "types": [], "allowJs": true, "checkJs": true, "strict": true }, "include": ["src"] }');
  put("src/lib.ts", "export const a = 1;\nexport type T = number;\nexport const b = 2;\n");
  put(
    "src/inner.ts",
    [
      "// “ünïcödé” 日本語 🎉: the positions count in the text, also after such characters",
      "export function load() {",
      "  if (Math.random() > 2) {",
      '    return import("./lib.ts");',
      "  }",
      '  const inner: import("./lib.ts").T = 1;',
      "  return inner;",
      "}",
      "",
    ].join("\n"),
  );
  put("src/required.cts", 'export function load() {\n  return [1].map(() => require("./lib.ts"));\n}\n');
  put("src/ambient.d.ts", 'declare module "made-up" {\n  export { a } from "./lib.ts";\n}\n');
  put("src/escape.cts", 'export function load() {\n  return r\\u0065quire("./lib.ts");\n}\n');
  put(
    "src/space.ts",
    [
      'import * as lib from "./lib.ts";',
      "// 日本語 🎉",
      "export function load() {",
      "  return [1].map(() => {",
      "    const value: lib.T = lib.a;",
      "    return value + lib",
      "      .b;",
      "  });",
      "}",
      "",
    ].join("\n"),
  );
  put("src/button.tsx", "export function Button(props: { label: number }) {\n  return <button>{props.label}</button>;\n}\n");
  put(
    "src/view.tsx",
    [
      'import { Button } from "./button.tsx";',
      'import * as lib from "./lib.ts";',
      "export function View() {",
      "  return (",
      "    <section>",
      "      <Button label={lib.a} />",
      '      {[1].map(() => import("./lib.ts"))}',
      "    </section>",
      "  );",
      "}",
      "",
    ].join("\n"),
  );
  put("src/doc.js", '/** @typedef {import("./lib.ts").T} Alias */\nconst first = 1;\n/** @type {import("./lib.ts").T} */\nconst second = 2;\nexport { first, second };\n');

  const project = loadProject(deep);
  const targets = (file: string): string[] =>
    project.facts.facts
      .filter((fact) => fact.file === file)
      .map((fact) => (fact.what.type === "reference" ? `${fact.line}:${fact.column} ${fact.what.target.join("/")}` : ""));






  // The native compiler makes nodes from the types in JSDoc comments.

  it("modules that are named below the top level", () => {
    // in a function: a dynamic import and an import type
    {
      expect(targets("src/inner.ts")).toEqual(["4:12 src/lib.ts", "6:35 src/lib.ts/#T"]);
    }

    // in a function: a require call
    {
      expect(targets("src/required.cts")).toEqual(["2:24 src/lib.ts"]);
    }

    // in a declared module: a re-export
    {
      expect(targets("src/ambient.d.ts")).toEqual(["2:12 src/lib.ts/#a"]);
    }

    // with an escape in the name `require`
    {
      expect(targets("src/escape.cts")).toEqual(["2:10 src/lib.ts"]);
    }

    // as members of a namespace import
    {
      expect(targets("src/space.ts")).toEqual(["1:13 src/lib.ts", "5:22 src/lib.ts/#T", "5:30 src/lib.ts/#a", "7:8 src/lib.ts/#b"]);
    }

    // in the JSX of a React component
    {
      expect(targets("src/view.tsx")).toEqual(["1:10 src/button.tsx/#Button", "2:13 src/lib.ts", "6:26 src/lib.ts/#a", "7:22 src/lib.ts"]);
    }

    // in the JSDoc comments of a JavaScript file
    {
      expect(targets("src/doc.js")).toEqual(["1:34 src/lib.ts/#T", "3:31 src/lib.ts/#T"]);
    }
  });
}
