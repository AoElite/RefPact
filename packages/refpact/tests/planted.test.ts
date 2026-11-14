// Every detection form, on the planted fixture.
//
// Each file of `fixtures/ts/planted/src/forms` marks its violations with `//~ kind path`
// comments; `written=path` gives the path as the import names it when a re-export is
// followed. One test runs the rule on all of them and requires that the violations are
// exactly the marked ones: the same file, line, kind and path. A form that the library stops
// detecting fails that test, and its message names the missing violations.
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";

import { anyOf, loadProject, pkg, rule } from "../src/index.ts";
import type { ImportRule, Violation } from "../src/index.ts";
import { checks } from "../src/frontend/checks.ts";

// The cross-checks are on in this file: each short way of the front end is compared with the
// long way for every project loaded here, and a difference is an error.
checks.aliases = true;
checks.walk = true;

const FIXTURE = fileURLToPath(new URL("../../../fixtures/ts/planted", import.meta.url));
const planted = loadProject(path.join(FIXTURE, "tsconfig.json"));

/** The targets that the planted fixture imports on purpose. */
const FORBIDDEN = anyOf(
  "src/db/client.ts",
  "src/internal/**",
  "shared/**",
  pkg("node:fs"),
  pkg("node:child_process"),
  pkg("fakepkg"),
  pkg("@fixture/scoped"),
);

function imports(...files: string[]): ImportRule {
  return rule("planted").files(anyOf(...files)).mustNotImport(FORBIDDEN);
}

/** One violation in the notation of the `//~` comments. */
function notation(violation: Violation): string {
  const render = (segments: readonly string[]): string => printPath(segments, planted.facts);
  const detail = violation.detail;
  if (detail.type !== "uses") {
    return `${violation.file}:${violation.line} ${detail.type}`;
  }
  const written = detail.written !== undefined ? ` written=${render(detail.written)}` : "";
  return `${violation.file}:${violation.line} ${detail.kind} ${render(detail.target)}${written}`;
}

function found(importRule: ImportRule): string[] {
  const result = importRule.evaluate(planted);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations.map(notation).sort();
}

/** The `//~` comments of one fixture file. */
function marked(file: string): string[] {
  const lines = readFileSync(path.join(FIXTURE, file), "utf8").split("\n");
  return lines.flatMap((line, index) => line.split("//~ ").slice(1).map((comment) => `${file}:${index + 1} ${comment.trim()}`));
}

// Every form of import in one test: the rule finds exactly the violations that the `//~`
// comments of `src/forms` mark. `local_names.ts` marks none; a violation found there is an
// extra one.
it("every planted violation is found, and nothing else", () => {
  const files = readdirSync(path.join(FIXTURE, "src/forms")).map((file) => `src/forms/${file}`);
  const expected = files.flatMap(marked).sort();
  const actual = found(imports("src/forms/**"));
  expect({
    "not found": expected.filter((entry) => !actual.includes(entry)),
    "not marked": actual.filter((entry) => !expected.includes(entry)),
  }).toEqual({ "not found": [], "not marked": [] });
  expect(actual).toEqual(expected);
  expect(expected.length).toBeGreaterThan(40);
});

// `references` lead to other tsconfig files; the files are relative to the root, without
// packages and default libraries; the root can be another directory.
it("the project is loaded the way the compiler sees it", () => {
  expect(planted.configFiles.map((file) => path.relative(FIXTURE, file))).toEqual([
    "tsconfig.json",
    path.join("shared", "tsconfig.json"),
  ]);
  expect(planted.files).toContain("shared/src/index.ts");
  expect(planted.files).toContain("src/db/client.ts");
  expect(planted.files).toContain("src/forms/require_in_js.cjs");
  expect(planted.files.filter((file) => file.includes("node_modules") || file.startsWith(".."))).toEqual([]);

  const inner = loadProject(path.join(FIXTURE, "tsconfig.json"), { root: path.join(FIXTURE, "src") });
  expect(inner.files).toContain("db/client.ts");
  expect(inner.files).not.toContain("shared/src/index.ts");
  const violations = rule("r").files(anyOf("forms/import_named.ts")).mustNotImport(anyOf("db/client.ts")).evaluate(inner);
  expect(violations).toMatchObject({ ok: true, violations: [{ file: "forms/import_named.ts", line: 1 }] });

  expect(() => loadProject(path.join(FIXTURE, "nothing.json"))).toThrow(/does not exist/);
});
