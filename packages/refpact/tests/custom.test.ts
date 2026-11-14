// Rules with a check of the user: the subjects, the filters, the exemptions, the errors, the
// reports and the baselines are those of every rule.
import { mkdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";
import { scratchDirectory } from "./scratch.ts";

import { all, allExcept, anyOf, ArchitectureError, baseline, layers, loadProject, onionArchitecture, pkg, rule, rules, RuleSet, unverified } from "../src/index.ts";
import type { CheckPatterns, Failure, ImportRule, ItemScope, ItemSelection, Scope, Violation } from "../src/index.ts";

const directory = scratchDirectory("refpact-custom-");
const root = path.join(directory, "app");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }'],
  "src/a.ts": ['import { b } from "./b";', 'import type { C } from "./c";', 'import { c } from "./c";', "export const a = [b, c] as C[];"],
  "src/b.ts": ['import { c } from "./c";', "export const b = c;"],
  "src/c.ts": ["export type C = number;", "export const c = 1;"],
  "src/d.ts": ["export const d = 1;", "debugger;"],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}
const app = loadProject(path.join(root, "tsconfig.json"));

/** One violation for each selected file with more than `limit` imports that the rule sees, at the first one. */
function atMost(limit: number): (scope: Scope) => Violation[] {
  return (scope) =>
    scope.subjects.flatMap((file) => {
      const imports = scope.imports.filter((seen) => seen.file === file);
      const first = imports[0];
      if (first === undefined || imports.length <= limit) {
        return [];
      }
      return [scope.violationAt(first.fact, `has ${imports.length} imports, more than ${limit}`, `more than ${limit} imports`)];
    });
}

function message(failure: Failure | undefined): string {
  if (failure === undefined) {
    throw new Error("the rule passes");
  }
  return failure.message;
}

it("a check of the user finds violations and reports them", () => {
  const failure = rule("small files").files(all()).mustSatisfy(atMost(2)).because("small files are easy to read").verify(app);
  expect(message(failure)).toBe(
    [
      "rule `small files` is violated in 1 place(s):",
      "  because small files are easy to read",
      "  src/a.ts:1:10: [small files] src/a.ts has 3 imports, more than 2",
    ].join("\n"),
  );
  expect(failure?.type === "violated" && failure.violations[0]?.detail).toEqual({ type: "custom", message: "has 3 imports, more than 2", key: "more than 2 imports" });
});

it("the subjects, the filters and the exemptions apply to a check of the user", () => {
  expect(rule("small files").files(allExcept("src/a.ts")).mustSatisfy(atMost(1)).verify(app)).toBeUndefined();
  expect(rule("small files").files(all()).mustSatisfy(atMost(2)).ignoringTypeImports().verify(app)).toBeUndefined();
  const exempt = rule("small files").files(all()).mustSatisfy(atMost(2)).ignoringImport(anyOf("src/a.ts"), anyOf("src/b.ts"));
  expect(exempt.verify(app)).toBeUndefined();
  // The scope has the imports of all files, and says which are of the subjects.
  const seen: string[] = [];
  rule("probe")
    .files(anyOf("src/b.ts"))
    .mustSatisfy((scope) => {
      seen.push(...scope.imports.map((found) => `${found.file.join("/")} ${found.subject} ${found.owners.map((owner) => owner?.join("/")).join(",")}`));
      return [];
    })
    .check(app);
  expect(seen).toEqual(["src/a.ts false src/b.ts", "src/a.ts false src/c.ts", "src/a.ts false src/c.ts", "src/b.ts true src/c.ts"]);
});

it("the targets of a check of the user are checked and hit", () => {
  const hits: string[] = [];
  rule("probe")
    .files(all())
    .mustSatisfy(
      (scope) => {
        for (const seen of scope.imports) {
          const found = Object.entries(seen.hits).map(([name, hit]) => `${name}=${hit.join("/")}`);
          hits.push(`${seen.fact.file}:${seen.fact.line} ${found.join(" ")}`);
        }
        return [];
      },
      { targets: { c: anyOf("src/c.ts"), values: anyOf("src/c.ts#c", "src/b.ts"), packages: unverified(pkg("*")) } },
    )
    .check(app);
  expect(hits).toEqual([
    "src/a.ts:1 values=src/b.ts/#b",
    "src/a.ts:2 c=src/c.ts/#C",
    "src/a.ts:3 c=src/c.ts/#c values=src/c.ts/#c",
    "src/b.ts:1 c=src/c.ts/#c values=src/c.ts/#c",
  ]);
  const check = (patterns: CheckPatterns): Failure | undefined => rule("typo").files(all()).mustSatisfy(() => [], patterns).verify(app);
  expect(message(check({ targets: { e: anyOf("src/e.ts") } }))).toBe("rule `typo` cannot be checked:\n  target pattern `src/e.ts` matches nothing in the analysed code");
  expect(check({ targets: { packages: anyOf(pkg("nothing")) } })).toBeUndefined();
});

