// Layers, independent slices, files that nothing imports, chains, and the rules about
// `package.json`, on a project that the test writes.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { symlinkSync } from "node:fs";
import { all, allExcept, anyOf, ArchitectureError, baseline, layers, loadProject, onionArchitecture, pkg, rule, rules } from "../src/index.ts";
import type { AnyOf, Failure, ImportRule, Layers, Project, Violation } from "../src/index.ts";

const scratch = scratchDirectory("refpact-graph-");

function write(root: string, files: Readonly<Record<string, string | readonly string[]>>): void {
  for (const [file, text] of Object.entries(files)) {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), typeof text === "string" ? text : `${text.join("\n")}\n`);
  }
}

const TSCONFIG = '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }';

// The packages that `deps.ts` imports are not installed. A package is known by its name.
const root = path.join(scratch, "shop");
write(root, {
  // The manifest of the workspace root. It must not count for the files of `packages/app`.
  "package.json": '{ "name": "shop-root", "private": true, "devDependencies": { "vitest": "1" } }',
  "packages/app/package.json": JSON.stringify({
    name: "@shop/app",
    dependencies: { left: "1", "@scope/kit": "1" },
    peerDependencies: { peer: "1" },
    optionalDependencies: { maybe: "1" },
    // `left` is in both lists: a package that is also a dependency is not a development package.
    devDependencies: { left: "1", devtool: "1", "@types/estree": "1", "@types/scope__typed": "1" },
  }),
  "packages/app/tsconfig.json": TSCONFIG,
  // A manifest that only sets the module type is not the manifest of a package.
  "packages/app/src/package.json": '{ "type": "module" }',
  "packages/app/src/main.ts": ['import { serve } from "./ui/page";', "export const start = serve;"],
  "packages/app/src/ui/page.ts": [
    'import { load } from "../domain/order";',
    'import { total } from "../features/cart/index";',
    'import { route } from "../domain/price";',
    "export const serve = (): number => load() + total() + route.length;",
  ],
  "packages/app/src/ui/routes.ts": 'export const ROUTE = "/orders";\n',
  "packages/app/src/domain/order.ts": ['import { read } from "../db/disk";', "export const load = (): number => read();"],
  "packages/app/src/domain/price.ts": [
    'import type { serve } from "../ui/page";',
    'import { ROUTE } from "../ui/routes";',
    "export type Serve = typeof serve;",
    "export const route = ROUTE;",
  ],
  "packages/app/src/db/disk.ts": ['import { readFileSync } from "node:fs";', 'export const read = (): number => readFileSync("orders").length;'],
  "packages/app/src/db/back.ts": ['import { load } from "../domain/order";', "export const back = load;"],
  "packages/app/src/features/cart/index.ts": [
    'import { fee } from "../pay/index";',
    'import { round } from "../shared/round";',
    "export const total = (): number => round(fee());",
  ],
  "packages/app/src/features/pay/index.ts": ['import { round } from "../shared/round";', "export const fee = (): number => round(3);"],
  "packages/app/src/features/shared/round.ts": "export const round = (value: number): number => value;\n",
  "packages/app/src/tools/dump.ts": ['import { read } from "../db/disk";', "export const dump = read;"],
  "packages/app/src/dead/a.ts": ['import { b } from "./b";', "export const a = b;"],
  "packages/app/src/dead/b.ts": ['import { a } from "./a";', "export const b = (): unknown => a;"],
  "packages/app/src/deps.ts": [
    'import left from "left";',
    'import { tool } from "devtool";',
    'import type { Node } from "estree";',
    'import { missing } from "missing";',
    'import { kit } from "@scope/kit/sub";',
    'import { test } from "vitest";',
    'import { self } from "@shop/app";',
    'import type { Typed } from "@scope/typed";',
    'import { value } from "estree";',
    'import { peer } from "peer";',
    'import { join } from "node:path";',
    'import { maybe } from "maybe";',
    "export const used = [left, tool, missing, kit, test, self, value, peer, join, maybe];",
    "export type Types = Node | Typed;",
  ],
});
const shop = loadProject(path.join(root, "packages/app/tsconfig.json"));
// The same project with its `package.json` as a file, and what it names as references.
const shopWithDependencies = loadProject(path.join(root, "packages/app/tsconfig.json"), { readDependencies: true });
const render = (path: readonly string[]): string => printPath(path, shop.facts);

function violations(importRule: ImportRule, project: Project = shop): readonly Violation[] {
  const result = importRule.evaluate(project);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations;
}

function use(violation: Violation): string {
  const detail = violation.detail;
  if (detail.type !== "uses") {
    throw new Error(`not a use: ${detail.type}`);
  }
  return `${violation.file}:${violation.line} ${detail.kind} ${render(detail.target)}`;
}

const uses = (importRule: ImportRule, project: Project = shop): string[] => violations(importRule, project).map(use);
const files = (importRule: ImportRule): string[] => violations(importRule).map((violation) => violation.unit.join("/"));

