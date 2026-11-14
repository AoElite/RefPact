import { expect, it } from "vitest";
import { printPath, renderErrors, renderViolations, type Violation } from "../src/core/index.ts";

const violations: readonly Violation[] = [
  {
    rule: "r",
    unit: ["src/app/main.ts"],
    file: "src/app/main.ts",
    line: 2,
    column: 1,
    note: "via barrel",
    detail: {
      type: "uses",
      target: ["src/db/client.ts", "#query"],
      written: ["src/db/index.ts", "#query"],
      kind: "import",
      certainty: "candidate",
    },
  },
  { rule: "r", unit: ["crate", "a"], file: "src/a.rs", line: 3, column: 5, detail: { type: "contains", marker: "unsafe" } },
];

it("report", () => {
  // joins paths with the separator
  {
    expect(renderViolations("r", violations, { separator: "::" })).toBe(
      [
        "rule `r` is violated in 2 place(s):",
        "  src/app/main.ts:2:1: [r] src/app/main.ts uses src/db/client.ts::#query (import, written as src/db/index.ts::#query, candidate, via barrel)",
        "  src/a.rs:3:5: [r] crate::a contains unsafe",
      ].join("\n"),
    );
  }

  // joins a segment with an attached prefix without the separator, and hides a prefix of the first segment
  {
    expect(printPath(["npm:lodash", "fp", "#map"], { separator: "/", attachedPrefixes: ["#"], hiddenPrefixes: ["npm:"] })).toBe("lodash/fp#map");
    expect(renderViolations("r", violations.slice(0, 1), { separator: "/", attachedPrefixes: ["#"] })).toBe(
      "rule `r` is violated in 1 place(s):\n" +
        "  src/app/main.ts:2:1: [r] src/app/main.ts uses src/db/client.ts#query (import, written as src/db/index.ts#query, candidate, via barrel)",
    );
  }

  // renders errors
  {
    expect(
      renderErrors("r", [
        { code: "empty-name" },
        { code: "pattern-matches-nothing", role: "subject-exception", value: "crate::x" },
        { code: "empty-list", role: "target" },
      ]),
    ).toBe(
      "rule `r` cannot be checked:\n" +
        "  the rule has no name\n" +
        "  subject exception `crate::x` matches nothing in the analysed code\n" +
        "  the target pattern list is empty",
    );
  }
});