it("the lists of files of a check of the user are checked and matched", () => {
  const matched: Record<string, string[]> = {};
  const lists = { files: { early: anyOf("src/a.ts", "src/b.ts"), all: anyOf("src/*"), last: anyOf("src/d.ts") } };
  rule("probe")
    .files(anyOf("src/a.ts"))
    .mustSatisfy((scope) => {
      for (const name of ["early", "all", "last"]) {
        matched[name] = scope.matched(name).map((file) => file.join("/"));
      }
      expect(() => scope.matched("late")).toThrow("the check names no list of files `late`");
      return [];
    }, lists)
    .check(app);
  expect(matched).toEqual({ early: ["src/a.ts", "src/b.ts"], all: ["src/a.ts", "src/b.ts", "src/c.ts", "src/d.ts"], last: ["src/d.ts"] });
  const typo = rule("typo").files(all()).mustSatisfy(() => [], { files: { e: anyOf("src/e.ts"), none: anyOf() } }).verify(app);
  expect(message(typo)).toBe(
    ["rule `typo` cannot be checked:", "  place pattern `src/e.ts` matches nothing in the analysed code", "  the place pattern list is empty"].join("\n"),
  );
});

it("a check of the user follows the imports in the graph of the files", () => {
  const file = (name: string): string[] => ["src", name];
  let edges: string[] = [];
  const chains: Record<string, string | undefined> = {};
  const depth = rule("no file is more than one import away from c")
    .files(all())
    .mustSatisfy((scope) => {
      const graph = scope.graph();
      expect(scope.graph()).toBe(graph);
      edges = scope.subjects.flatMap((from) => graph.edgesOf(from).map((edge) => `${from.join("/")} -> ${edge.to.join("/")} at ${edge.fact.line} by ${edge.target.join("/")}`));
      const isC = (found: readonly string[]): boolean => found.join("/") === "src/c.ts";
      for (const from of scope.subjects) {
        chains[from.join("/")] = graph.chain(from, isC)?.map((step) => `${step.from.join("/")}:${step.line} ${step.kind} ${step.to.join("/")}`).join(", ");
      }
      expect(graph.edgesOf(file("nothing.ts"))).toEqual([]);
      expect(graph.chain(file("nothing.ts"), isC)).toBeUndefined();
      // A chain is the detail of a built-in rule, so it prints as that rule prints it.
      const steps = graph.chain(file("a.ts"), (found) => found.join("/") === "src/b.ts") ?? [];
      const first = steps[0];
      return first === undefined ? [] : [scope.violationAt(scope.facts.facts.find((fact) => fact.file === first.file && fact.line === first.line)!, "is one import away from b")];
    });
  expect(message(depth.verify(app))).toBe(
    "rule `no file is more than one import away from c` is violated in 1 place(s):\n  src/a.ts:1:10: [no file is more than one import away from c] src/a.ts is one import away from b",
  );
  // Each edge is there once, with its first import: the type import of `c` in `a`.
  expect(edges).toEqual(["src/a.ts -> src/b.ts at 1 by src/b.ts/#b", "src/a.ts -> src/c.ts at 2 by src/c.ts/#C", "src/b.ts -> src/c.ts at 1 by src/c.ts/#c"]);
  expect(chains).toEqual({ "src/a.ts": "src/a.ts:2 import-type src/c.ts", "src/b.ts": "src/b.ts:1 import src/c.ts", "src/c.ts": "", "src/d.ts": undefined });
  // The graph has the imports that the rule sees: without type imports, `a` reaches `c` at line 3.
  let withoutTypes: string | undefined;
  rule("probe")
    .files(all())
    .mustSatisfy((scope) => {
      withoutTypes = scope.graph().chain(file("a.ts"), (found) => found.join("/") === "src/c.ts")?.map((step) => `${step.line}`).join(",");
      return [];
    })
    .ignoringTypeImports()
    .check(app);
  expect(withoutTypes).toBe("3");
  // An import through a barrel file leads to the barrel and to the file that declares the name.
  const barrels = path.join(directory, "barrels");
  const sources: Record<string, string> = {
    "tsconfig.json": files["tsconfig.json"]![0]!,
    "src/user.ts": 'import { one } from "./lib";\nexport const user = one;\n',
    "src/lib/index.ts": 'export { one } from "./one";\n',
    "src/lib/one.ts": 'import type { user } from "../user";\nexport const one = 1;\nexport type User = typeof user;\n',
    "src/alone.ts": "export const alone = 1;\n",
  };
  for (const [name, text] of Object.entries(sources)) {
    mkdirSync(path.dirname(path.join(barrels, name)), { recursive: true });
    writeFileSync(path.join(barrels, name), text);
  }
  let throughBarrel: string[] = [];
  let tangles: string[][] = [];
  rule("probe")
    .files(all())
    .mustSatisfy((scope) => {
      throughBarrel = scope.graph().edgesOf(["src", "user.ts"]).map((edge) => edge.to.join("/"));
      tangles = scope.graph().tangles().map((tangle) => [tangle.members.map((file) => file.join("/")).join(" "), tangle.cycle.map((step) => `${step.from.join("/")}:${step.line} ${step.kind}`).join(", ")]);
      return [];
    })
    .check(loadProject(path.join(barrels, "tsconfig.json")));
  expect(throughBarrel).toEqual(["src/lib/index.ts", "src/lib/one.ts"]);
  // The files that reach each other are one tangle, with a shortest cycle through its first file.
  expect(tangles).toEqual([["src/lib/index.ts src/lib/one.ts src/user.ts", "src/lib/index.ts:1 export-from, src/lib/one.ts:1 import-type, src/user.ts:1 import"]]);
});

