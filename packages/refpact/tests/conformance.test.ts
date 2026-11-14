import { readdirSync, readFileSync } from "node:fs";
import { join as joinPath } from "node:path";
import { describe, expect, it } from "vitest";
import {
  applyBaseline,
  diagram,
  evaluate,
  parseBaseline,
  renderViolation,
  renderViolations,
  type Certainty,
  type DiagramFormat,
  type DiagramLayer,
  type Constraint,
  type Demand,
  type Items,
  type Related,
  type EvaluateResult,
  type Fact,
  type FactSet,
  type Include,
  type KindFilter,
  type Path,
  type Rule,
  type Severity,
  type StaleEntry,
  type Targets,
  type Step,
  type Violation,
} from "../src/core/index.ts";

/** Splits a path or pattern as the cases write it. */
function split(text: string, separator: string): string[] {
  return text.split(separator);
}

function pathEquals(a: Path, b: Path): boolean {
  return a.length === b.length && a.every((segment, index) => segment === b[index]);
}

/** The number of cases the directory holds. A harness that runs fewer cases must fail. */
const EXPECTED_CASES = 292;

type Json = null | boolean | number | string | Json[] | { [key: string]: Json };
type JsonObject = { [key: string]: Json };

interface CaseFile {
  readonly file: string;
  readonly separator: string;
  readonly facts: FactSet;
  readonly cases: readonly {
    readonly name: string;
    readonly rule: Rule;
    readonly expect: JsonObject;
    readonly baseline?: string;
    readonly diagram?: DiagramFormat;
    readonly layers?: readonly DiagramLayer[];
  }[];
}

function object(value: Json | undefined, what: string): JsonObject {
  if (value === null || value === undefined || typeof value !== "object" || Array.isArray(value)) {
    throw new Error(`${what}: expected an object, found ${JSON.stringify(value)}`);
  }
  return value;
}

function string(value: Json | undefined, what: string): string {
  if (typeof value !== "string") {
    throw new Error(`${what}: expected a string, found ${JSON.stringify(value)}`);
  }
  return value;
}

function number(value: Json | undefined, what: string): number {
  if (typeof value !== "number") {
    throw new Error(`${what}: expected a number, found ${JSON.stringify(value)}`);
  }
  return value;
}

function list(value: Json | undefined, what: string): Json[] {
  if (value === undefined) {
    return [];
  }
  if (!Array.isArray(value)) {
    throw new Error(`${what}: expected a list, found ${JSON.stringify(value)}`);
  }
  return value;
}

function strings(value: Json | undefined, what: string): string[] {
  return list(value, what).map((entry) => string(entry, what));
}

function path(value: Json | undefined, separator: string): string[] {
  if (typeof value === "string") {
    return split(value, separator);
  }
  if (Array.isArray(value)) {
    return value.map((segment) => string(segment, "segment"));
  }
  throw new Error(`a path is a string or an array, found ${JSON.stringify(value)}`);
}

function paths(value: Json | undefined, separator: string): string[][] {
  return list(value, "paths").map((entry) => path(entry, separator));
}

function certainty(value: Json | undefined): Certainty {
  if (value === undefined || value === "exact") {
    return "exact";
  }
  if (value === "candidate") {
    return "candidate";
  }
  throw new Error(`unknown certainty ${JSON.stringify(value)}`);
}

function fact(json: JsonObject, separator: string): Fact {
  let what: Fact["what"];
  if (json["reference"] !== undefined && json["marker"] === undefined) {
    const reference = object(json["reference"], "reference");
    what = {
      type: "reference",
      target: path(reference["target"], separator),
      aliases: paths(reference["aliases"], separator),
      kind: string(reference["kind"], "kind"),
      certainty: certainty(reference["certainty"]),
      ...(reference["properties"] !== undefined ? { properties: object(reference["properties"], "properties") as Record<string, string> } : {}),
    };
  } else if (json["reference"] === undefined && typeof json["marker"] === "string") {
    what = { type: "marker", marker: json["marker"] };
  } else {
    throw new Error("a fact has exactly one of `reference` and `marker`");
  }
  const note = json["note"];
  const item = json["item"];
  return {
    unit: path(json["unit"], separator),
    file: string(json["file"], "file"),
    line: number(json["line"], "line"),
    column: json["column"] === undefined ? 1 : number(json["column"], "column"),
    tags: strings(json["tags"], "tags"),
    ...(note !== undefined && { note: string(note, "note") }),
    ...(item !== undefined && { item: path(item, separator) }),
    what,
  };
}

