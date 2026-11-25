// The types of items: what a function takes and returns, what a variable has, and what a type
// is assignable to.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, loadProject, pkg, rule } from "../src/index.ts";
import type { ImportRule, Violation } from "../src/index.ts";

const root = path.join(scratchDirectory("refpact-types-"), "shop");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }'],
  "src/domain/entity.ts": ["export interface Saved {}", "export class Entity implements Saved {}"],
  "src/domain/order.ts": ['import { Entity } from "./entity";', "export class Order extends Entity {}", "export class Special extends Order {}", "export interface Total { readonly value: number }"],
  "src/domain/index.ts": ['export { Order } from "./order";'],
  "src/db/pool.ts": ["export class Pool {}", "export const pool: Pool = new Pool();", "export const size = 3;"],
  "src/app/service.ts": [
    'import { Order } from "../domain";',
    'import type { Total } from "../domain/order";',
    'import * as db from "../db/pool";',
    'import type { Readable } from "node:stream";',
    "export function total(order: Order, pool: db.Pool): Promise<Total> { return Promise.resolve({ value: 1 }); }",
    "export function first<T>(items: T[]): T | undefined { return items[0]; }",
    "export function bind(this: Order): void {}",
    "export function read(stream: Readable): typeof db.pool { return db.pool; }",
    'export function load(): import("../db/pool").Pool { return new db.Pool(); }',
    "export const make = (order: Order): Total => ({ value: 2 });",
    "export const handler = function (orders: readonly Order[]): void {};",
    "export const config: { readonly pool: db.Pool } = { pool: new db.Pool() };",
    "function untyped(order) { return order; }",
  ],
  "src/cycle.ts": ["interface A extends B {}", "interface B extends A {}"],
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

function relations(file: string): string[] {
  return (shop.facts.declarations ?? [])
    .filter((declaration) => declaration.file === file)
    .map((declaration) => {
      const ties = declaration.relations.map((tie) => `${tie.relation} ${render(tie.target)}`).sort();
      return `${declaration.name} [${ties.join(", ")}]`;
    });
}

it("the types that functions and variables take, return and have", () => {
  expect(relations("src/app/service.ts")).toEqual([
    // An imported name is at the import and at its declaration, as for `extends`. A global
    // type such as `Promise` is no path.
    "total [returns src/domain/order.ts#Total, takes src/db/pool.ts#Pool, takes src/domain/index.ts#Order, takes src/domain/order.ts#Order]",
    "first []",
    "bind []",
    "read [returns src/db/pool.ts#pool, takes node:stream#Readable]",
    "load [returns src/db/pool.ts#Pool]",
    "make [returns src/domain/order.ts#Total, takes src/domain/index.ts#Order, takes src/domain/order.ts#Order]",
    "handler [takes src/domain/index.ts#Order, takes src/domain/order.ts#Order]",
    "config [has-type src/db/pool.ts#Pool]",
    "untyped []",
  ]);
  expect(relations("src/db/pool.ts")).toEqual(["Pool []", "pool [has-type src/db/pool.ts#Pool]", "size []"]);
});

it("what a type is assignable to", () => {
  expect(relations("src/domain/order.ts")).toEqual([
    "Order [assignable-to src/domain/entity.ts#Entity, assignable-to src/domain/entity.ts#Saved, extends src/domain/entity.ts#Entity]",
    "Special [assignable-to src/domain/entity.ts#Entity, assignable-to src/domain/entity.ts#Saved, assignable-to src/domain/order.ts#Order, extends src/domain/order.ts#Order]",
    "Total []",
    // The field of the interface `Total`.
    "value []",
  ]);
  // A cycle ends, and a type is not assignable to itself.
  expect(relations("src/cycle.ts")).toEqual([
    "A [assignable-to src/cycle.ts#B, extends src/cycle.ts#B]",
    "B [assignable-to src/cycle.ts#A, extends src/cycle.ts#A]",
  ]);
});

it("rules on the types of items", () => {
  const saved = rule("saved things live in the domain").items(all()).assignableTo(anyOf("src/domain/entity.ts#Saved"));
  expect(items(saved.mustResideIn(anyOf("src/db/**")))).toEqual([
    "src/domain/entity.ts#Entity",
    "src/domain/order.ts#Order",
    "src/domain/order.ts#Special",
  ]);
  rule("orders are saved").items(anyOf("src/domain/order.ts")).ofKind("class").mustBeAssignableTo(anyOf("src/domain/entity.ts#Saved")).check(shop);
  expect(items(rule("no streams in the app").items(anyOf("src/app/**")).taking(anyOf(pkg("node:stream"))).mustNotExist())).toEqual(["src/app/service.ts#read"]);
  expect(items(rule("orders come from the domain").items(all()).taking(anyOf("src/domain/order.ts#Order")).mustNotExist())).toEqual([
    "src/app/service.ts#total",
    "src/app/service.ts#make",
    "src/app/service.ts#handler",
  ]);
  expect(items(rule("totals").items(all()).returning(anyOf("src/domain/order.ts#Total")).mustBeNamed("make"))).toEqual(["src/app/service.ts#total"]);
  expect(items(rule("pools").items(all()).ofType(anyOf("src/db/**")).mustNotExist())).toEqual(["src/app/service.ts#config", "src/db/pool.ts#pool"]);
  const typo = rule("typo").items(all()).returning(anyOf("src/domain/order.ts#Totl")).mustNotExist().evaluate(shop);
  expect(!typo.ok && typo.errors.map((error) => [error.code, error.role])).toEqual([["pattern-matches-nothing", "related"]]);
});