it("a check of items gets the items that the filters of the rule select", () => {
  const constants = (): ItemSelection => rule("constants are short").items(all()).ofKind("variable");
  const short = (scope: ItemScope): Violation[] =>
    scope.items.filter((item) => item.name.length > 0).map((item) => scope.violationOfItem(item, `declares ${item.name}`, "declares"));
  expect(message(constants().mustSatisfy(short).verify(app))).toBe(
    [
      "rule `constants are short` is violated in 4 place(s):",
      "  src/a.ts:4:14: [constants are short] src/a.ts declares a",
      "  src/b.ts:2:14: [constants are short] src/b.ts declares b",
      "  src/c.ts:2:14: [constants are short] src/c.ts declares c",
      "  src/d.ts:1:14: [constants are short] src/d.ts declares d",
    ].join("\n"),
  );
  const names = (made: ImportRule): string[] => {
    const result = made.evaluate(app);
    return result.ok ? result.violations.map((violation) => (violation.detail.type === "custom" ? violation.detail.message : "")) : [JSON.stringify(result.errors)];
  };
  // The files and each filter of the rule narrow the items; the scope is that of the rule.
  expect(names(rule("r").items(anyOf("src/c.ts")).mustSatisfy(short))).toEqual(["declares C", "declares c"]);
  expect(names(rule("r").items(anyOf("src/c.ts")).ofKind("type").mustSatisfy(short))).toEqual(["declares C"]);
  expect(names(constants().named("a", "d").mustSatisfy(short))).toEqual(["declares a", "declares d"]);
  const seen: string[] = [];
  rule("probe")
    .items(anyOf("src/c.ts", "src/b.ts"))
    .ofKind("type")
    .mustSatisfy(
      (scope) => {
        seen.push(`${scope.subjects.length} files, ${scope.declarations.length} declarations, ${scope.items.length} items, ${scope.imports.length} imports`);
        seen.push(`${scope.itemImports.length} imports in items, hit ${scope.imports.filter((found) => found.hits["c"] !== undefined).length}, matched ${scope.matched("d").length}`);
        return [];
      },
      { targets: { c: anyOf("src/c.ts") }, files: { d: anyOf("src/d.ts") } },
    )
    .ignoringTypeImports()
    .check(app);
  expect(seen).toEqual(["2 files, 3 declarations, 1 items, 3 imports", "0 imports in items, hit 2, matched 1"]);
  // The errors are those of a rule about items, and of the lists of the check.
  expect(names(constants().named("nothing*").mustSatisfy(short, { targets: { e: anyOf("src/e.ts") } }))).toEqual([
    JSON.stringify([
      { code: "pattern-matches-nothing", role: "name", value: "nothing*" },
      { code: "pattern-matches-nothing", role: "target", value: "src/e.ts" },
    ]),
  ]);
  // A baseline counts the violations of the items by their key.
  const file = path.join(directory, "items.baseline");
  expect(constants().mustSatisfy(short).writeBaseline(app, baseline(file))).toBeUndefined();
  expect(readFileSync(file, "utf8")).toContain("constants are short\tsrc/c.ts\tcustom declares\t1");
  expect(constants().mustSatisfy(short).verifyWithBaseline(app, baseline(file))).toBeUndefined();
});

