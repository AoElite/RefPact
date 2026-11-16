// Rules about items: what the front end declares, and the rules on top of it.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, allExcept, anyOf, loadProject, pkg, rule } from "../src/index.ts";
import type { ImportRule, Violation } from "../src/index.ts";

const root = path.join(scratchDirectory("refpact-items-"), "shop");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [], "experimentalDecorators": true }, "include": ["src"] }'],
  "src/domain/port.ts": ["export interface Repository {}", "export interface Port extends Repository {}"],
  "src/domain/order.ts": ["export class Entity {}", "export class Order extends Entity {}", "const LIMIT = 3;", "export { LIMIT as limit };"],
  "src/db/repository.ts": [
    'import { EventEmitter } from "node:events";',
    'import type { Repository } from "../domain/port";',
    'import { log } from "../log";',
    "@log",
    "export class SqlRepository extends EventEmitter implements Repository {}",
    "export class Cache implements Repository {}",
    "function open(): void {}",
    "export const pool = 1, size = 2;",
  ],
  "src/ui/view.ts": ['import * as port from "../domain/port";', "export class MemoryRepository implements port.Repository {}", "export default function render(): void {}", 'export class Inline implements import("../domain/port").Repository {}'],
  "src/log.ts": ["export function log(target: unknown): void {}", "export type Level = string;", "export enum Color { Red }", "namespace Space {}"],
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

it("the declarations of a project", () => {
  const declared = (shop.facts.declarations ?? []).map((declaration) => {
    const relations = declaration.relations.map((tie) => `${tie.relation} ${render(tie.target)}`).sort();
    return `${render(declaration.path)} ${declaration.kind} ${declaration.visibility} ${declaration.line}:${declaration.column} [${relations.join(", ")}]`;
  });
  expect(declared).toEqual([
    "src/db/repository.ts#SqlRepository class exported 5:14 [assignable-to node:events#EventEmitter, assignable-to src/domain/port.ts#Repository, decorator src/log.ts#log, extends node:events#EventEmitter, implements src/domain/port.ts#Repository]",
    "src/db/repository.ts#Cache class exported 6:14 [assignable-to src/domain/port.ts#Repository, implements src/domain/port.ts#Repository]",
    "src/db/repository.ts#open function local 7:10 []",
    "src/db/repository.ts#pool variable exported 8:14 []",
    "src/db/repository.ts#size variable exported 8:24 []",
    "src/domain/order.ts#Entity class exported 1:14 []",
    "src/domain/order.ts#Order class exported 2:14 [assignable-to src/domain/order.ts#Entity, extends src/domain/order.ts#Entity]",
    "src/domain/order.ts#LIMIT variable exported 3:7 []",
    "src/domain/port.ts#Repository interface exported 1:18 []",
    "src/domain/port.ts#Port interface exported 2:18 [assignable-to src/domain/port.ts#Repository, extends src/domain/port.ts#Repository]",
    "src/log.ts#log function exported 1:17 []",
    "src/log.ts#Level type exported 2:13 []",
    "src/log.ts#Color enum exported 3:13 []",
    "src/log.ts#Space namespace local 4:11 []",
    "src/ui/view.ts#MemoryRepository class exported 2:14 [assignable-to src/domain/port.ts#Repository, implements src/domain/port.ts#Repository]",
    "src/ui/view.ts#render function exported 3:25 []",
    "src/ui/view.ts#Inline class exported 4:14 [assignable-to src/domain/port.ts#Repository, implements src/domain/port.ts#Repository]",
  ]);
});

it("items that implement or extend", () => {
  const repositories = () => rule("repositories").items(all()).implementing(anyOf("src/domain/port.ts#Repository"));
  expect(items(repositories().mustResideIn(anyOf("src/db/**")))).toEqual(["src/ui/view.ts#MemoryRepository", "src/ui/view.ts#Inline"]);
  // `implements import("..").Repository` names the interface as an import of the type does.
  expect(items(repositories().mustBeNamed("*Repository"))).toEqual(["src/db/repository.ts#Cache", "src/ui/view.ts#Inline"]);
  expect(items(rule("emitters").items(all()).extending(anyOf(pkg("node:events"))).mustNotExist())).toEqual(["src/db/repository.ts#SqlRepository"]);
  expect(items(rule("decorated").items(all()).ofKind("class").mustBeDecoratedWith(anyOf("src/log.ts#log")))).toEqual([
    "src/db/repository.ts#Cache",
    "src/domain/order.ts#Entity",
    "src/domain/order.ts#Order",
    "src/ui/view.ts#MemoryRepository",
    "src/ui/view.ts#Inline",
  ]);
  rule("orders are entities").items(anyOf("src/domain/order.ts")).named("Order").mustExtend(anyOf("src/domain/order.ts#Entity")).check(shop);
  rule("classes in db implement a port").items(anyOf("src/db/**")).ofKind("class").mustImplement(anyOf("src/domain/port.ts")).check(shop);
});

it("visibility and errors", () => {
  expect(items(rule("nothing local in db").items(anyOf("src/db/**")).withVisibility("local").mustNotExist())).toEqual(["src/db/repository.ts#open"]);
  expect(items(rule("exports of the log").items(allExcept("src/db/**", "src/ui/**", "src/domain/**")).withVisibility("exported").ofKind("type", "enum").mustNotExist())).toEqual([
    "src/log.ts#Level",
    "src/log.ts#Color",
  ]);
  const typo = rule("typo").items(all()).implementing(anyOf("src/domain/port.ts#Repositry")).named("Nothing*").mustNotExist().evaluate(shop);
  expect(!typo.ok && typo.errors.map((error) => [error.code, error.role])).toEqual([
    ["pattern-matches-nothing", "name"],
    ["pattern-matches-nothing", "related"],
  ]);
  const message = rule("caches live in the domain").items(all()).named("Cache").mustResideIn(anyOf("src/domain/**")).verify(shop)?.message;
  expect(message).toContain("src/db/repository.ts:6:14: [caches live in the domain] src/db/repository.ts declares class src/db/repository.ts#Cache");
});
