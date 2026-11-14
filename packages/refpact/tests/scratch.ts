// Temporary directories for the tests of one file.
import { mkdtempSync, rmSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";
import { afterAll } from "vitest";

const made = new Set<string>();

function remove(dir: string): void {
  rmSync(dir, { recursive: true, force: true });
  made.delete(dir);
}

// Vitest does not run `afterAll` of a file that fails while it loads, which a mutation of the
// source often makes happen. The process removes what is left when it exits.
process.once("exit", () => {
  for (const dir of made) {
    remove(dir);
  }
});

/** A new empty directory, removed after the tests of the file, or at the latest when the process exits. */
export function scratchDirectory(prefix: string): string {
  const dir = mkdtempSync(path.join(tmpdir(), prefix));
  made.add(dir);
  afterAll(() => remove(dir));
  return dir;
}
