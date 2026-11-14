// The rule vocabulary on the planted fixture: combination, errors, filters and reports.
import path from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

import { all, allExcept, anyOf, ArchitectureError, loadProject, pkg, rule, unverified } from "../src/index.ts";
import type { AnyOf, ImportRule, Target } from "../src/index.ts";

const FIXTURE = fileURLToPath(new URL("../../../fixtures/ts/planted", import.meta.url));
const planted = loadProject(path.join(FIXTURE, "tsconfig.json"));

/** `file:line kind` of each violation. */
function lines(importRule: ImportRule): string[] {
  const result = importRule.evaluate(planted);
  if (!result.ok) {
    throw new Error(JSON.stringify(result.errors));
  }
  return result.violations.map((violation) => {
    const kind = violation.detail.type === "uses" ? violation.detail.kind : "";
    return `${violation.file}:${violation.line} ${kind}`;
  });
}

function errors(importRule: ImportRule): string[] {
  const result = importRule.evaluate(planted);
  if (result.ok) {
    throw new Error("the rule must be an error");
  }
  return result.errors.map((error) => `${error.code} ${error.role ?? ""} ${error.value ?? ""}`.trim());
}

// A list of targets must not weaken a rule: one target is caught, and the same target
// together with a second one must be caught as well, in both orders (see
// docs/comparison.md for a tool where this differs).
it("no list weakens a rule", () => {
  // a second target does not hide the first
  {
    const withTargets = (...targets: Target[]) =>
      lines(rule("r").files(anyOf("src/forms/import_named.ts")).mustNotImport(anyOf(...targets)));
    const one = withTargets("src/db/client.ts");
    expect(one).toEqual(["src/forms/import_named.ts:1 import"]);
    expect(withTargets("src/db/client.ts", "src/platform/**")).toEqual(one);
    expect(withTargets("src/platform/**", "src/db/client.ts")).toEqual(one);
    expect(withTargets(pkg("fakepkg"), "src/platform/**", "src/db/client.ts", pkg("node:net"))).toEqual(one);
  }

  // a second file pattern does not hide the first
  {
    const withFiles = (...files: string[]) => lines(rule("r").files(anyOf(...files)).mustNotImport(anyOf("src/db/client.ts")));
    const one = withFiles("src/forms/import_named.ts");
    const two = withFiles("src/forms/import_named.ts", "src/forms/import_default.ts");
    expect(one).toHaveLength(1);
    expect(two).toHaveLength(2);
    expect(two).toContain(one[0]);
    expect(withFiles("src/forms/import_default.ts", "src/forms/import_named.ts")).toEqual(two);
  }

  // exceptions remove files and nothing else
  {
    const some = rule("r").files(anyOf("src/forms/import_*.ts").except("src/forms/import_named.ts"));
    const found = lines(some.mustNotImport(anyOf("src/db/client.ts")));
    expect(found).not.toContain("src/forms/import_named.ts:1 import");
    expect(found).toContain("src/forms/import_default.ts:1 import");
    const everywhere = lines(rule("r").files(allExcept("src/forms/**", "src/db/**", "src/barrel/**", "src/rings/**")).mustNotImport(anyOf("src/db/client.ts")));
    expect(everywhere).toEqual([]);
  }
});