function factSet(json: JsonObject, separator: string): FactSet {
  return {
    separator,
    units: paths(json["units"], separator),
    unitLines: list(json["unit_lines"], "unit_lines").map((lines) => number(lines, "unit_lines")),
    items: paths(json["items"], separator),
    kinds: strings(json["kinds"], "kinds"),
    markers: strings(json["markers"], "markers"),
    tags: strings(json["tags"], "tags"),
    facts: list(json["facts"], "facts").map((entry) => fact(object(entry, "fact"), separator)),
    declarations: list(json["declarations"], "declarations").map((entry) => {
      const declaration = object(entry, "declaration");
      return {
        path: path(declaration["path"], separator),
        name: string(declaration["name"], "name"),
        unit: path(declaration["unit"], separator),
        file: string(declaration["file"], "file"),
        line: number(declaration["line"], "line"),
        column: declaration["column"] === undefined ? 1 : number(declaration["column"], "column"),
        ...(declaration["end_line"] !== undefined && { endLine: number(declaration["end_line"], "end_line") }),
        tags: strings(declaration["tags"], "tags"),
        kind: string(declaration["kind"], "kind"),
        visibility: string(declaration["visibility"], "visibility"),
        relations: list(declaration["relations"], "relations").map((tie) => {
          const relation = object(tie, "relation");
          return { relation: string(relation["relation"], "relation"), target: path(relation["target"], separator) };
        }),
        modifiers: strings(declaration["modifiers"], "modifiers"),
      };
    }),
    itemKinds: strings(json["item_kinds"], "item_kinds"),
    visibilities: strings(json["visibilities"], "visibilities"),
    relations: strings(json["relations"], "relations"),
    modifiers: strings(json["modifiers"], "modifiers"),
    properties: strings(json["properties"], "properties"),
  };
}

/** Globs: a string as it is, or an array of segments joined. */
function globs(value: Json | undefined): string[] {
  return list(value, "globs").map((entry) => (Array.isArray(entry) ? entry.map((segment) => string(segment, "segment")).join("") : string(entry, "glob")));
}

function related(value: Json | undefined, separator: string): Related {
  const json = object(value, "related");
  return { relation: string(json["relation"], "relation"), anyOf: paths(json["any_of"], separator) };
}

function targetsOf(value: Json | undefined, separator: string): Targets {
  const lists = object(value, "targets");
  return { anyOf: paths(lists["any_of"], separator), unverified: paths(lists["unverified"], separator) };
}

function items(json: JsonObject, separator: string): Items {
  const must = json["must"];
  let demand: Demand;
  if (must === "not_exist") {
    demand = { type: "not-exist" };
  } else {
    const given = object(must, "must");
    if (given["reside_in"] !== undefined) {
      demand = { type: "reside-in", places: paths(given["reside_in"], separator) };
    } else if (given["be_named"] !== undefined) {
      demand = { type: "be-named", names: globs(given["be_named"]) };
    } else if (given["not_use"] !== undefined) {
      demand = { type: "not-use", targets: targetsOf(given["not_use"], separator) };
    } else if (given["only_use"] !== undefined) {
      demand = { type: "only-use", targets: targetsOf(given["only_use"], separator) };
    } else if (given["have_modifier"] !== undefined) {
      demand = { type: "have-modifier", modifiers: strings(given["have_modifier"], "have_modifier") };
    } else if (given["not_exceed_lines"] !== undefined) {
      demand = { type: "not-exceed-lines", lines: number(given["not_exceed_lines"], "not_exceed_lines") };
    } else {
      demand = { type: "have", ...related(given["have"], separator) };
    }
  }
  return {
    ...(json["named"] !== undefined && { named: globs(json["named"]) }),
    ...(json["kinds"] !== undefined && { kinds: strings(json["kinds"], "kinds") }),
    ...(json["visibilities"] !== undefined && { visibilities: strings(json["visibilities"], "visibilities") }),
    ...(json["modifiers"] !== undefined && { modifiers: strings(json["modifiers"], "modifiers") }),
    ...(json["related"] !== undefined && { related: list(json["related"], "related").map((entry) => related(entry, separator)) }),
    must: demand,
  };
}

