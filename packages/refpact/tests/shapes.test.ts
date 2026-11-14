// Allow-lists, cycles and baselines, on the planted fixture (`src/rings`, `src/features`),
// and rules across the packages of a workspace.
import { existsSync, mkdirSync, readFileSync, symlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { all, allExcept, anyOf, ArchitectureError, baseline, loadProject, pkg, rule, unverified } from "../src/index.ts";
import type { ImportRule, Project, Violation } from "../src/index.ts";

const FIXTURE = fileURLToPath(new URL("../../../fixtures/ts/planted", import.meta.url));
const planted = loadProject(path.join(FIXTURE, "tsconfig.json"));
const render = (path: readonly string[]): string => printPath(path, planted.facts);

const scratch = scratchDirectory("refpact-shapes-");

function violations(importRule: ImportRule, project: Project = planted): readonly Violation[] {
  const result = importRule.evaluate(project);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations;
}

/** Line, kind and path of each violation. */
function uses(importRule: ImportRule, project: Project = planted): string[] {
  return violations(importRule, project).map((violation) => {
    const detail = violation.detail;
    if (detail.type !== "uses") {
      throw new Error(`not a use: ${detail.type}`);
    }
    const written = detail.written !== undefined ? ` written=${render(detail.written)}` : "";
    return `${violation.file}:${violation.line} ${detail.kind} ${render(detail.target)}${written}`;
  });
}

/** The steps of each cycle, and the size of the tangle. */
function cycles(importRule: ImportRule, project: Project = planted): { steps: string[]; size: number }[] {
  return violations(importRule, project).map((violation) => {
    const detail = violation.detail;
    if (detail.type !== "cycle") {
      throw new Error(`not a cycle: ${detail.type}`);
    }
    return {
      steps: detail.steps.map((step) => `${step.from.join("/")} -> ${step.to.join("/")} at ${step.file}:${step.line} ${step.kind}`),
      size: detail.size,
    };
  });
}

const strict = (...allowed: Parameters<typeof anyOf>): ImportRule =>
  rule("strict imports little").files(anyOf("src/rings/strict.ts")).mustOnlyImport(anyOf(...allowed));

it("allow-lists", () => {
  // an allow-list reports every other import
  {
    expect(uses(strict(pkg("node:*")))).toEqual([
      "src/rings/strict.ts:2 import fakepkg#thing",
      "src/rings/strict.ts:3 import src/db/index.ts#query",
      "src/rings/strict.ts:4 import src/rings/beta.ts#beta",
      "src/rings/strict.ts:5 import-type src/rings/gamma.ts#Gamma",
    ]);
  }

  // `pkg("*")` is every package, and not the modules of Node
  {
    expect(uses(strict(pkg("*"), "src/rings/**", "src/db/**"))).toEqual(["src/rings/strict.ts:1 import node:fs#readFileSync"]);
  }

  // an import is allowed when the file that declares the name is allowed
  {
    // `../db` is the barrel `src/db/index.ts`; `query` is declared in `src/db/client.ts`.
    expect(uses(strict(pkg("node:fs"), pkg("fakepkg"), "src/rings/**", "src/db/client.ts"))).toEqual([]);
  }

  // one export can be allowed
  {
    expect(uses(strict(pkg("node:fs").export("readFileSync"), pkg("fakepkg").export("other"), "src/**"))).toEqual([
      "src/rings/strict.ts:2 import fakepkg#thing",
    ]);
  }

  // type imports can be left out
  {
    expect(uses(strict(pkg("*"), pkg("node:*"), "src/db/**", "src/rings/beta.ts").ignoringTypeImports())).toEqual([]);
    expect(uses(strict(pkg("*"), pkg("node:*"), "src/db/**", "src/rings/beta.ts"))).toEqual([
      "src/rings/strict.ts:5 import-type src/rings/gamma.ts#Gamma",
    ]);
  }

  // a complete allow-list passes
  {
    strict(pkg("node:fs"), pkg("fakepkg"), "src/db/**", "src/rings/**").check(planted);
  }

  // an allowed file that does not exist is an error
  {
    const result = strict(pkg("node:fs"), "src/ringz/**").evaluate(planted);
    expect(result).toMatchObject({ ok: false, errors: [{ code: "pattern-matches-nothing", role: "target", value: "src/ringz/**" }] });
  }

  // an unverified target is allowed and not checked
  {
    const result = rule("r").files(anyOf("src/rings/alpha.ts")).mustOnlyImport(unverified("src/rings/beta.ts", "src/generated/**")).evaluate(planted);
    expect(result).toEqual({ ok: true, violations: [] });
  }
});

const RING = [
  "src/rings/alpha.ts -> src/rings/beta.ts at src/rings/alpha.ts:2 import",
  "src/rings/beta.ts -> src/rings/gamma.ts at src/rings/beta.ts:1 import-type",
  "src/rings/gamma.ts -> src/rings/alpha.ts at src/rings/gamma.ts:1 import",
];
const FEATURES = [
  "src/features/cart -> src/features/pay at src/features/cart/total.ts:1 import",
  "src/features/pay -> src/features/shared at src/features/pay/index.ts:1 import",
  "src/features/shared -> src/features/cart at src/features/shared/format.ts:1 import",
];

it("cycles", () => {
  // a cycle between files is reported once, with its steps
  {
    const acyclic = rule("no cycles").files(anyOf("src/rings/**")).mustBeAcyclic();
    expect(cycles(acyclic)).toEqual([{ steps: RING, size: 3 }]);
    expect(violations(acyclic)[0]).toMatchObject({ unit: ["src", "rings", "alpha.ts"], file: "src/rings/alpha.ts", line: 2 });
  }

  // a cycle through a type import is gone when type imports are left out
  {
    expect(cycles(rule("no cycles").files(anyOf("src/rings/**")).mustBeAcyclic().ignoringTypeImports())).toEqual([]);
  }

  // an `export .. from` is a step of a cycle
  {
    expect(cycles(rule("no cycles").files(anyOf("src/features/**")).mustBeAcyclic())).toEqual([
      {
        steps: [
          "src/features/cart/index.ts -> src/features/cart/total.ts at src/features/cart/index.ts:1 export-from",
          "src/features/cart/total.ts -> src/features/pay/index.ts at src/features/cart/total.ts:1 import",
          "src/features/pay/index.ts -> src/features/shared/format.ts at src/features/pay/index.ts:1 import",
          "src/features/shared/format.ts -> src/features/cart/index.ts at src/features/shared/format.ts:1 import",
        ],
        size: 4,
      },
    ]);
  }

  // slices join the files of a directory
  {
    const between = rule("no cycles between features").files(anyOf("src/features/**")).mustBeAcyclicBetween(anyOf("src/features/*"));
    // `solo` imports `shared` and is not in the tangle.
    expect(cycles(between)).toEqual([{ steps: FEATURES, size: 3 }]);
  }

  // each tangle is reported, in the order of the files
  {
    const everywhere = rule("no cycles").files(anyOf("src/features/**", "src/rings/**")).mustBeAcyclic();
    expect(cycles(everywhere).map((cycle) => cycle.size)).toEqual([4, 3]);
  }

  // files without a cycle pass
  {
    rule("no cycles").files(anyOf("src/db/**", "src/forms/import_*.ts")).mustBeAcyclic().check(planted);
  }

  // a cycle rule that cannot fail is an error
  {
    const one = rule("r").files(anyOf("src/rings/alpha.ts")).mustBeAcyclic().evaluate(planted);
    expect(one).toMatchObject({ ok: false, errors: [{ code: "too-few-nodes" }] });
    const noSlice = rule("r").files(anyOf("src/features/**")).mustBeAcyclicBetween(anyOf("src/features/*", "src/feature/*")).evaluate(planted);
    expect(noSlice).toMatchObject({ ok: false, errors: [{ code: "pattern-matches-nothing", role: "slice", value: "src/feature/*" }] });
  }

  // a cycle is printed with every step
  {
    const failure = rule("no cycles").files(anyOf("src/rings/**")).mustBeAcyclic().verify(planted);
    expect(failure?.message).toBe(
      [
        "rule `no cycles` is violated in 1 place(s):",
        "  src/rings/alpha.ts:2:10: [no cycles] src/rings/alpha.ts is in a cycle of 3 (the tangle has 3 in all):",
        "      src/rings/alpha.ts -> src/rings/beta.ts  at src/rings/alpha.ts:2:10, uses src/rings/beta.ts#beta (import)",
        "      src/rings/beta.ts -> src/rings/gamma.ts  at src/rings/beta.ts:1:15, uses src/rings/gamma.ts#Gamma (import-type)",
        "      src/rings/gamma.ts -> src/rings/alpha.ts  at src/rings/gamma.ts:1:10, uses src/rings/alpha.ts#alpha (import)",
      ].join("\n"),
    );
  }
});

const HEADER =
  "# refpact baseline: violations that are accepted for now.\n# Columns, separated by tabs: rule, unit, what, count.\n";
const noDb = (): ImportRule => rule("strict does not import the database").files(anyOf("src/rings/strict.ts")).mustNotImport(anyOf("src/db/**"));
const DB_LINE = "strict does not import the database\tsrc/rings/strict.ts\tuses import src/db/index.ts#query\t1\n";
let files = 0;
const newFile = (): string => path.join(scratch, `nested-${(files += 1)}`, "architecture.baseline");

it("baselines", () => {
  // a rule does not use a baseline unless it is given one
  {
    const file = newFile();
    expect(noDb().writeBaseline(planted, baseline(file))).toBeUndefined();
    // The file accepts the violation, and the plain check still fails.
    expect(noDb().verify(planted)).toMatchObject({ type: "violated", violations: [{ line: 3 }] });
    expect(() => noDb().check(planted)).toThrow(ArchitectureError);
    noDb().checkWithBaseline(planted, baseline(file));
  }

  // a written baseline has one line for each key and no line numbers
  {
    const file = newFile();
    noDb().writeBaseline(planted, baseline(file));
    expect(readFileSync(file, "utf8")).toBe(HEADER + DB_LINE);
  }

  // a violation that the baseline does not accept fails
  {
    const file = newFile();
    const wider = rule("strict does not import the database").files(anyOf("src/rings/strict.ts")).mustNotImport(anyOf("src/db/**", pkg("fakepkg")));
    noDb().writeBaseline(planted, baseline(file));
    expect(wider.verifyWithBaseline(planted, baseline(file))).toMatchObject({ type: "violated", violations: [{ line: 2 }] });
    expect(() => wider.checkWithBaseline(planted, baseline(file))).toThrow(/fakepkg#thing/);
  }

  // a baseline file that is not there accepts nothing
  {
    expect(noDb().verifyWithBaseline(planted, baseline(newFile()))).toMatchObject({ type: "violated", violations: [{ line: 3 }] });
  }

  // an entry for a violation that is gone fails
  {
    const file = newFile();
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, `${DB_LINE}strict does not import the database\tsrc/rings/strict.ts\tuses require src/db/client.ts\t2\n`);
    const failure = noDb().verifyWithBaseline(planted, baseline(file));
    expect(failure).toMatchObject({
      type: "stale-baseline",
      entries: [{ unit: "src/rings/strict.ts", what: "uses require src/db/client.ts", count: 2, found: 0 }],
    });
    expect(failure?.message).toContain("uses require src/db/client.ts (the baseline accepts 2, the code has 0)");
    expect(failure?.message).toContain("REFPACT_BASELINE=update");
  }

  // rules share a file and keep the entries of each other
  {
    const file = newFile();
    const acyclic = rule("no cycles").files(anyOf("src/rings/**")).mustBeAcyclic();
    noDb().writeBaseline(planted, baseline(file));
    acyclic.writeBaseline(planted, baseline(file));
    const cycleLine = "no cycles\tsrc/rings/alpha.ts\tcycle src/rings/alpha.ts > src/rings/beta.ts > src/rings/gamma.ts\t1\n";
    expect(readFileSync(file, "utf8")).toBe(HEADER + cycleLine + DB_LINE);
    noDb().writeBaseline(planted, baseline(file));
    expect(readFileSync(file, "utf8")).toBe(HEADER + cycleLine + DB_LINE);
    acyclic.checkWithBaseline(planted, baseline(file));
    noDb().checkWithBaseline(planted, baseline(file));
  }

  // writing a rule without violations removes its entries
  {
    const file = newFile();
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, "clean\tsrc/rings/alpha.ts\tuses import src/db/gone.ts\t1\nother\tsrc/a.ts\tuses import x\t1\n");
    rule("clean").files(anyOf("src/rings/alpha.ts")).mustNotImport(anyOf("src/db/**")).writeBaseline(planted, baseline(file));
    expect(readFileSync(file, "utf8")).toBe(`${HEADER}other\tsrc/a.ts\tuses import x\t1\n`);
  }

  // a baseline that cannot be read fails with the line, and is not replaced
  {
    const file = newFile();
    mkdirSync(path.dirname(file), { recursive: true });
    writeFileSync(file, `${HEADER}${DB_LINE}this line has no tabs\n`);
    expect(noDb().verifyWithBaseline(planted, baseline(file))).toMatchObject({
      type: "baseline",
      message: `${file}:4: this line of the baseline is not valid`,
    });
    expect(noDb().writeBaseline(planted, baseline(file))).toMatchObject({ type: "baseline" });
    expect(readFileSync(file, "utf8")).toContain("this line has no tabs");
  }

  // a rule that cannot be checked fails also with a baseline
  {
    const file = newFile();
    const wrong = rule("typo").files(anyOf("src/rings/strikt.ts")).mustNotImport(anyOf("src/db/**"));
    expect(wrong.verifyWithBaseline(planted, baseline(file))).toMatchObject({ type: "invalid" });
    expect(wrong.writeBaseline(planted, baseline(file))).toMatchObject({ type: "invalid" });
    expect(existsSync(file)).toBe(false);
  }

  // a rule name with a tab cannot have a baseline
  {
    const named = rule("a\tb").files(anyOf("src/rings/strict.ts")).mustNotImport(anyOf("src/db/**"));
    expect(named.verifyWithBaseline(planted, baseline(newFile()))).toMatchObject({ type: "baseline" });
  }

  // the environment variable makes the check write the baseline
  {
    const file = newFile();
    const before = process.env["REFPACT_BASELINE"];
    try {
      process.env["REFPACT_BASELINE"] = "yes";
      expect(noDb().verifyWithBaseline(planted, baseline(file))).toMatchObject({ type: "violated" });
      expect(existsSync(file)).toBe(false);
      process.env["REFPACT_BASELINE"] = "update";
      noDb().checkWithBaseline(planted, baseline(file));
      // The plain check does not read the variable or the file.
      expect(noDb().verify(planted)).toMatchObject({ type: "violated" });
    } finally {
      if (before === undefined) {
        delete process.env["REFPACT_BASELINE"];
      } else {
        process.env["REFPACT_BASELINE"] = before;
      }
    }
    expect(readFileSync(file, "utf8")).toBe(HEADER + DB_LINE);
    noDb().checkWithBaseline(planted, baseline(file));
  }
});