{
  const shopLayers = layers("shop").layer("ui", "src/ui/**").layer("domain", "src/domain/**").layer("db", "src/db/**");
  const usesOf = (failure: Failure | undefined): [string, string[]][] => {
    const list = failure?.type === "several" ? failure.failures : failure === undefined ? [] : [failure];
    return list.map((one) => {
      if (one.type !== "violated") {
        throw new Error(one.message);
      }
      return [one.rule, one.violations.map(use)];
    });
  };

  it("layers", () => {
    // a layer must not import a layer above it
    {
      expect(usesOf(shopLayers.verify(shop))).toEqual([
        [
          "shop: domain does not import the layers above it",
          ["src/domain/price.ts:1 import-type src/ui/page.ts#serve", "src/domain/price.ts:2 import src/ui/routes.ts#ROUTE"],
        ],
        ["shop: db does not import the layers above it", ["src/db/back.ts:1 import src/domain/order.ts#load"]],
      ]);
      expect(() => shopLayers.check(shop)).toThrow(ArchitectureError);
    }

    // the report of several layers has the report of each
    {
      const message = shopLayers.verify(shop)?.message ?? "";
      expect(message).toContain("rule `shop: domain does not import the layers above it` is violated in 2 place(s):");
      expect(message).toContain("rule `shop: db does not import the layers above it` is violated in 1 place(s):");
    }

    // layers can leave out type imports
    {
      const domain = usesOf(shopLayers.ignoringTypeImports().verify(shop))[0];
      expect(domain?.[1]).toEqual(["src/domain/price.ts:2 import src/ui/routes.ts#ROUTE"]);
    }

    // layers and an onion have every filter of a rule, for all their rules
    {
      expect(usesOf(shopLayers.onlyKinds("import-type").verify(shop))).toEqual([
        ["shop: domain does not import the layers above it", ["src/domain/price.ts:1 import-type src/ui/page.ts#serve"]],
      ]);
      shopLayers.ignoringKinds("import", "import-type").check(shop);
      expect(shopLayers.ignoringCandidates().verify(shop)?.type).toBe("several");
      // one known import is left out; the rule of the other layer still fails
      const exempt = shopLayers.ignoringImport(anyOf("src/db/back.ts"), anyOf("src/domain/order.ts"));
      expect(usesOf(exempt.verify(shop)).map(([name]) => name)).toEqual(["shop: domain does not import the layers above it"]);
      // a warning reports and passes; a reason is in the report of each rule
      const warned: string[] = [];
      const warn = console.warn;
      console.warn = (text: string) => void warned.push(text);
      try {
        shopLayers.asWarning().check(shop);
      } finally {
        console.warn = warn;
      }
      expect(warned).toHaveLength(2);
      expect(shopLayers.because("the ui is the outside").verify(shop)?.message.split("the ui is the outside")).toHaveLength(3);
      // the rule about the files in a layer looks at no import, so it gets no filter of the imports
      const covered = shopLayers.everyFileInALayer(anyOf("src/ui/**")).ignoringTypeImports().asWarning().rules();
      expect(covered.map((one) => [one.toCore().kinds?.type, one.toCore().severity])).toEqual([
        ["except", "warning"],
        ["except", "warning"],
        [undefined, "warning"],
      ]);
      const onion = onionArchitecture("onion").domainModels("src/db/**").applicationServices("src/domain/**").adapter("web", "src/ui/**");
      expect(onion.verify(shop)?.type).toBe("several");
      onion.ignoringKinds("import", "import-type").check(shop);
      // the filters decide what a diagram draws too
      expect(shopLayers.diagram(shop)).toContain("stroke:#d62728");
      expect(shopLayers.ignoringKinds("import", "import-type").diagram(shop)).not.toContain("stroke:#d62728");
    }

    // a lower layer must not import any layer above it
    {
      // `db/back.ts` imports the domain. Below the ui only, the db passes.
      layers("two").layer("ui", "src/ui/**").layer("db", "src/db/**").check(shop);
      const bottom = layers("three").layer("domain", "src/domain/**").layer("features", "src/features/**").layer("db", "src/db/**");
      expect(usesOf(bottom.verify(shop))).toEqual([
        ["three: db does not import the layers above it", ["src/db/back.ts:1 import src/domain/order.ts#load"]],
      ]);
    }

    // layers that cannot fail are an error
    {
      const one = layers("one").layer("ui", "src/ui/**").verify(shop);
      expect(one?.type).toBe("malformed");
      expect(one?.message).toContain("fewer than two layers");
      expect(layers("typo").layer("ui", "src/ui/**").layer("domain", "src/domian/**").verify(shop)?.type).toBe("invalid");
    }

    // layers take a baseline
    {
      const file = path.join(scratch, "layers.baseline");
      writeFileSync(
        file,
        [
          "shop: domain does not import the layers above it\tsrc/domain/price.ts\tuses import-type src/ui/page.ts#serve\t1",
          "shop: domain does not import the layers above it\tsrc/domain/price.ts\tuses import src/ui/routes.ts#ROUTE\t1",
          "shop: db does not import the layers above it\tsrc/db/back.ts\tuses import src/domain/order.ts#load\t1",
          "",
        ].join("\n"),
      );
      shopLayers.checkWithBaseline(shop, baseline(file));
      expect(shopLayers.verify(shop)).toBeDefined();
    }
  });
}

/** The names of the rules that fail, and the imports that each reports. */
function failed(failure: Failure | undefined): [string, string[]][] {
  const list = failure?.type === "several" ? failure.failures : failure === undefined ? [] : [failure];
  return list.map((one) => {
    if (one.type !== "violated") {
      throw new Error(one.message);
    }
    return [one.rule, one.violations.map((violation) => (violation.detail.type === "uses" ? use(violation) : violation.unit.join("/")))];
  });
}