/** A set of rules of the user: one rule for each file, or a failure when no target is given. */
class KeepsAwayFrom extends RuleSet {
  readonly name = "the files keep away";
  private readonly sources: readonly string[];
  private readonly target: string;

  constructor(sources: readonly string[], target: string) {
    super();
    this.sources = sources;
    this.target = target;
  }

  rules(): ImportRule[] {
    if (this.target === "") {
      throw new ArchitectureError({ type: "malformed", rule: this.name, message: `rule \`${this.name}\` cannot be checked:\n  no target is given` });
    }
    return this.sources.map((source) => rule(`${source} does not import ${this.target}`).files(anyOf(source)).mustNotImport(anyOf(this.target)));
  }
}

it("a class of the user that gives rules is checked as the groups are", () => {
  expect(new KeepsAwayFrom(["src/c.ts", "src/d.ts"], "src/a.ts").verify(app)).toBeUndefined();
  new KeepsAwayFrom(["src/c.ts"], "src/a.ts").check(app);
  const one = new KeepsAwayFrom(["src/c.ts", "src/b.ts"], "src/c.ts").verify(app);
  expect(one?.type === "violated" && one.rule).toBe("src/b.ts does not import src/c.ts");
  const several = new KeepsAwayFrom(["src/a.ts", "src/b.ts"], "src/c.ts");
  const failure = several.verify(app);
  expect(failure?.type === "several" && failure.failures.map((found) => found.type)).toEqual(["violated", "violated"]);
  expect(() => several.check(app)).toThrow(ArchitectureError);
  // A set without rules cannot fail, and the failure of the set itself comes through.
  expect(message(new KeepsAwayFrom([], "src/c.ts").verify(app))).toBe("rule `the files keep away` cannot be checked:\n  the group has no rules, so it cannot fail");
  expect(message(new KeepsAwayFrom(["src/a.ts"], "").verify(app))).toBe("rule `the files keep away` cannot be checked:\n  no target is given");
  expect(message(new KeepsAwayFrom(["src/a.ts"], "").verifyWithBaseline(app, baseline("unused")))).toContain("no target is given");
  // With a baseline, the violations of each rule of the set are accepted.
  const file = path.join(directory, "set.baseline");
  for (const made of several.rules()) {
    expect(made.writeBaseline(app, baseline(file))).toBeUndefined();
  }
  expect(several.verifyWithBaseline(app, baseline(file))).toBeUndefined();
  several.checkWithBaseline(app, baseline(file));
  expect(() => new KeepsAwayFrom(["src/a.ts"], "src/b.ts").checkWithBaseline(app, baseline(file))).toThrow(ArchitectureError);
  // The groups, the layers and the onion are sets of rules.
  const sets: RuleSet[] = [rules("group"), layers("layers"), onionArchitecture("onion")];
  expect(sets.map((set) => set.name)).toEqual(["group", "layers", "onion"]);
});

it("a check of the user sees markers and declarations, and reports a file as a whole", () => {
  const failure = rule("no debugger")
    .files(all())
    .mustSatisfy((scope) => scope.markers.map((seen) => scope.violationOfFile(seen.file, `has a ${seen.marker} statement`)))
    .verify(app);
  expect(message(failure)).toBe("rule `no debugger` is violated in 1 place(s):\n  src/d.ts: [no debugger] src/d.ts has a debugger statement");
  const names: string[] = [];
  rule("probe")
    .files(anyOf("src/c.ts"))
    .mustSatisfy((scope) => {
      names.push(...scope.declarations.map((declaration) => declaration.name));
      return [];
    })
    .check(app);
  expect(names).toEqual(["C", "c"]);
});

it("a baseline counts a violation of the user by its key", () => {
  const file = path.join(directory, "custom.baseline");
  const small = rule("small files").files(all()).mustSatisfy(atMost(2));
  expect(small.writeBaseline(app, baseline(file))).toBeUndefined();
  expect(readFileSync(file, "utf8")).toContain("small files\tsrc/a.ts\tcustom more than 2 imports\t1");
  expect(small.verifyWithBaseline(app, baseline(file))).toBeUndefined();
});