{
  // Two packages with a tsconfig file each. `web` has `core` in its `node_modules`, as a
  // link to the directory of the package, the way pnpm, npm and yarn make it.
  const root = path.join(scratch, "workspace");
  const put = (file: string, text: string): void => {
    mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
    writeFileSync(path.join(root, file), text);
  };
  const tsconfig = '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }';
  put("packages/core/package.json", '{ "name": "@shop/core", "type": "module", "exports": { ".": "./src/index.ts" } }');
  put("packages/core/tsconfig.json", tsconfig);
  put("packages/core/src/index.ts", 'export { price } from "./price";\n');
  put("packages/core/src/price.ts", "export const price = (): number => 1;\n");
  put("packages/core/src/debug.ts", 'import { render } from "@shop/web";\nexport const debug = render;\n');
  put("packages/web/package.json", '{ "name": "@shop/web", "type": "module", "exports": { ".": "./src/index.ts" } }');
  put("packages/web/tsconfig.json", tsconfig);
  put("packages/web/src/index.ts", 'import { price } from "@shop/core";\nexport const render = (): number => price();\n');
  put("packages/web/src/whole.ts", 'import * as core from "@shop/core";\nexport const whole = core;\n');
  mkdirSync(path.join(root, "packages/web/node_modules/@shop"), { recursive: true });
  symlinkSync(path.join(root, "packages/core"), path.join(root, "packages/web/node_modules/@shop/core"), "dir");
  mkdirSync(path.join(root, "packages/core/node_modules/@shop"), { recursive: true });
  symlinkSync(path.join(root, "packages/web"), path.join(root, "packages/core/node_modules/@shop/web"), "dir");
  const workspace = loadProject(["packages/core/tsconfig.json", "packages/web/tsconfig.json"].map((file) => path.join(root, file)), { root });

  it("the packages of a workspace", () => {
    // the files of all packages are units, relative to the root
    {
      expect(workspace.files).toEqual([
        "packages/core/src/debug.ts",
        "packages/core/src/index.ts",
        "packages/core/src/price.ts",
        "packages/web/src/index.ts",
        "packages/web/src/whole.ts",
      ]);
    }

    // an import of a package of the workspace is an import of its files
    {
      const byPath = rule("web does not import the price file").files(anyOf("packages/web/**")).mustNotImport(anyOf("packages/core/src/price.ts"));
      expect(uses(byPath, workspace)).toEqual(["packages/web/src/index.ts:1 import packages/core/src/price.ts#price written=@shop/core#price"]);
      // An import of the package as a whole is an import of the file that the package names.
      const entry = rule("web does not import the entry of core").files(anyOf("packages/web/src/whole.ts")).mustNotImport(anyOf("packages/core/src/index.ts"));
      expect(uses(entry, workspace)).toEqual(["packages/web/src/whole.ts:1 import packages/core/src/index.ts written=@shop/core"]);
      const byName = rule("core does not import web").files(anyOf("packages/core/**")).mustNotImport(anyOf(pkg("@shop/web")));
      expect(uses(byName, workspace)).toEqual(["packages/core/src/debug.ts:1 import @shop/web#render"]);
    }

    // a package may only import the packages in a list
    {
      const only = (...allowed: Parameters<typeof anyOf>): ImportRule =>
        rule("web imports core only").files(anyOf("packages/web/**")).mustOnlyImport(anyOf(...allowed));
      only(pkg("@shop/core")).check(workspace);
      only("packages/core/**").check(workspace);
      expect(uses(only("packages/web/**"), workspace)).toEqual([
        "packages/web/src/index.ts:1 import @shop/core#price",
        "packages/web/src/whole.ts:1 import @shop/core",
      ]);
    }

    // packages must not form a cycle
    {
      const acyclic = rule("no cycles between packages").files(all()).mustBeAcyclicBetween(anyOf("packages/*"));
      expect(cycles(acyclic, workspace)).toEqual([
        {
          steps: [
            "packages/core -> packages/web at packages/core/src/debug.ts:1 import",
            "packages/web -> packages/core at packages/web/src/index.ts:1 import",
          ],
          size: 2,
        },
      ]);
      rule("no cycles between packages").files(allExcept("packages/core/src/debug.ts")).mustBeAcyclicBetween(anyOf("packages/*")).check(workspace);
    }
  });
}