it("layer options", () => {
  const three = (): Layers => layers("three").layer("ui", "src/ui/**").layer("features", "src/features/**").layer("domain", "src/domain/**");
  const uiImports = ["src/ui/page.ts:1 import src/domain/order.ts#load", "src/ui/page.ts:3 import src/domain/price.ts#route"];

  // strict layers: the ui imports the domain, two layers below it
  {
    const found = failed(three().strict().verify(shop));
    expect(found.map(([name]) => name)).toEqual([
      "three: ui imports only the layers it may import",
      "three: domain does not import the layers above it",
    ]);
    expect(found[0]?.[1]).toEqual(uiImports);
    expect(three().strict().rules()).toHaveLength(3);
    expect(three().rules()).toHaveLength(2);
  }

  // a layer may import some layers only
  {
    const found = failed(three().mayOnlyImport("ui", "features").verify(shop));
    expect(found[0]).toEqual(["three: ui imports only the layers it may import", uiImports]);
  }

  // only some layers may import a layer
  {
    const four = (): Layers =>
      layers("four").layer("ui", "src/ui/**").layer("tools", "src/tools/**").layer("domain", "src/domain/**").layer("db", "src/db/**");
    expect(failed(four().mayOnlyBeImportedBy("db", "domain").verify(shop))).toEqual([
      ["four: tools imports only the layers it may import", ["src/tools/dump.ts:1 import src/db/disk.ts#read"]],
      ["four: domain does not import the layers above it", ["src/domain/price.ts:1 import-type src/ui/page.ts#serve", "src/domain/price.ts:2 import src/ui/routes.ts#ROUTE"]],
      ["four: db does not import the layers above it", ["src/db/back.ts:1 import src/domain/order.ts#load"]],
    ]);
    expect(failed(four().mayOnlyBeImportedBy("db", "tools", "domain").verify(shop))).toHaveLength(2);
  }
});

it("layers that cover the project, and malformed layers", () => {
  const shopLayers = (): Layers => layers("shop").layer("ui", "src/ui/**").layer("domain", "src/domain/**").layer("db", "src/db/**");

  // every file must be in a layer
  {
    const found = failed(shopLayers().everyFileInALayer(allExcept("src/main.ts", "src/deps.ts")).verify(shop));
    const [name, outside] = found[found.length - 1] ?? ["", []];
    expect(name).toBe("shop: every file is in a layer");
    expect([...outside].sort()).toEqual([
      "src/dead/a.ts",
      "src/dead/b.ts",
      "src/features/cart/index.ts",
      "src/features/pay/index.ts",
      "src/features/shared/round.ts",
      "src/tools/dump.ts",
    ]);
    const all = shopLayers()
      .layer("rest", "src/dead/**", "src/features/**", "src/tools/**")
      .everyFileInALayer(allExcept("src/main.ts", "src/deps.ts"))
      .verify(shop);
    expect(failed(all).map(([rule]) => rule)).not.toContain("shop: every file is in a layer");
  }

  // layers that name what does not exist, or what the order forbids anyway, are an error
  {
    const malformed = (made: Layers): string => {
      const failure = made.verify(shop);
      expect(failure?.type).toBe("malformed");
      expect(() => made.rules()).toThrow(ArchitectureError);
      return failure?.message ?? "";
    };
    expect(
      malformed(shopLayers().mayOnlyImport("ui", "domian", "ui").mayOnlyImport("ui").mayOnlyBeImportedBy("db", "db").mayOnlyBeImportedBy("store")),
    ).toBe(
      [
        "rule `shop` cannot be checked:",
        "  `domian` is not a layer",
        "  `ui` may only import `ui`, which the order of the layers forbids",
        "  `ui` has two lists of the layers it may import",
        "  `db` may only be imported by `db`, which the order of the layers forbids",
        "  `store` is not a layer",
      ].join("\n"),
    );
    expect(malformed(shopLayers().strict().mayOnlyImport("ui", "db"))).toContain("`ui` may only import `db`");
    expect(malformed(shopLayers().mayOnlyBeImportedBy("ui", "domain"))).toContain("`ui` may only be imported by `domain`");
    expect(malformed(shopLayers().mayOnlyBeImportedBy("domain").mayOnlyBeImportedBy("domain", "ui"))).toContain(
      "`domain` has two lists of the layers that may import it",
    );
    expect(malformed(shopLayers().layer("ui", "src/tools/**"))).toContain("two layers have the name `ui`");
  }
});

it("only some files may import a file", () => {
  const db = rule("only the domain imports the db").files(anyOf("src/db/**")).mustOnlyBeImportedBy(anyOf("src/domain/**"));
  expect(uses(db)).toEqual(["src/tools/dump.ts:1 import src/db/disk.ts#read"]);
  rule("the domain and the tools").files(anyOf("src/db/**")).mustOnlyBeImportedBy(anyOf("src/domain/**", "src/tools/**")).check(shop);
  const typo = rule("typo").files(anyOf("src/db/**")).mustOnlyBeImportedBy(anyOf("src/domian/**")).evaluate(shop);
  expect(!typo.ok && typo.errors.map((error) => error.code)).toEqual(["pattern-matches-nothing"]);
});

