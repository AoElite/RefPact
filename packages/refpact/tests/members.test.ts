// `resolveMembers`: the uses of members of classes, interfaces and enums, which the compiler resolves.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { anyOf, loadProject, rule } from "../src/index.ts";
import type { ImportRule, Project } from "../src/index.ts";

const root = path.join(scratchDirectory("refpact-members-"), "shop");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [], "strict": true }, "include": ["src"] }'],
  "src/domain/entity.ts": ["export class Entity {", "  id = 1;", "  save(): void {}", "}"],
  "src/domain/order.ts": [
    'import { Entity } from "./entity";',
    "export class Order extends Entity {",
    "  total = 0;",
    "  static fresh(): Order {",
    "    return new Order();",
    "  }",
    "  get price(): number {",
    "    return this.total;",
    "  }",
    "  add(amount: number): this {",
    "    this.total += amount;",
    "    return this;",
    "  }",
    "}",
    "export interface Priced {",
    "  readonly price: number;",
    "  describe(): string;",
    "}",
    "export enum Mode {",
    "  Fast,",
    "  Slow,",
    "}",
    "export function make(): Order {",
    "  return Order.fresh();",
    "}",
    "export const settings = { limit: 3 };",
  ],
  "src/ui/view.ts": [
    'import { make, Mode, settings } from "../domain/order";',
    'import type { Order, Priced } from "../domain/order";',
    'import * as domain from "../domain/order";',
    "export function render(order: Order, priced: Priced, box: { value: Order }): number {",
    "  order.add(1).add(2);",
    "  order.save();",
    "  const made = make();",
    "  const mode = Mode.Fast;",
    "  const label = priced.describe();",
    "  const items = [order].map((each) => each.id);",
    "  const local = { size: items.length + label.length + mode + settings.limit };",
    "  return made.total + order.price + priced.price + box.value.total + local.size + domain.make().total;",
    "}",
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}
const shop = loadProject(path.join(root, "tsconfig.json"), { resolveMembers: true });
const plain = loadProject(path.join(root, "tsconfig.json"));

/** The references of the kind `member` of a file: `line:column target`. */
function members(project: Project, file: string): string[] {
  return project.facts.facts.flatMap((fact) =>
    fact.file === file && fact.what.type === "reference" && fact.what.kind === "member"
      ? [`${fact.line}:${fact.column} ${printPath(fact.what.target, project.facts)}`]
      : [],
  );
}

function violations(made: ImportRule, project: Project = shop): string[] {
  const result = made.evaluate(project);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations.map((violation) =>
    violation.detail.type === "uses" ? `${violation.file}:${violation.line} ${printPath(violation.detail.target, project.facts)}` : "?",
  );
}

it("the uses of the members of another file are references", () => {
  expect(members(shop, "src/ui/view.ts")).toEqual([
    "5:9 src/domain/order.ts#Order#add",
    "5:16 src/domain/order.ts#Order#add",
    // A member that a class inherits is a member of the class that declares it.
    "6:9 src/domain/entity.ts#Entity#save",
    "8:21 src/domain/order.ts#Mode#Fast",
    "9:24 src/domain/order.ts#Priced#describe",
    "10:44 src/domain/entity.ts#Entity#id",
    "12:15 src/domain/order.ts#Order#total",
    "12:29 src/domain/order.ts#Order#price",
    "12:44 src/domain/order.ts#Priced#price",
    "12:62 src/domain/order.ts#Order#total",
    "12:97 src/domain/order.ts#Order#total",
  ]);
});

it("a member of the own file, of an object type and of the standard library is no reference", () => {
  // `this.total`, `Order.fresh()` and `new Order()` in the file of `Order`.
  expect(members(shop, "src/domain/order.ts")).toEqual([]);
  const targets = members(shop, "src/ui/view.ts").join(" ");
  for (const name of ["#value", "#size", "#limit", "#length", "#map", "#make"]) {
    expect(targets).not.toContain(name);
  }
});

it("a rule about imports sees the uses of members", () => {
  const found = violations(rule("the ui does not save").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/entity.ts")));
  expect(found).toEqual(["src/ui/view.ts:6 src/domain/entity.ts#Entity#save", "src/ui/view.ts:10 src/domain/entity.ts#Entity#id"]);
  // Two filters of kinds both hold, in each order: the later one does not take the place of the other.
  const filtered = () => rule("the ui does not save").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/entity.ts"));
  expect(violations(filtered().ignoringKinds("member").ignoringTypeImports())).toEqual([]);
  expect(violations(filtered().ignoringTypeImports().ignoringKinds("member"))).toEqual([]);
  expect(violations(filtered().ignoringKinds("member").onlyKinds("member", "import"))).toEqual([]);
  expect(violations(filtered().onlyKinds("import").onlyKinds("member", "import"))).toEqual([]);
  expect(violations(filtered().onlyKinds("member", "import").ignoringKinds("member"))).toEqual([]);
  expect(violations(filtered().onlyKinds("member", "import").ignoringKinds("import"))).toEqual(found);
  // The ui imports no name of `entity.ts`, so without the members the rule finds nothing.
  expect(violations(rule("the ui does not save").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/entity.ts")), plain)).toEqual([]);
});

it("a target names one member, or all members of an export", () => {
  const one = rule("the ui does not add to an order").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/order.ts#Order#add")).onlyKinds("member");
  expect(violations(one)).toEqual(["src/ui/view.ts:5 src/domain/order.ts#Order#add", "src/ui/view.ts:5 src/domain/order.ts#Order#add"]);
  const all = rule("the ui reads no price").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/order.ts#Priced")).onlyKinds("member");
  expect(violations(all)).toEqual(["src/ui/view.ts:9 src/domain/order.ts#Priced#describe", "src/ui/view.ts:12 src/domain/order.ts#Priced#price"]);
});

it("the report prints a member after its owner", () => {
  expect(() => rule("the ui does not save").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/entity.ts#Entity#save")).check(shop)).toThrow(
    "src/ui/view.ts:6:9: [the ui does not save] src/ui/view.ts uses src/domain/entity.ts#Entity#save (member)",
  );
});

it("a rule that names the kind is an error for a project without members", () => {
  const made = rule("r").files(anyOf("src/ui/**")).mustNotImport(anyOf("src/domain/**")).onlyKinds("member");
  const result = made.evaluate(plain);
  expect(result.ok ? [] : result.errors).toEqual([{ code: "unknown-kind", role: "kind", value: "member" }]);
});

it("the other facts are those of a load without members", () => {
  const others = (project: Project) => project.facts.facts.filter((fact) => fact.what.type !== "reference" || fact.what.kind !== "member");
  expect(others(shop)).toEqual(others(plain));
  expect(members(plain, "src/ui/view.ts")).toEqual([]);
  expect(plain.facts.kinds).not.toContain("member");
  expect(shop.facts.kinds).toContain("member");
});
