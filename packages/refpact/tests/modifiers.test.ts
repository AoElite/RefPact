// The modifiers of items: what a declaration says besides its kind and its visibility.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, loadProject, rule } from "../src/index.ts";
import type { ImportRule, Violation } from "../src/index.ts";

const root = path.join(scratchDirectory("refpact-modifiers-"), "shop");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }'],
  "src/api.ts": [
    "/** Serves one request. */",
    "export async function serve(): Promise<void> {}",
    "// A plain comment is no documentation.",
    "export function plain(): void {}",
    "export const load = async (): Promise<number> => 1;",
    "export let count = 0;",
    "/** The limit. */ export const LIMIT = 3, OTHER = 4;",
    "export abstract class Base {}",
    "export default class {}",
    "declare function external(): void;",
    "export const enum Color { Red }",
    "export enum Size { Small }",
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}
const shop = loadProject(path.join(root, "tsconfig.json"));
const render = (path: readonly string[]): string => printPath(path, shop.facts);

function items(made: ImportRule): string[] {
  const result = made.evaluate(shop);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations.map((violation: Violation) => (violation.detail.type === "item" ? render(violation.detail.item) : "?"));
}

it("the modifiers of the declarations", () => {
  const declared = (shop.facts.declarations ?? []).map((declaration) => `${declaration.name} [${declaration.modifiers.join(", ")}]`);
  expect(declared).toEqual([
    "serve [async, documented]",
    "plain []",
    "load [async, const]",
    "count []",
    // A comment before a statement documents each name of it.
    "LIMIT [const, documented]",
    "OTHER [const, documented]",
    "Base [abstract]",
    "default [default]",
    "external [declare]",
    "Color [const]",
    "Size []",
  ]);
  expect(shop.facts.modifiers).toEqual(["async", "abstract", "declare", "const", "default", "documented"]);
});

it("rules on the modifiers of items", () => {
  expect(items(rule("no async").items(all()).withModifier("async").mustNotExist())).toEqual(["src/api.ts#serve", "src/api.ts#load"]);
  expect(items(rule("no default exports").items(all()).withModifier("default").mustNotExist())).toEqual(["src/api.ts#default"]);
  expect(items(rule("exported functions are documented").items(all()).ofKind("function").withVisibility("exported").mustHaveModifier("documented"))).toEqual(["src/api.ts#plain"]);
  expect(items(rule("variables are constant").items(all()).ofKind("variable").mustHaveModifier("const"))).toEqual(["src/api.ts#count"]);
  const typo = rule("typo").items(all()).withModifier("asycn" as "async").mustNotExist().evaluate(shop);
  expect(!typo.ok && typo.errors.map((error) => [error.code, error.role, error.value])).toEqual([["unknown-modifier", "modifier", "asycn"]]);
});