it("exemptions", () => {
  // an exemption lets one known import pass
  {
    const domain = (): ImportRule => rule("the domain does not import the ui").files(anyOf("src/domain/**")).mustNotImport(anyOf("src/ui/**"));
    expect(uses(domain())).toHaveLength(2);
    domain().ignoringImport(anyOf("src/domain/price.ts"), anyOf("src/ui/**")).check(shop);
    expect(uses(domain().ignoringImport(anyOf("src/domain/price.ts"), anyOf("src/ui/routes.ts")))).toEqual([
      "src/domain/price.ts:1 import-type src/ui/page.ts#serve",
    ]);

    // an exemption that lets nothing pass is an error
    const unused = domain()
      .ignoringImport(anyOf("src/domain/price.ts"), anyOf("src/ui/**"))
      .ignoringImport(anyOf("src/domain/order.ts"), anyOf("src/ui/**"))
      .verify(shop);
    expect(unused?.type).toBe("invalid");
    expect(unused?.message).toContain("exemption `src/domain/order.ts -> src/ui/**` lets no reference pass");

    // the files of an exemption are a selection, as the files of a rule are
    expect(uses(domain().ignoringImport(all(), anyOf("src/ui/routes.ts")))).toHaveLength(1);
    expect(uses(domain().ignoringImport(allExcept("src/domain/order.ts"), anyOf("src/ui/routes.ts")))).toHaveLength(1);
    const excepted = domain().ignoringImport(anyOf("src/domain/**").except("src/domain/price.ts"), anyOf("src/ui/**")).verify(shop);
    expect(excepted?.message).toContain("exemption `src/domain/** except src/domain/price.ts -> src/ui/**` lets no reference pass");

    // without the types: packages are no files, and a selection is no list of targets
    const wrong = [anyOf(pkg("node:fs")) as unknown as AnyOf<string>, anyOf("**")];
    const targets = [anyOf("src/ui/**"), all() as unknown as AnyOf];
    wrong.forEach((from, index) => {
      expect(() => domain().ignoringImport(from, targets[index] as AnyOf)).toThrow("takes a selection of files as in files(..)");
    });
  }

  // an exempt import is no edge of a cycle
  {
    const dead = (): ImportRule => rule("dead").files(anyOf("src/dead/**")).mustBeAcyclic();
    expect(violations(dead())).toHaveLength(1);
    dead().ignoringImport(anyOf("src/dead/b.ts"), anyOf("src/dead/a.ts")).check(shop);
  }
});

it("rule groups", () => {
  const domain = rule("the domain does not import the ui").files(anyOf("src/domain/**")).mustNotImport(anyOf("src/ui/**"));
  const db = rule("the db does not import the ui").files(anyOf("src/db/**")).mustNotImport(anyOf("src/ui/**"));
  const features = rule("features").files(all()).mustBeIndependentBetween(anyOf("src/features/*"));

  // a group reports every rule that fails, in the order of the rules
  {
    const failure = rules("shop", domain, db, features).verify(shop);
    expect(failure?.type === "several" && failure.failures.map((one) => one.type === "violated" && one.rule)).toEqual([
      "the domain does not import the ui",
      "features",
    ]);
    expect(() => rules("shop", domain, db, features).check(shop)).toThrow(ArchitectureError);
  }

  // a group passes when all rules pass, and one failure is reported as itself
  {
    rules("passes", db).check(shop);
    const failure = rules("one fails", domain, db).verify(shop);
    expect(failure?.type === "violated" && failure.rule).toBe("the domain does not import the ui");
  }

  // a group without rules cannot fail
  {
    const empty = rules("empty").verify(shop);
    expect(empty?.type).toBe("malformed");
    expect(empty?.message).toContain("no rules");
  }
});

{
  const features = (selection: Parameters<ReturnType<typeof rule>["files"]>[0]): ImportRule =>
    rule("features").files(selection).mustBeIndependentBetween(anyOf("src/features/*"));

  it("independent slices", () => {
    // no slice may import another slice
    {
      expect(uses(features(all()))).toEqual([
        "src/features/cart/index.ts:1 import src/features/pay/index.ts#fee",
        "src/features/cart/index.ts:2 import src/features/shared/round.ts#round",
        "src/features/pay/index.ts:1 import src/features/shared/round.ts#round",
      ]);
    }

    // a part that is not selected is shared
    {
      expect(uses(features(allExcept("src/features/shared/**")))).toEqual(["src/features/cart/index.ts:1 import src/features/pay/index.ts#fee"]);
    }

    // slices that cannot import each other are an error
    {
      const result = features(anyOf("src/features/cart/**")).evaluate(shop);
      expect(!result.ok && result.errors.map((error) => error.code)).toEqual(["too-few-nodes"]);
    }
  });
}

it("files that nothing imports", () => {
  // a file that no other file imports is reported
  {
    const found = violations(rule("imported").files(allExcept("src/main.ts")).mustBeImported());
    // `dead/a.ts` and `dead/b.ts` import each other.
    expect(found.map((violation) => violation.unit.join("/"))).toEqual(["src/db/back.ts", "src/deps.ts", "src/tools/dump.ts"]);
    expect(found.every((violation) => violation.detail.type === "unused" && violation.line === 0)).toBe(true);
  }

  // type imports can be left out
  {
    const routes = rule("imported").files(anyOf("src/ui/page.ts")).mustBeImported();
    routes.check(shop);
    // Without the type import of `domain/price.ts`, `main.ts` still imports the page.
    routes.ignoringTypeImports().check(shop);
    const price = rule("imported").files(anyOf("src/ui/routes.ts")).mustBeImported().onlyKinds("import-type");
    expect(files(price)).toEqual(["src/ui/routes.ts"]);
  }

  // a file that nothing imports is printed without a line
  {
    const failure = rule("imported").files(anyOf("src/tools/**", "src/ui/**")).mustBeImported().verify(shop);
    expect(failure?.message).toBe(
      "rule `imported` is violated in 1 place(s):\n  src/tools/dump.ts: [imported] src/tools/dump.ts is not used from outside",
    );
  }
});