function rule(json: JsonObject, separator: string): Rule {
  const subjects = object(json["subjects"], "subjects");
  let include: Include;
  if (subjects["all"] === true && subjects["any_of"] === undefined) {
    include = { type: "all" };
  } else if (subjects["all"] === undefined && Array.isArray(subjects["any_of"])) {
    include = { type: "any-of", patterns: paths(subjects["any_of"], separator) };
  } else {
    throw new Error("subjects have exactly one of `all: true` and `any_of`");
  }
  const except = subjects["except"];

  let constraint: Constraint;
  const KEYS = [
    "must_not_use",
    "must_only_use",
    "must_not_contain",
    "must_be_acyclic",
    "must_be_independent",
    "must_be_used",
    "must_not_reach",
    "must_be_reachable_from",
    "must_be_in",
    "must_only_be_used_by",
    "must_not_exceed_lines",
    "items",
  ];
  const given = KEYS.filter((key) => json[key] !== undefined);
  const targets = (value: Json | undefined) => targetsOf(value, separator);
  const key = given[0];
  if (given.length !== 1 || key === undefined) {
    throw new Error("a rule has exactly one constraint");
  }
  const value = json[key];
  switch (key) {
    case "must_not_use":
      constraint = { type: "must-not-use", targets: targets(value) };
      break;
    case "must_only_use":
      constraint = { type: "must-only-use", targets: targets(value) };
      break;
    case "must_not_contain":
      constraint = { type: "must-not-contain", markers: strings(value, key) };
      break;
    case "must_be_acyclic": {
      const slices = object(value, key)["slices"];
      constraint = { type: "must-be-acyclic", ...(slices !== undefined && { slices: paths(slices, separator) }) };
      break;
    }
    case "must_be_independent":
      constraint = { type: "must-be-independent", slices: paths(object(value, key)["slices"], separator) };
      break;
    case "must_be_used":
      object(value, key);
      constraint = { type: "must-be-used" };
      break;
    case "must_not_reach":
      constraint = { type: "must-not-reach", targets: targets(value) };
      break;
    case "must_be_reachable_from":
      constraint = { type: "must-be-reachable-from", roots: paths(object(value, key)["roots"], separator) };
      break;
    case "must_be_in":
      constraint = { type: "must-be-in", places: paths(object(value, key)["places"], separator) };
      break;
    case "must_only_be_used_by":
      constraint = { type: "must-only-be-used-by", users: paths(object(value, key)["users"], separator) };
      break;
    case "must_not_exceed_lines":
      constraint = { type: "must-not-exceed-lines", lines: number(object(value, key)["lines"], "lines") };
      break;
    default:
      constraint = { type: "items", ...items(object(value, key), separator) };
  }

  let kinds: KindFilter = { type: "all" };
  if (json["kinds"] !== undefined) {
    const filter = object(json["kinds"], "kinds");
    if (Array.isArray(filter["only"]) && filter["except"] === undefined) {
      kinds = { type: "only", kinds: strings(filter["only"], "only") };
    } else if (filter["only"] === undefined && Array.isArray(filter["except"])) {
      kinds = { type: "except", kinds: strings(filter["except"], "except") };
    } else {
      throw new Error("`kinds` has exactly one of `only` and `except`");
    }
  }

  const candidates = json["candidates"];
  if (candidates !== undefined && candidates !== "include" && candidates !== "ignore") {
    throw new Error(`unknown candidates setting ${JSON.stringify(candidates)}`);
  }

  return {
    name: string(json["name"], "rule name"),
    ...(json["reason"] !== undefined && { reason: string(json["reason"], "reason") }),
    ...(json["severity"] !== undefined && { severity: string(json["severity"], "severity") as Severity }),
    subjects: { include, ...(except !== undefined && { except: paths(except, separator) }) },
    constraint,
    kinds,
    ignoreTags: strings(json["ignore_tags"], "ignore_tags"),
    properties: strings(json["properties"], "properties"),
    candidates: candidates ?? "include",
    exemptions: list(json["exemptions"], "exemptions").map((entry) => {
      const exemption = object(entry, "exemption");
      return { from: paths(exemption["from"], separator), to: paths(exemption["to"], separator) };
    }),
  };
}

