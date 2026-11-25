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
  "src/api/handler.ts": [
    'import { Order } from "../domain/order";',
    'import { pool } from "../db/repository";',
    'import type { Repository } from "../domain/port";',
    "/** Handles orders. */",
    "export class Handler {",
    "  private readonly limit: number = 1;",
    "  #seen = 0;",
    "  constructor(protected store: Repository) {}",
    "  async handle(order: Order): Promise<Order> {",
    "    return pool > this.limit ? order : new Order();",
    "  }",
    "  size(): number;",
    "  size(unit: string): number;",
    "  size(unit?: string): number {",
    "    return unit === undefined ? this.#seen : 1;",
    "  }",
    "}",
    "export function open(): number {",
    "  const pool = 2;",
    "  return pool;",
    "}",
    "export interface Saved { readonly id: string; readonly port: Repository; save(order: Order): void }",
  ],
  "src/log.ts": ["export function log(target: unknown): void {}", "export type Level = string;", "export enum Color { Red }", "namespace Space {}"],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}
const shop = loadProject(path.join(root, "tsconfig.json"));
// The same project, with the uses of the imported names.
const used = loadProject(path.join(root, "tsconfig.json"), { resolveUses: true });
const render = (path: readonly string[]): string => printPath(path, shop.facts);

/** The place of each violation of a rule about imports, with the item that the import is in. */
function imports(made: ImportRule, project = used): string[] {
  const result = made.evaluate(project);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations.map((violation: Violation) => {
    const item = violation.detail.type === "uses" && violation.detail.item !== undefined ? ` in ${render(violation.detail.item)}` : "";
    return `${violation.file}:${violation.line}:${violation.column}${item}`;
  });
}

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
    "src/api/handler.ts#Handler class exported 5:14 []",
    "src/api/handler.ts#Handler#limit field private 6:20 []",
    "src/api/handler.ts#Handler##seen field private 7:3 []",
    "src/api/handler.ts#Handler#constructor method public 8:3 [takes src/domain/port.ts#Repository]",
    "src/api/handler.ts#Handler#store field protected 8:25 [has-type src/domain/port.ts#Repository]",
    "src/api/handler.ts#Handler#handle method public 9:9 [returns src/domain/order.ts#Order, takes src/domain/order.ts#Order]",
    "src/api/handler.ts#Handler#size method public 14:3 []",
    "src/api/handler.ts#open function exported 18:17 []",
    "src/api/handler.ts#Saved interface exported 22:18 []",
    "src/api/handler.ts#Saved#id field public 22:35 []",
    "src/api/handler.ts#Saved#port field public 22:56 [has-type src/domain/port.ts#Repository]",
    "src/api/handler.ts#Saved#save method public 22:74 [takes src/domain/order.ts#Order]",
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
    "src/api/handler.ts#Handler",
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

it("the fields and the methods of classes and interfaces are items", () => {
  // A selection holds the members only when it names their kinds.
  expect(items(rule("nothing in the api").items(anyOf("src/api/**")).mustNotExist())).toEqual(["src/api/handler.ts#Handler", "src/api/handler.ts#open", "src/api/handler.ts#Saved"]);
  const members = () => rule("members").items(anyOf("src/api/**")).ofKind("field", "method");
  expect(items(members().mustNotExist())).toEqual([
    "src/api/handler.ts#Handler#limit",
    // A private name has its `#`.
    "src/api/handler.ts#Handler##seen",
    "src/api/handler.ts#Handler#constructor",
    // A parameter of the constructor that declares a field.
    "src/api/handler.ts#Handler#store",
    "src/api/handler.ts#Handler#handle",
    "src/api/handler.ts#Handler#size",
    "src/api/handler.ts#Saved#id",
    "src/api/handler.ts#Saved#port",
    "src/api/handler.ts#Saved#save",
  ]);
  expect(items(members().withVisibility("private", "protected").mustNotExist())).toEqual(["src/api/handler.ts#Handler#limit", "src/api/handler.ts#Handler##seen", "src/api/handler.ts#Handler#store"]);
  expect(items(members().taking(anyOf("src/domain/order.ts#Order")).mustNotExist())).toEqual(["src/api/handler.ts#Handler#handle", "src/api/handler.ts#Saved#save"]);
  expect(items(members().ofType(anyOf("src/domain/port.ts#Repository")).mustNotExist())).toEqual(["src/api/handler.ts#Handler#store", "src/api/handler.ts#Saved#port"]);
  expect(items(members().withModifier("async").mustNotExist())).toEqual(["src/api/handler.ts#Handler#handle"]);
  // An overloaded method is one method, with the lines of its body.
  const long = rule("short methods").items(anyOf("src/api/**")).ofKind("method").mustNotExceedLines(2).evaluate(shop);
  expect(long.ok && long.violations.map((violation) => violation.line)).toEqual([9, 14]);
});

it("what the code of an item imports", () => {
  const handlers = () => rule("the handlers do not use the database").items(anyOf("src/api/**"));
  expect(imports(handlers().named("Handler").mustNotImport(anyOf("src/db/**")))).toEqual(["src/api/handler.ts:10:12 in src/api/handler.ts#Handler#handle"]);
  expect(imports(handlers().ofKind("method").mustOnlyImport(anyOf("src/domain/order.ts")))).toEqual(["src/api/handler.ts:10:12 in src/api/handler.ts#Handler#handle"]);
  // A variable with the name of an import is no use of the import.
  expect(imports(handlers().named("open").mustNotImport(anyOf("src/db/**")))).toEqual([]);
  // A rule about files reports the import and each use of its name.
  expect(imports(rule("the api does not import the database").files(anyOf("src/api/**")).mustNotImport(anyOf("src/db/**")))).toEqual([
    "src/api/handler.ts:2:10",
    "src/api/handler.ts:10:12",
  ]);
  expect(imports(rule("the api does not import the database").files(anyOf("src/api/**")).mustNotImport(anyOf("src/db/**")), shop)).toEqual(["src/api/handler.ts:2:10"]);
  // Without the option, no reference says which item it is in: the rule cannot fail.
  const unread = handlers().mustNotImport(anyOf("src/db/**")).evaluate(shop);
  expect(!unread.ok && unread.errors.map((error) => error.code)).toEqual(["no-subjects"]);
});