{
  const noDisk = rule("the ui does not reach the file system").files(anyOf("src/ui/**")).mustNotReach(anyOf(pkg("node:fs")));

  it("chains", () => {
    // a chain to a target is reported with its steps
    {
      expect(noDisk.verify(shop)?.message).toBe(
        [
          "rule `the ui does not reach the file system` is violated in 1 place(s):",
          "  src/ui/page.ts:1:10: [the ui does not reach the file system] src/ui/page.ts reaches node:fs#readFileSync in 3 step(s):",
          "      src/ui/page.ts -> src/domain/order.ts  at src/ui/page.ts:1:10, uses src/domain/order.ts#load (import)",
          "      src/domain/order.ts -> src/db/disk.ts  at src/domain/order.ts:1:10, uses src/db/disk.ts#read (import)",
          "      src/db/disk.ts uses node:fs#readFileSync  at src/db/disk.ts:1:10 (import)",
        ].join("\n"),
      );
    }

    // a direct import is a chain without steps
    {
      const found = violations(rule("r").files(anyOf("src/db/disk.ts")).mustNotReach(anyOf(pkg("node:fs"))));
      const detail = found[0]?.detail;
      expect(detail?.type === "reaches" && [detail.steps.length, detail.end.unit.join("/"), detail.end.line]).toEqual([0, "src/db/disk.ts", 1]);
    }

    // a chain can lead to a file
    {
      const found = violations(rule("r").files(anyOf("src/main.ts")).mustNotReach(anyOf("src/db/disk.ts")));
      const detail = found[0]?.detail;
      expect(detail?.type === "reaches" && detail.steps.map((step) => step.to.join("/"))).toEqual(["src/ui/page.ts", "src/domain/order.ts"]);
      expect(detail?.type === "reaches" && render(detail.end.target)).toBe("src/db/disk.ts#read");
    }
  });
}

it("reachable from the roots", () => {
  // a file that no root reaches is reported
  {
    const live = rule("live").files(all()).mustBeReachableFrom(anyOf("src/main.ts"));
    expect(files(live)).toEqual(["src/db/back.ts", "src/dead/a.ts", "src/dead/b.ts", "src/deps.ts", "src/tools/dump.ts"]);
    expect(violations(live).every((violation) => violation.detail.type === "unreachable")).toBe(true);
  }

  // a rule that cannot fail is an error
  {
    const onlyRoots = rule("r").files(anyOf("src/main.ts")).mustBeReachableFrom(anyOf("src/main.ts")).evaluate(shop);
    expect(!onlyRoots.ok && onlyRoots.errors.map((error) => error.code)).toEqual(["no-subjects"]);
    const typo = rule("r").files(all()).mustBeReachableFrom(anyOf("src/mian.ts")).evaluate(shop);
    expect(!typo.ok && typo.errors).toEqual([{ code: "pattern-matches-nothing", role: "root", value: "src/mian.ts" }]);
  }

  // the new violations have baseline keys
  {
    const file = path.join(scratch, "graph.baseline");
    const rules = [
      rule("imported").files(anyOf("src/tools/**", "src/ui/**")).mustBeImported(),
      rule("reach").files(anyOf("src/ui/**")).mustNotReach(anyOf(pkg("node:fs"))),
      rule("live").files(anyOf("src/tools/**", "src/ui/**")).mustBeReachableFrom(anyOf("src/main.ts")),
    ];
    for (const one of rules) {
      expect(one.writeBaseline(shop, baseline(file))).toBeUndefined();
      one.checkWithBaseline(shop, baseline(file));
    }
    const lines = readFileSync(file, "utf8").split("\n").filter((line) => line !== "" && !line.startsWith("#"));
    expect(lines).toEqual([
      "imported\tsrc/tools/dump.ts\tunused\t1",
      "live\tsrc/tools/dump.ts\tunreachable\t1",
      "reach\tsrc/ui/page.ts\treaches node:fs#readFileSync\t1",
    ]);
  }
});

