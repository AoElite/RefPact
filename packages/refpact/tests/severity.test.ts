// Warnings: rules whose violations are reported and do not fail the check.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { afterEach, expect, it, vi } from "vitest";
import { scratchDirectory } from "./scratch.ts";

import { anyOf, baseline, loadProject, rule, rules } from "../src/index.ts";

const directory = scratchDirectory("refpact-severity-");
const root = path.join(directory, "app");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }'],
  "src/api.ts": ['import { query } from "./db";', "export const get = query;"],
  "src/db.ts": ["export const query = 1;"],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}
const app = loadProject(path.join(root, "tsconfig.json"));
const apiUsesDb = () => rule("the api does not use the db").files(anyOf("src/api.ts")).mustNotImport(anyOf("src/db.ts")).because("the db moves");

afterEach(() => {
  vi.restoreAllMocks();
});

it("a warning passes and reports its violations", () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  expect(apiUsesDb().verify(app)?.type).toBe("violated");
  expect(warn).not.toHaveBeenCalled();
  const warning = apiUsesDb().asWarning();
  expect(warning.verify(app)).toBeUndefined();
  warning.check(app);
  expect(warn).toHaveBeenCalledTimes(2);
  expect(warn.mock.calls[0]?.[0]).toBe(
    "warning: rule `the api does not use the db` is violated in 1 place(s):\n  because the db moves\n  src/api.ts:1:10: [the api does not use the db] src/api.ts uses src/db.ts#query (import)",
  );
  // The evaluation does not change.
  const result = warning.evaluate(app);
  expect(result.ok && result.violations.length).toBe(1);
  // A set of rules fails only on its errors.
  expect(rules("set", apiUsesDb().asWarning()).verify(app)).toBeUndefined();
  expect(rules("set", apiUsesDb().asWarning(), apiUsesDb()).verify(app)?.type).toBe("violated");
});

it("a warning that cannot fail is an error", () => {
  const invalid = rule("r").files(anyOf("src/web.ts")).mustNotImport(anyOf("src/db.ts")).asWarning();
  expect(invalid.verify(app)?.type).toBe("invalid");
});

it("a warning with a baseline", () => {
  const warn = vi.spyOn(console, "warn").mockImplementation(() => {});
  const file = baseline(path.join(directory, "warnings.baseline"));
  writeFileSync(file.file, "");
  expect(apiUsesDb().asWarning().verifyWithBaseline(app, file)).toBeUndefined();
  expect(warn).toHaveBeenCalledTimes(1);
  // A stale entry still fails.
  expect(apiUsesDb().writeBaseline(app, file)).toBeUndefined();
  const gone = rule("the api does not use the db").files(anyOf("src/db.ts")).mustNotImport(anyOf("src/api.ts")).asWarning();
  expect(gone.verifyWithBaseline(app, file)?.type).toBe("stale-baseline");
});
