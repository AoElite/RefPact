// `followImplementations`: a use of a member of an interface or of a class is also a candidate
// use of the member of each class that implements or extends it.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { anyOf, loadProject, rule } from "../src/index.ts";
import type { Project } from "../src/index.ts";

const root = scratchDirectory("refpact-implementations-");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [], "strict": true }, "include": ["src"] }'],
  "src/domain/store.ts": ["export interface Store {", "  save(order: number): void;", "}", "export class Base {", "  close(): void {}", "}"],
  "src/domain/service.ts": [
    'import type { Base, Store } from "./store";',
    "export function place(store: Store, base: Base): void {",
    "  store.save(1);",
    "  base.close();",
    "}",
  ],
  "src/infra/pg.ts": [
    'import { Base } from "../domain/store";',
    'import type { Store } from "../domain/store";',
    "export class PgStore extends Base implements Store {",
    "  save(order: number): void {",
    "    void order;",
    "  }",
    "}",
    "export class Cached extends PgStore {}",
  ],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}

function members(project: Project): string[] {
  return project.facts.facts.flatMap((fact) => {
    if (fact.file !== "src/domain/service.ts" || fact.what.type !== "reference" || fact.what.kind !== "member") {
      return [];
    }
    const sure = fact.what.certainty === "candidate" ? " ?" : "";
    const note = fact.note !== undefined ? ` (${fact.note})` : "";
    return [`${fact.line}:${fact.column} ${printPath(fact.what.target, project.facts)}${sure}${note}`];
  });
}

it("a use of a member is a candidate use of the member of each class below", () => {
  const followed = loadProject(root, { followImplementations: true });
  expect(members(followed)).toEqual([
    "3:9 src/domain/store.ts#Store#save",
    "3:9 src/infra/pg.ts#PgStore#save ? (may be called through Store.save)",
    // A class that extends a class that implements the interface.
    "3:9 src/infra/pg.ts#Cached#save ? (may be called through Store.save)",
    "4:8 src/domain/store.ts#Base#close",
    "4:8 src/infra/pg.ts#PgStore#close ? (may be called through Base.close)",
    "4:8 src/infra/pg.ts#Cached#close ? (may be called through Base.close)",
  ]);

  const inward = rule("the domain does not use the infrastructure").files(anyOf("src/domain/**")).mustNotImport(anyOf("src/infra/**"));
  expect(inward.verify(followed)?.message.split("\n")[1]).toBe(
    "  src/domain/service.ts:3:9: [the domain does not use the infrastructure] src/domain/service.ts uses src/infra/pg.ts#PgStore#save (member, candidate, may be called through Store.save)",
  );
  inward.ignoringCandidates().check(followed);
});

it("it is not the default, also with resolveMembers", () => {
  const plain = loadProject(root, { resolveMembers: true });
  expect(members(plain)).toEqual(["3:9 src/domain/store.ts#Store#save", "4:8 src/domain/store.ts#Base#close"]);
  rule("the domain does not use the infrastructure").files(anyOf("src/domain/**")).mustNotImport(anyOf("src/infra/**")).check(plain);
});
