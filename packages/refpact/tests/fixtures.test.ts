// The fixtures are real projects. If they did not pass the compiler, the planted forms
// would not be proof that the library reads real TypeScript.
import { execFileSync } from "node:child_process";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

// The compiler under test, not whichever one owns the name `tsc` in `node_modules/.bin`.
const compiler = process.env["REFPACT_TYPESCRIPT"] || "typescript";
const tsc = fileURLToPath(new URL(`../node_modules/${compiler}/bin/tsc`, import.meta.url));

function compile(fixture: string, ...args: string[]): string {
  const cwd = fileURLToPath(new URL(`../../../fixtures/ts/${fixture}`, import.meta.url));
  try {
    return execFileSync(process.execPath, [tsc, ...args], { cwd, encoding: "utf8", timeout: 120_000 });
  } catch (error) {
    // A compiler that was stopped at the limit has written nothing, which is not a pass.
    const failed = error as { stdout?: string; signal?: string | null };
    return typeof failed.signal === "string" ? `the compiler did not end in time (${failed.signal})` : (failed.stdout ?? String(error));
  }
}

it("the clean fixture passes the compiler", () => {
  expect(compile("app", "--noEmit", "-p", "tsconfig.json")).toBe("");
});

// `-b` builds the referenced project first, as its project reference requires.
it("the planted fixture passes the compiler", () => {
  expect(compile("planted", "-b")).toBe("");
});