{
  const declared = rule("declared").files(all()).mustOnlyImportDeclaredPackages();
  const noDev = rule("no dev").files(all()).mustNotImportDevDependencies();

  it("packages and package.json", () => {
    // an import of a package that the nearest package.json does not declare is reported
    {
      expect(uses(declared, shopWithDependencies)).toEqual([
        // `vitest` is declared by the workspace root only.
        "src/deps.ts:4 import missing#missing",
        "src/deps.ts:6 import vitest#test",
        // `@types/estree` declares the types of `estree`, not its values.
        "src/deps.ts:9 import estree#value",
      ]);
    }

    // an import of a package that is only a devDependency is reported
    {
      expect(uses(noDev, shopWithDependencies)).toEqual([
        "src/deps.ts:2 import devtool#tool",
        "src/deps.ts:3 import-type estree#Node",
        "src/deps.ts:8 import-type @scope/typed#Typed",
      ]);
      expect(uses(noDev.ignoringTypeImports(), shopWithDependencies)).toEqual(["src/deps.ts:2 import devtool#tool"]);
    }

    // the rules take the selection of files
    {
      rule("declared").files(allExcept("src/deps.ts")).mustOnlyImportDeclaredPackages().check(shopWithDependencies);
      rule("no dev").files(allExcept("src/deps.ts")).mustNotImportDevDependencies().check(shopWithDependencies);
      const typo = rule("declared").files(anyOf("src/dpes.ts")).mustOnlyImportDeclaredPackages().verify(shopWithDependencies);
      expect(typo?.type).toBe("invalid");
    }

    // the rules take a baseline
    {
      const file = path.join(scratch, "packages.baseline");
      expect(declared.writeBaseline(shopWithDependencies, baseline(file))).toBeUndefined();
      declared.checkWithBaseline(shopWithDependencies, baseline(file));
      expect(readFileSync(file, "utf8")).toContain("declared\tsrc/deps.ts\tuses import vitest#test\t1\n");
      expect(declared.verify(shopWithDependencies)?.type).toBe("violated");
    }

    // a project without devDependencies cannot break the rule about them
    {
      const plain = path.join(scratch, "plain");
      write(plain, {
        "package.json": '{ "name": "plain", "dependencies": { "left": "1" } }',
        "tsconfig.json": TSCONFIG,
        "src/a.ts": ['import left from "left";', 'import right from "right";', "export const a = [left, right];"],
      });
      const project = loadProject(path.join(plain, "tsconfig.json"), { readDependencies: true });
      const rule_ = rule("no dev").files(all()).mustNotImportDevDependencies();
      const failure = rule_.verify(project);
      expect(failure?.type).toBe("malformed");
      expect(failure?.message).toContain("no package.json of the project has devDependencies");
      expect(() => rule_.evaluate(project)).toThrow(ArchitectureError);
      expect(rule_.verifyWithBaseline(project, baseline(path.join(scratch, "none.baseline")))?.type).toBe("malformed");
      expect(uses(rule("declared").files(all()).mustOnlyImportDeclaredPackages(), project)).toEqual(["src/a.ts:2 import right#default"]);
    }

    // the nearest package.json that describes a package is the manifest of a file: not the
    // one of the workspace root, and not one that only sets the module type
    {
      expect(shopWithDependencies.files.filter((file) => file.endsWith("package.json"))).toEqual(["package.json"]);
      expect(shop.files.some((file) => file.endsWith("package.json"))).toBe(false);
    }

    // the rules need the dependencies of the project
    {
      const failure = declared.verify(shop);
      expect(failure?.type).toBe("malformed");
      expect(failure?.message).toContain("the project is loaded without `readDependencies`");
    }
  });
}