// The example of `docs/typescript.md` with the limits of the small project.
it("a check counts the importers of a file and its instability", () => {
  const stable = (mostImporters: number): ImportRule =>
    rule("a file that many files import is stable")
      .files(anyOf("src/**"))
      .mustSatisfy((scope) => {
        const graph = scope.graph();
        const importers = new Map<string, number>();
        for (const file of scope.facts.units) {
          for (const edge of graph.edgesOf(file)) {
            importers.set(edge.to.join("/"), (importers.get(edge.to.join("/")) ?? 0) + 1);
          }
        }
        return scope.subjects.flatMap((file) => {
          const imports = graph.edgesOf(file).length;
          const importedBy = importers.get(file.join("/")) ?? 0;
          // The share of the imports of other files among all imports of and by the file.
          const instability = imports / Math.max(imports + importedBy, 1);
          if (importedBy <= mostImporters || instability < 0.5) {
            return [];
          }
          const text = `is imported by ${importedBy} files and imports ${imports}: instability ${instability.toFixed(2)}`;
          return [scope.violationOfFile(file, text, "is not stable")];
        });
      });
  // `b.ts` is imported by `a.ts` and imports `c.ts`. `c.ts` is imported by two files and imports none.
  expect(message(stable(0).verify(app)).split("\n").slice(1)).toEqual([
    "  src/b.ts: [a file that many files import is stable] src/b.ts is imported by 1 files and imports 1: instability 0.50",
  ]);
  stable(1).check(app);
  // The filters of the rule decide what the graph has.
  stable(0).onlyKinds("import-type").check(app);
});

// The Rust test `custom_graph.rs` does the same for the graphs of a Rust check.
it("the graph of a check gives the cycles and the chains of the built-in rules", () => {
  const planted = loadProject(fileURLToPath(new URL("../../../fixtures/ts/planted/tsconfig.json", import.meta.url)));
  const self = loadProject(fileURLToPath(new URL("../tsconfig.json", import.meta.url)));
  const found = (importRule: ImportRule, project: typeof self): unknown[] => {
    const result = importRule.evaluate(project);
    if (!result.ok) {
      throw new Error(JSON.stringify(result.errors));
    }
    return result.violations.map(({ unit, detail }) => (detail.type === "cycle" ? { unit, steps: detail.steps, size: detail.size } : detail.type === "reaches" ? { unit, steps: detail.steps } : detail));
  };
  for (const filter of [(made: ImportRule) => made, (made: ImportRule) => made.ignoringTypeImports()]) {
    for (const project of [planted, self]) {
      const cycles: unknown[] = [];
      const own = rule("r").files(all()).mustSatisfy((scope) => {
        cycles.push(...scope.graph().tangles().map((tangle) => ({ unit: tangle.members[0], steps: tangle.cycle, size: tangle.members.length })));
        return [];
      });
      filter(own).check(project);
      // The planted project has a cycle with and without type imports, so the lists are not empty.
      expect(project !== planted || cycles.length > 0).toBe(true);
      expect(cycles).toEqual(found(filter(rule("r").files(all()).mustBeAcyclic()), project));
    }
    // The graph of the selected files gives the cycles of the built-in rule for any
    // selection: without `gamma`, the ring of the planted project is no cycle, though the
    // graph of all files still has it.
    for (const selection of [anyOf("src/rings/**"), anyOf("src/rings/alpha.ts", "src/rings/beta.ts")]) {
      const cycles: unknown[] = [];
      let all = 0;
      const own = rule("r").files(selection).mustSatisfy((scope) => {
        const graph = scope.subjectGraph();
        expect(scope.subjectGraph()).toBe(graph);
        cycles.push(...graph.tangles().map((tangle) => ({ unit: tangle.members[0], steps: tangle.cycle, size: tangle.members.length })));
        expect(graph.edgesOf(["src", "rings", "gamma.ts"]).length > 0).toBe(scope.selects(["src", "rings", "gamma.ts"]));
        all = scope.graph().tangles().length;
        return [];
      });
      filter(own).check(planted);
      expect(all).toBeGreaterThan(0);
      expect(cycles).toEqual(found(filter(rule("r").files(selection).mustBeAcyclic()), planted));
    }
    const chains: unknown[] = [];
    const own = rule("r").files(all()).mustSatisfy(
      (scope) => {
        const ends = new Set(scope.imports.filter((seen) => seen.hits["fs"] !== undefined).map((seen) => seen.file.join("/")));
        for (const file of scope.subjects) {
          const steps = scope.graph().chain(file, (end) => ends.has(end.join("/")));
          if (steps !== undefined) {
            chains.push({ unit: file, steps });
          }
        }
        return [];
      },
      { targets: { fs: anyOf(pkg("node:fs")) } },
    );
    filter(own).check(self);
    expect(chains.length).toBeGreaterThan(10);
    expect(chains).toEqual(found(filter(rule("r").files(all()).mustNotReach(anyOf(pkg("node:fs")))), self));
  }
});
