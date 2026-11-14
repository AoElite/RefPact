// Markers, rules about them, the onion architecture and the reasons of rules.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, loadProject, onionArchitecture, rule, rules } from "../src/index.ts";
import type { Failure } from "../src/index.ts";

const root = path.join(scratchDirectory("refpact-presets-"), "shop");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }'],
  "src/model.ts": ['import { price } from "./service";', "export class Order {}", "export const total = price;"],
  "src/service.ts": ['import { Order } from "./model";', "export function price(): Order {", '  console.log("pricing");', "  return new Order();", "}"],
  "src/application.ts": ['import { price } from "./service";', "export const checkout = price;"],
  "src/web.ts": ['import { checkout } from "./application";', 'import { save } from "./store";', "export function get(): void {", "  debugger;", "  checkout();", "  save();", "}"],
  "src/store.ts": [
    'import { get } from "./web";',
    "export function save(): void {",
    "  const console = { log: (text: string) => text };",
    '  console.log("saved");',
    "}",
    "export const again = get;",
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}
const shop = loadProject(path.join(root, "tsconfig.json"));

function message(failure: Failure | undefined): string {
  if (failure === undefined) {
    throw new Error("the rule passes");
  }
  return failure.message;
}

it("the global console and debugger statements are markers", () => {
  const markers = shop.facts.facts.flatMap((fact) => (fact.what.type === "marker" ? [`${fact.file}:${fact.line}:${fact.column} ${fact.what.marker}`] : []));
  // The `console` of `store.ts` is a local variable.
  expect(markers).toEqual(["src/service.ts:3:3 console", "src/web.ts:4:3 debugger"]);
  expect(shop.facts.markers).toEqual(["console", "debugger", "unresolved-import", "computed-import"]);
});

it("the conventions of the code are rules about markers", () => {
  const noConsole = rule("no use of the console").files(all()).mustNotContain("console").because("output belongs to the code that owns it");
  const noDebugger = rule("no debugger statement").files(all()).mustNotContain("debugger");
  expect(message(rules("the conventions of the code", noConsole, noDebugger).verify(shop))).toBe(
    [
      "rule `no use of the console` is violated in 1 place(s):",
      "  because output belongs to the code that owns it",
      "  src/service.ts:3:3: [no use of the console] src/service.ts contains console",
      "rule `no debugger statement` is violated in 1 place(s):",
      "  src/web.ts:4:3: [no debugger statement] src/web.ts contains debugger",
    ].join("\n"),
  );
  expect(rule("no use of the console").files(anyOf("src/web.ts", "src/store.ts")).mustNotContain("console").verify(shop)).toBeUndefined();
});

it("the reason of a rule is in the report and in the failure", () => {
  const failure = rule("the model imports nothing")
    .files(anyOf("src/model.ts"))
    .mustNotImport(anyOf("src/service.ts"))
    .because("the model is the centre")
    .verify(shop);
  expect(failure).toMatchObject({ type: "violated", reason: "the model is the centre" });
  expect(message(failure)).toBe(
    [
      "rule `the model imports nothing` is violated in 1 place(s):",
      "  because the model is the centre",
      "  src/model.ts:1:10: [the model imports nothing] src/model.ts uses src/service.ts#price (import)",
    ].join("\n"),
  );
});

it("the onion architecture", () => {
  const onion = onionArchitecture("the shop")
    .domainModels("src/model.ts")
    .domainServices("src/service.ts")
    .applicationServices("src/application.ts")
    .adapter("web", "src/web.ts")
    .adapter("store", "src/store.ts")
    .because("the domain is the centre");
  expect(onion.rules().map((made) => made.name)).toEqual([
    "the shop: adapter web imports only the layers it may import",
    "the shop: adapter store does not import the layers above it",
    "the shop: application services does not import the layers above it",
    "the shop: domain services does not import the layers above it",
    "the shop: domain models does not import the layers above it",
  ]);
  // The adapters import each other, and the model imports a service.
  expect(message(onion.verify(shop))).toBe(
    [
      "rule `the shop: adapter web imports only the layers it may import` is violated in 1 place(s):",
      "  because the domain is the centre",
      "  src/web.ts:2:10: [the shop: adapter web imports only the layers it may import] src/web.ts uses src/store.ts#save (import)",
      "rule `the shop: adapter store does not import the layers above it` is violated in 1 place(s):",
      "  because the domain is the centre",
      "  src/store.ts:1:10: [the shop: adapter store does not import the layers above it] src/store.ts uses src/web.ts#get (import)",
      "rule `the shop: domain models does not import the layers above it` is violated in 1 place(s):",
      "  because the domain is the centre",
      "  src/model.ts:1:10: [the shop: domain models does not import the layers above it] src/model.ts uses src/service.ts#price (import)",
    ].join("\n"),
  );
  expect(onionArchitecture("one part").domainModels("src/model.ts").verify(shop)).toMatchObject({ type: "malformed" });
});