it("the dependencies of package.json files are references", () => {
  const dir = path.join(scratch, "installed");
  const manifest = (name: string, dependencies: readonly string[] = []): string =>
    JSON.stringify({ name, dependencies: Object.fromEntries(dependencies.map((one) => [one, "1"])) });
  write(dir, {
    "package.json": [
      "{",
      '  "name": "app",',
      '  "dependencies": {',
      '    "direct": "1",',
      '    "@acme/ui": "workspace:*"',
      "  },",
      '  "devDependencies": { "tester": "1", "shared": "1" },',
      '  "optionalDependencies": {',
      '    "absent": "1"',
      "  },",
      '  "peerDependencies": { "host": "1" }',
      "}",
    ],
    "tsconfig.json": '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src", "packages"] }',
    "src/a.ts": ['import { direct } from "direct";', 'import { ui } from "@acme/ui";', "export const a = [direct, ui];"],
    "packages/ui/package.json": manifest("@acme/ui", ["deep"]),
    "packages/ui/src/index.ts": "export const ui = 1;\n",
    "node_modules/direct/package.json": manifest("direct", ["deep", "shared"]),
    "node_modules/deep/package.json": manifest("deep", ["deeper", "direct"]),
    "node_modules/deeper/package.json": manifest("deeper"),
    "node_modules/shared/package.json": manifest("shared"),
    "node_modules/host/package.json": manifest("host"),
    "node_modules/tester/package.json": manifest("tester", ["shared", "test-only", "not-installed", "deep"]),
    "node_modules/test-only/package.json": manifest("test-only"),
  });
  mkdirSync(path.join(dir, "node_modules/@acme"), { recursive: true });
  symlinkSync(path.join(dir, "packages/ui"), path.join(dir, "node_modules/@acme/ui"), "dir");
  const app = loadProject(path.join(dir, "tsconfig.json"), { readDependencies: true });

  // each package.json that a file belongs to is a file of the project
  expect(app.files).toEqual(["package.json", "packages/ui/package.json", "packages/ui/src/index.ts", "src/a.ts"]);
  const named = (file: string): string[] =>
    app.facts.facts
      .filter((fact) => fact.file === file && fact.what.type === "reference")
      .map((fact) => {
        const what = fact.what as { target: readonly string[]; aliases: readonly (readonly string[])[]; kind: string };
        const aliases = what.aliases.map((alias) => ` [${alias.join("/")}]`).join("");
        return `${fact.line}:${fact.column} ${what.kind} ${what.target.join("/")}${aliases}${fact.note !== undefined ? ` (${fact.note})` : ""}`;
      });
  expect(named("package.json")).toEqual([
    "4:5 dependency npm:direct",
    // a package of the project is also its package.json
    "5:5 dependency npm:@acme/ui [packages/ui/package.json]",
    "7:24 dev-dependency npm:tester",
    "7:39 dev-dependency npm:shared",
    // a package that is not installed is named, and not followed
    "9:5 optional-dependency npm:absent",
    "11:25 peer-dependency npm:host",
    // what the dependencies need, at the line of the dependency, the nearest first
    "4:5 transitive-dependency npm:deep (through `direct`)",
    // the file names it for the development; the package is installed with it too
    "4:5 transitive-dependency npm:shared (through `direct`)",
    "4:5 transitive-dependency npm:deeper (through `direct`, `deep`)",
    // only the development needs it; `deep`, which the tester needs too, is installed with the package
    "7:24 transitive-dev-dependency npm:test-only (through `tester`)",
  ]);
  expect(named("packages/ui/package.json")).toEqual([
    "1:36 dependency npm:deep",
    "1:36 transitive-dependency npm:deeper (through `deep`)",
    "1:36 transitive-dependency npm:direct (through `deep`)",
    "1:36 transitive-dependency npm:shared (through `deep`, `direct`)",
  ]);

  // a rule says which package may depend on which, directly or through other packages
  const without = (name: string): ImportRule => rule("installed without").files(anyOf("package.json")).mustNotImport(anyOf(pkg(name)));
  const failure = without("deeper").verify(app);
  expect(failure?.message).toContain("package.json:4:5: [installed without] package.json uses deeper (transitive-dependency, through `direct`, `deep`)");
  without("test-only").onlyKinds("dependency", "transitive-dependency").check(app);
  expect(without("test-only").onlyKinds("transitive-dev-dependency").verify(app)?.type).toBe("violated");
  // the rules about files see the dependencies between the packages of the project
  const onUi = rule("the app names the ui").files(anyOf("package.json")).mustNotImport(anyOf("packages/ui/**")).verify(app);
  expect(onUi?.type).toBe("violated");
  rule("the packages form no cycle").files(anyOf("**/package.json")).mustBeAcyclic().check(app);
  // a package.json has its lines, as every file
  const long = rule("short files").files(anyOf("**/package.json")).mustNotExceedLines(5).verify(app);
  expect(long?.type === "violated" && long.violations.map((violation) => violation.file)).toEqual(["package.json"]);
  // the package of a package.json is an item
  const names = rule("the packages are of acme").items(anyOf("**/package.json")).ofKind("package").mustBeNamed("@acme/*").verify(app);
  expect(names?.type === "violated" && names.violations.map((violation) => `${violation.file}:${violation.line}`)).toEqual(["package.json:2"]);
  // the rules about the imports read the same references
  rule("declared").files(anyOf("src/**", "packages/ui/src/**")).mustOnlyImportDeclaredPackages().check(app);
  rule("declared").files(all()).mustOnlyImportDeclaredPackages().check(app);

  // without the option, a package.json is no file, and a rule that names the kinds is an error
  const plain = loadProject(path.join(dir, "tsconfig.json"));
  expect(plain.files).toEqual(["packages/ui/src/index.ts", "src/a.ts"]);
  expect(plain.facts.facts.filter((fact) => !fact.file.endsWith("package.json"))).toEqual(app.facts.facts.filter((fact) => !fact.file.endsWith("package.json")));
  const unknown = rule("kinds").files(all()).mustNotImport(anyOf(pkg("direct"))).onlyKinds("dependency").verify(plain);
  expect(unknown?.type).toBe("invalid");
});