{
  const target = anyOf("src/db/client.ts");

  it("a rule that selects nothing is an error", () => {
    // a file pattern that matches nothing, alone and beside one that matches
    {
      expect(errors(rule("r").files(anyOf("src/nothing/**")).mustNotImport(target))).toEqual([
        "pattern-matches-nothing subject src/nothing/**",
      ]);
      expect(errors(rule("r").files(anyOf("src/forms/import_named.ts", "src/forms/gone.ts")).mustNotImport(target))).toEqual([
        "pattern-matches-nothing subject src/forms/gone.ts",
      ]);
    }

    // an exception that matches nothing
    {
      expect(errors(rule("r").files(allExcept("src/gone/**")).mustNotImport(target))).toEqual([
        "pattern-matches-nothing subject-exception src/gone/**",
      ]);
    }

    // exceptions that leave no file
    {
      expect(errors(rule("r").files(anyOf("src/db/client.ts").except("src/**")).mustNotImport(target))).toEqual(["no-subjects"]);
    }

    // a target file that does not exist, beside one that is violated
    {
      const importRule = rule("r").files(anyOf("src/forms/import_named.ts")).mustNotImport(anyOf("src/db/client.ts", "src/db/clinet.ts"));
      expect(errors(importRule)).toEqual(["pattern-matches-nothing target src/db/clinet.ts"]);
    }

    // an export that the file does not have
    {
      expect(errors(rule("r").files(all()).mustNotImport(anyOf("src/db/client.ts#qeury")))).toEqual([
        "pattern-matches-nothing target src/db/client.ts/#qeury",
      ]);
      expect(lines(rule("r").files(anyOf("src/forms/import_named.ts")).mustNotImport(anyOf("src/db/client.ts#query")))).toHaveLength(1);
    }

    // an empty list
    {
      expect(errors(rule("r").files(anyOf()).mustNotImport(target))).toEqual(["empty-list subject"]);
      expect(errors(rule("r").files(all()).mustNotImport(anyOf()))).toEqual(["empty-list target"]);
      expect(errors(rule("").files(all()).mustNotImport(target))).toEqual(["empty-name"]);
    }

    // packages are not checked for existence, and neither are unverified targets
    {
      expect(lines(rule("r").files(all()).mustNotImport(anyOf(pkg("no-such-package"))))).toEqual([]);
      expect(lines(rule("r").files(all()).mustNotImport(unverified("src/generated/schema.ts")))).toEqual([]);
      const mixed = rule("r").files(anyOf("src/forms/import_named.ts")).mustNotImport(anyOf("src/db/client.ts").andUnverified("src/generated/**"));
      expect(lines(mixed)).toHaveLength(1);
    }

    // `files` takes file patterns only
    {
      const packages = anyOf(pkg("fakepkg")) as unknown as AnyOf<string>;
      expect(() => rule("r").files(packages)).toThrow(TypeError);
    }
  });
}

{
  const typeFile = rule("r").files(anyOf("src/forms/import_type.ts")).mustNotImport(anyOf("src/db/client.ts"));

  it("kinds of import", () => {
    // all kinds count by default
    {
      expect(lines(typeFile)).toEqual([
        "src/forms/import_type.ts:1 import-type",
        "src/forms/import_type.ts:2 import-type",
        "src/forms/import_type.ts:2 import",
      ]);
    }

    // type-only imports can be left out
    {
      expect(lines(typeFile.ignoringTypeImports())).toEqual(["src/forms/import_type.ts:2 import"]);
      const reexports = rule("r").files(anyOf("src/forms/export_type_from.ts")).mustNotImport(anyOf("src/db/client.ts"));
      expect(lines(reexports)).toHaveLength(2);
      expect(lines(reexports.ignoringTypeImports())).toEqual([]);
    }

    // kinds can be selected
    {
      const forms = rule("r").files(anyOf("src/forms/*")).mustNotImport(anyOf("src/db/client.ts"));
      expect(lines(forms.onlyKinds("dynamic-import"))).toEqual(["src/forms/import_dynamic.ts:2 dynamic-import"]);
      expect(lines(forms.onlyKinds("export-from"))).toEqual([
        "src/forms/export_named_from.ts:1 export-from",
        "src/forms/export_named_from.ts:2 export-from",
        "src/forms/export_star_as.ts:1 export-from",
        "src/forms/export_star_from.ts:1 export-from",
      ]);
      expect(lines(forms.ignoringKinds("import", "import-type", "export-from", "export-type-from", "dynamic-import"))).toEqual(
        lines(forms.onlyKinds("require")),
      );
      expect(lines(forms.onlyKinds("require")).length).toBeGreaterThan(4);
    }

    // a kind that does not exist is an error
    {
      expect(errors(typeFile.onlyKinds("imports" as "import"))).toEqual(["unknown-kind kind imports"]);
    }
  });
}