function check(expectation: JsonObject, result: EvaluateResult, separator: string): void {
  const violations = expectation["violations"];
  const errors = expectation["errors"];
  if (Array.isArray(violations) && errors === undefined) {
    if (!result.ok) {
      throw new Error(`expected violations, found errors ${JSON.stringify(result.errors)}`);
    }
    expect(result.violations.length, "number of violations").toBe(violations.length);
    violations.forEach((expected, index) => {
      checkViolation(object(expected, "violation"), result.violations[index]!, separator, index);
    });
  } else if (violations === undefined && Array.isArray(errors)) {
    if (result.ok) {
      throw new Error(`expected errors, found violations ${JSON.stringify(result.violations)}`);
    }
    const wanted = errors.map((entry) => {
      const error = object(entry, "error");
      return { code: error["code"], role: error["role"], value: error["value"] };
    });
    const found = result.errors.map((error) => ({ code: error.code, role: error.role, value: error.value }));
    expect(found).toEqual(wanted);
  } else {
    throw new Error("an expectation has exactly one of `violations` and `errors`");
  }
}

function checkStale(expected: Json | undefined, stale: readonly StaleEntry[]): void {
  const wanted = list(expected, "stale").map((entry) => {
    const value = object(entry, "stale entry");
    return { unit: value["unit"], what: value["what"], count: value["count"], found: value["found"] };
  });
  expect(stale.map(({ unit, what, count, found }) => ({ unit, what, count, found }))).toEqual(wanted);
}

function checkStep(expected: JsonObject, step: Step, separator: string): void {
  for (const [key, value] of Object.entries(expected)) {
    let equal: boolean;
    switch (key) {
      case "from":
      case "to":
      case "target":
        equal = pathEquals(path(value, separator), step[key]);
        break;
      case "file":
      case "line":
      case "column":
      case "kind":
        equal = value === step[key];
        break;
      default:
        throw new Error(`unknown key \`${key}\` in an expected step`);
    }
    if (!equal) {
      throw new Error(`\`${key}\` of a step differs: expected ${JSON.stringify(value)}, found ${JSON.stringify(step)}`);
    }
  }
}

function checkViolation(expected: JsonObject, violation: Violation, separator: string, index: number): void {
  const detail = violation.detail;
  const cycle = detail.type === "cycle" ? detail : undefined;
  const chain = detail.type === "reaches" ? detail : undefined;
  const uses = detail.type === "uses" ? detail : undefined;
  const marker = detail.type === "contains" ? detail.marker : undefined;
  const item = detail.type === "item" || detail.type === "too-long" ? detail : undefined;
  const long = detail.type === "too-long" ? detail : undefined;
  const itemPath = item?.item ?? uses?.item;
  const samePath = (value: Json, actual: Path | undefined): boolean =>
    actual !== undefined && pathEquals(path(value, separator), actual);
  for (const [key, value] of Object.entries(expected)) {
    let equal: boolean;
    switch (key) {
      case "unit":
        equal = samePath(value, violation.unit);
        break;
      case "file":
        equal = value === violation.file;
        break;
      case "line":
        equal = value === violation.line;
        break;
      case "column":
        equal = value === violation.column;
        break;
      case "note":
        equal = value === violation.note;
        break;
      case "target":
        equal = samePath(value, uses?.target);
        break;
      case "written":
        equal = value === null ? uses?.written === undefined : samePath(value, uses?.written);
        break;
      case "kind":
        equal = value === uses?.kind;
        break;
      case "certainty":
        equal = value === uses?.certainty;
        break;
      case "marker":
        equal = value === marker;
        break;
      case "size":
        equal = value === cycle?.size;
        break;
      case "lines":
        equal = value === long?.lines;
        break;
      case "limit":
        equal = value === long?.limit;
        break;
      case "detail":
        equal = value === detail.type;
        break;
      case "item":
        equal = samePath(value, itemPath);
        break;
      case "item_kind":
        equal = value === item?.kind;
        break;
      case "cycle":
      case "chain": {
        const steps = list(value, key);
        const found = key === "cycle" ? cycle?.steps : chain?.steps;
        equal = found !== undefined && found.length === steps.length;
        if (equal) {
          steps.forEach((step, at) => checkStep(object(step, "step"), found?.[at] as Step, separator));
        }
        break;
      }
      case "end": {
        equal = chain !== undefined;
        for (const [name, wanted] of Object.entries(object(value, "end"))) {
          const end = chain?.end;
          if (name === "unit" || name === "target") {
            equal &&= samePath(wanted, end?.[name]);
          } else if (name === "file" || name === "line" || name === "column" || name === "kind") {
            equal &&= wanted === end?.[name];
          } else {
            throw new Error(`unknown key \`${name}\` in an expected end`);
          }
        }
        break;
      }
      default:
        throw new Error(`unknown key \`${key}\` in an expected violation`);
    }
    if (!equal) {
      throw new Error(
        `violation ${index}: \`${key}\` differs: expected ${JSON.stringify(value)}, found ${renderViolation(violation, { separator })}`,
      );
    }
  }
}