it("a dependency has the version and the licence of its package", () => {
  const dir = path.join(scratch, "licensed");
  const manifest = (name: string, more: Readonly<Record<string, unknown>>, dependencies: readonly string[] = []): string =>
    JSON.stringify({ name, ...more, dependencies: Object.fromEntries(dependencies.map((one) => [one, "*"])) });
  write(dir, {
    "package.json": [
      "{",
      '  "name": "app",',
      '  "dependencies": {',
      '    "direct": "1",',
      '    "shared": "1",',
      '    "@acme/ui": "workspace:*"',
      "  },",
      '  "devDependencies": { "tester": "3" },',
      '  "optionalDependencies": { "absent": "1" }',
      "}",
    ],
    "tsconfig.json": '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src", "packages"] }',
    "src/a.ts": ['import { direct } from "direct";', "export const a = direct;"],
    "packages/ui/package.json": manifest("@acme/ui", { version: "0.1.0" }),
    "packages/ui/src/index.ts": "export const ui = 1;\n",
    "node_modules/direct/package.json": manifest("direct", { version: "1.2.0", license: "MIT" }, ["shared", "old"]),
    // another version of a package that the file names is installed below the package that needs it
    "node_modules/direct/node_modules/shared/package.json": manifest("shared", { version: "2.0.0", license: "GPL-3.0-only" }),
    // an old form of the licence, and a form that names no licence
    "node_modules/shared/package.json": manifest("shared", { version: "1.0.0", license: { type: "ISC", url: "https://example.org" } }),
    "node_modules/old/package.json": manifest("old", { licenses: [{ type: "MIT" }], license: 7, version: 1 }),
    // the tester needs the version of `shared` that the file names
    "node_modules/tester/package.json": manifest("tester", { version: "3.0.0" }, ["shared"]),
  });
  mkdirSync(path.join(dir, "node_modules/@acme"), { recursive: true });
  symlinkSync(path.join(dir, "packages/ui"), path.join(dir, "node_modules/@acme/ui"), "dir");
  const app = loadProject(path.join(dir, "tsconfig.json"), { readDependencies: true });

  const properties = app.facts.facts
    .filter((fact) => fact.file === "package.json" && fact.what.type === "reference")
    .map((fact) => {
      const what = fact.what as { target: readonly string[]; kind: string; properties?: Readonly<Record<string, string>> };
      const pairs = Object.entries(what.properties ?? {}).map(([name, value]) => ` ${name}=${value}`);
      return `${what.kind} ${what.target.join("/")}${pairs.join("")}`;
    });
  expect(properties).toEqual([
    "dependency npm:direct version=1.2.0 license=MIT",
    "dependency npm:shared version=1.0.0 license=ISC",
    "dependency npm:@acme/ui version=0.1.0",
    "dev-dependency npm:tester version=3.0.0",
    // a package that is not installed has no properties
    "optional-dependency npm:absent",
    // two versions of one package are two references
    "transitive-dependency npm:shared version=2.0.0 license=GPL-3.0-only",
    "transitive-dependency npm:old",
  ]);
  // what the code imports has no properties, and neither has a package that says nothing
  const references = app.facts.facts.filter((fact) => fact.what.type === "reference");
  const without = references.filter((fact) => !("properties" in fact.what)).map((fact) => (fact.what as { target: readonly string[] }).target.join("/"));
  expect(without).toEqual(["npm:direct/#direct", "npm:absent", "npm:old"]);

  // a check of your own reads the licence: the example of docs/typescript.md
  const accepted = (licences: readonly string[]): ImportRule =>
    rule("every package has a licence that we accept")
      .files(anyOf("**/package.json"))
      .mustSatisfy((scope) =>
        scope.imports
          .filter((seen) => seen.subject)
          // a package of the project is no package of others
          .filter((seen) => seen.owners.every((owner) => owner === undefined))
          .flatMap((seen) => {
            const licence = seen.reference.properties?.["license"];
            if (licence !== undefined && licences.includes(licence)) {
              return [];
            }
            const name = printPath(seen.reference.target, scope.facts);
            return [scope.violationAt(seen.fact, `is installed with ${name}, which has ${licence !== undefined ? `the licence ${licence}` : "no licence"}`)];
          }),
      )
      .readingProperties("license")
      .onlyKinds("dependency", "transitive-dependency");
  const messages = (failure: Failure | undefined): readonly string[] =>
    failure?.type === "violated" ? failure.violations.map((violation) => `${violation.file}:${violation.line} ${(violation.detail as { message: string }).message}`) : [];
  expect(messages(accepted(["MIT", "ISC"]).verify(app))).toEqual([
    "package.json:4 is installed with shared, which has the licence GPL-3.0-only",
    "package.json:4 is installed with old, which has no licence",
  ]);
  expect(accepted(["MIT", "ISC"]).verify(app)?.message).toContain(
    "package.json:4:5: [every package has a licence that we accept] package.json is installed with shared, which has the licence GPL-3.0-only (through `direct`)",
  );
  expect(messages(accepted(["MIT"]).verify(app))[0]).toBe("package.json:5 is installed with shared, which has the licence ISC");
  // a property that no package can have is an error, and so is each property of a load without the dependencies
  expect(accepted(["MIT"]).readingProperties("licence").verify(app)?.message).toContain("property `licence` is not known to this front end");
  const plain = rule("licensed").files(anyOf("src/**")).mustNotImport(anyOf("npm:shared")).readingProperties("license");
  expect(plain.verify(app)).toBeUndefined();
  expect(plain.verify(loadProject(path.join(dir, "tsconfig.json")))?.message).toContain("property `license` is not known to this front end");

  // a check of your own compares the versions: the example of docs/typescript.md
  const oneVersion = rule("one version of each package is installed")
    .files(anyOf("**/package.json"))
    .mustSatisfy((scope) => {
      const versions = new Map<string, Set<string>>();
      const versioned = scope.imports.flatMap((seen) => {
        const version = seen.reference.properties?.["version"];
        return version !== undefined ? [{ seen, version, name: printPath(seen.reference.target, scope.facts) }] : [];
      });
      for (const { name, version } of versioned) {
        versions.set(name, (versions.get(name) ?? new Set()).add(version));
      }
      return versioned
        .filter(({ seen, name }) => seen.subject && (versions.get(name) as Set<string>).size > 1)
        .map(({ seen, name, version }) => {
          const all = [...(versions.get(name) as Set<string>)].sort().join(", ");
          return scope.violationAt(seen.fact, `is installed with ${name} ${version}, one of ${all}`, `is installed with several versions of ${name}`);
        });
    });
  expect(messages(oneVersion.verify(app))).toEqual([
    "package.json:5 is installed with shared 1.0.0, one of 1.0.0, 2.0.0",
    "package.json:4 is installed with shared 2.0.0, one of 1.0.0, 2.0.0",
  ]);
  // the filters of the rule say which packages count: the file names one version
  oneVersion.onlyKinds("dependency", "dev-dependency").check(app);
});