it("one export of a package", () => {
  // is found as a named import and as a member of a namespace import
  {
    const thing = rule("r").files(anyOf("src/forms/package_*.ts")).mustNotImport(anyOf(pkg("fakepkg").export("thing")));
    expect(lines(thing)).toEqual(["src/forms/package_namespace.ts:3 import", "src/forms/package_root.ts:1 import"]);
  }

  // does not cover the other exports, the default export, or a subpath
  {
    const other = rule("r").files(anyOf("src/forms/package_*.ts")).mustNotImport(anyOf(pkg("fakepkg").export("other")));
    expect(lines(other)).toEqual([]);
    const fallback = rule("r").files(anyOf("src/forms/package_*.ts")).mustNotImport(anyOf(pkg("fakepkg").export("default")));
    expect(lines(fallback)).toEqual(["src/forms/package_default.ts:1 import"]);
  }

  // a subpath is its own target, and the package covers it
  {
    const sub = rule("r").files(anyOf("src/forms/package_*.ts")).mustNotImport(anyOf(pkg("fakepkg/sub")));
    expect(lines(sub)).toEqual(["src/forms/package_subpath.ts:1 import"]);
    const whole = rule("r").files(anyOf("src/forms/package_subpath.ts")).mustNotImport(anyOf(pkg("fakepkg")));
    expect(lines(whole)).toEqual(["src/forms/package_subpath.ts:1 import"]);
  }

  // a module of Node is named with or without `node:`
  {
    const builtin = (name: string) => lines(rule("r").files(anyOf("src/forms/node_builtin.ts")).mustNotImport(anyOf(pkg(name))));
    expect(builtin("node:fs")).toEqual(["src/forms/node_builtin.ts:1 import", "src/forms/node_builtin.ts:3 import"]);
    expect(builtin("fs")).toEqual(builtin("node:fs"));
    expect(builtin("fs/promises")).toEqual(["src/forms/node_builtin.ts:3 import"]);
    expect(builtin("child_process")).toEqual(["src/forms/node_builtin.ts:2 import"]);
  }

  // a package pattern can match a part of the name
  {
    const scope = rule("r").files(anyOf("src/forms/package_*.ts")).mustNotImport(anyOf(pkg("@fixture/*")));
    expect(lines(scope)).toEqual(["src/forms/package_scoped.ts:1 import"]);
  }
});

{
  const violated = rule("no database here").files(anyOf("src/forms/through_barrel.ts")).mustNotImport(anyOf("src/db/client.ts"));

  it("reports", () => {
    // a violation has file, line, rule name and path
    {
      const failure = violated.onlyKinds("import-type").verify(planted);
      expect(failure?.message).toBe(
        "rule `no database here` is violated in 1 place(s):\n" +
          "  src/forms/through_barrel.ts:3:15: [no database here] src/forms/through_barrel.ts uses src/db/client.ts#Row " +
          "(import-type, written as src/db/index.ts#Row)",
      );
    }

    // `check` throws the report, and passes without a violation
    {
      expect(() => violated.check(planted)).toThrow(ArchitectureError);
      expect(() => violated.check(planted)).toThrow(/is violated in 3 place/);
      const clean = rule("r").files(anyOf("src/forms/local_names.ts")).mustNotImport(anyOf("src/db/client.ts"));
      expect(clean.verify(planted)).toBeUndefined();
      clean.check(planted);
    }

    // an error of the rule is a failure too
    {
      const invalid = rule("typo").files(anyOf("src/nope/**")).mustNotImport(anyOf("src/db/client.ts"));
      expect(invalid.verify(planted)?.message).toBe(
        "rule `typo` cannot be checked:\n  subject pattern `src/nope/**` matches nothing in the analysed code",
      );
      expect(() => invalid.check(planted)).toThrow(ArchitectureError);
    }
  });
}
