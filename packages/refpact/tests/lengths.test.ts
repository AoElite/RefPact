// The rules about lengths: the lines of files and of items, as the front end counts them.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, loadProject, rule } from "../src/index.ts";
import type { ImportRule } from "../src/index.ts";

const root = path.join(scratchDirectory("refpact-lengths-"), "shop");
const files: Record<string, string> = {
  "tsconfig.json": '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }\n',
  // 12 lines, with a line break at the end.
  "src/api.ts": [
    "/** Serves. */",
    "export function serve(",
    "  port: number,",
    "): number {",
    "  return port;",
    "}",
    "",
    "export class Config {",
    "  port = 1;",
    "}",
    "export const one = 1,",
    "  handler = (): number => {",
    "    return 2;",
    "  };",
    "export type Id = string;",
    "",
  ].join("\n"),
  // 2 lines, without a line break at the end.
  "src/short.ts": "export const a = 1;\nexport const b = 2;",
  "src/empty.ts": "",
};
for (const [file, text] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), text);
}
const shop = loadProject(path.join(root, "tsconfig.json"));

/** The lines that a rule reports, each with the file or the item. */
function lengths(made: ImportRule): string[] {
  const result = made.evaluate(shop);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations.map((violation) => {
    if (violation.detail.type !== "too-long") {
      throw new Error(`unexpected ${violation.detail.type}`);
    }
    return `${printPath(violation.detail.item ?? violation.unit, shop.facts)} ${violation.detail.lines}`;
  });
}

it("the lines of a file are counted, and a line break at the end starts no line", () => {
  expect(lengths(rule("r").files(all()).mustNotExceedLines(1))).toEqual(["src/api.ts 15", "src/short.ts 2"]);
  expect(shop.facts.unitLines).toEqual([15, 0, 2]);
  // an empty file has no import and no item: a violation about it has its file
  const placed = rule("the empty file is the api").files(anyOf("src/empty.ts")).mustBeIn(["src/api.ts"]).evaluate(shop);
  expect(placed.ok && placed.violations.map((violation) => violation.file)).toEqual(["src/empty.ts"]);
});

it("a file at the limit passes, and a longer file is reported", () => {
  rule("r").files(anyOf("src/api.ts")).mustNotExceedLines(15).check(shop);
  expect(() => rule("files are short").files(anyOf("src/api.ts")).mustNotExceedLines(14).check(shop)).toThrow(
    "  src/api.ts: [files are short] src/api.ts has 15 lines, and the limit is 14",
  );
});

it("the lines of an item run from its name to the end of its declaration", () => {
  expect(lengths(rule("r").items(anyOf("src/api.ts")).mustNotExceedLines(0))).toEqual([
    "src/api.ts#serve 5",
    "src/api.ts#Config 3",
    "src/api.ts#one 1",
    "src/api.ts#handler 3",
    "src/api.ts#Id 1",
  ]);
});

it("a rule about long functions reports the function at its name", () => {
  expect(() => rule("functions are short").items(all()).ofKind("function").mustNotExceedLines(4).check(shop)).toThrow(
    "src/api.ts:2:17: [functions are short] src/api.ts declares function src/api.ts#serve, which has 5 lines, and the limit is 4",
  );
  rule("functions are short").items(all()).ofKind("function").mustNotExceedLines(5).check(shop);
});