function loadCases(): CaseFile[] {
  const dir = joinPath(import.meta.dirname, "../../../conformance/cases");
  return readdirSync(dir)
    .filter((name) => name.endsWith(".json"))
    .sort()
    .map((file) => {
      const json = object(JSON.parse(readFileSync(joinPath(dir, file), "utf8")) as Json, file);
      const separator = string(json["separator"], "separator");
      return {
        file,
        separator,
        facts: factSet(object(json["facts"], "facts"), separator),
        cases: list(json["cases"], "cases").map((entry) => {
          const testCase = object(entry, "case");
          return {
            name: string(testCase["name"], "case name"),
            rule: rule(object(testCase["rule"], "rule"), separator),
            expect: object(testCase["expect"], "expect"),
            ...(testCase["baseline"] !== undefined && { baseline: string(testCase["baseline"], "baseline") }),
            ...(testCase["diagram"] !== undefined && { diagram: string(testCase["diagram"], "diagram") as DiagramFormat }),
            ...(testCase["layers"] !== undefined && {
              layers: list(testCase["layers"], "layers").map((entry) => {
                const layer = object(entry, "layer");
                const mustNotUse = layer["must_not_use"];
                return {
                  name: string(layer["name"], "layer name"),
                  units: paths(layer["units"], separator),
                  mustNotUse: mustNotUse === undefined ? [] : list(mustNotUse, "must_not_use").map((to) => number(to, "layer number")),
                };
              }),
            }),
          };
        }),
      };
    });
}

const files = loadCases();

for (const { file, separator, facts, cases } of files) {
  describe(file, () => {
    for (const testCase of cases) {
      it(testCase.name, () => {
        if (testCase.diagram !== undefined) {
          // The nodes are the layers of the case, or the slices of the constraint (SPEC 4.17).
          const { constraint } = testCase.rule;
          const slices = constraint.type === "must-be-acyclic" || constraint.type === "must-be-independent" ? constraint.slices : undefined;
          const nodes = testCase.layers !== undefined ? { layers: testCase.layers } : slices !== undefined ? { slices } : undefined;
          const drawn = diagram(testCase.rule, facts, nodes, testCase.diagram);
          if (drawn.ok) {
            expect(drawn.text).toBe(testCase.expect["diagram"]);
          } else {
            expect(testCase.expect["diagram"], "the diagram has errors").toBeUndefined();
            check(testCase.expect, drawn, separator);
          }
          return;
        }
        let result = evaluate(testCase.rule, facts);
        let stale: readonly StaleEntry[] = [];
        if (testCase.baseline !== undefined) {
          const baseline = parseBaseline(testCase.baseline);
          if (!baseline.ok) {
            expect(testCase.expect["baseline_error"], "line of the baseline that is invalid").toBe(baseline.line);
            return;
          }
          if (result.ok) {
            const applied = applyBaseline(testCase.rule.name, result.violations, baseline.entries, { separator });
            stale = applied.stale;
            result = { ok: true, violations: applied.remaining };
          }
        }
        expect(testCase.expect["baseline_error"], "the baseline was read").toBeUndefined();
        check(testCase.expect, result, separator);
        const { reason, severity } = testCase.rule;
        if (reason !== undefined && result.ok) {
          expect(renderViolations(testCase.rule.name, result.violations, { separator }, reason)).toContain(`\n  because ${reason}\n`);
        }
        if (severity === "warning" && result.ok) {
          expect(renderViolations(testCase.rule.name, result.violations, { separator }, reason, severity)).toMatch(/^warning: rule `/);
        }
        checkStale(testCase.expect["stale"], stale);
      });
    }
  });
}

it(`runs exactly ${EXPECTED_CASES} conformance cases`, () => {
  expect(files.reduce((total, file) => total + file.cases.length, 0)).toBe(EXPECTED_CASES);
});
