/** The baseline file. */
import { existsSync, mkdirSync, readFileSync, renameSync, writeFileSync } from "node:fs";
import path from "node:path";

import { baselineEntries, formatBaseline, parseBaseline } from "./core/index.ts";
import type { BaselineEntry, PathFormat, Violation } from "./core/index.ts";
import type { Failure } from "./rules.ts";

/**
 * A file of violations that the project accepts for now.
 *
 * A rule uses a baseline only when it is checked with `checkWithBaseline`. Plain `check`
 * never reads one.
 *
 * The file is text, one accepted violation per line, without line numbers of the source.
 * Commit it. To write it, run the tests once with `REFPACT_BASELINE=update`.
 */
export class Baseline {
  /** The absolute path of the file. */
  readonly file: string;

  /** @internal */
  constructor(file: string) {
    this.file = path.resolve(file);
  }

  /** @internal The entries, or the reason why the file cannot be read. A file that is not there is empty. */
  read(): readonly BaselineEntry[] | Failure {
    if (!existsSync(this.file)) {
      return [];
    }
    let text: string;
    try {
      text = readFileSync(this.file, "utf8");
    } catch (error) {
      return { type: "baseline", message: `refpact: cannot read ${this.file}: ${(error as Error).message}` };
    }
    const parsed = parseBaseline(text);
    if (!parsed.ok) {
      return { type: "baseline", message: `${this.file}:${parsed.line}: this line of the baseline is not valid` };
    }
    return parsed.entries;
  }

  /** @internal Replaces the entries of one rule with its current violations. */
  write(rule: string, violations: readonly Violation[], format: PathFormat): Failure | undefined {
    const entries = this.read();
    if (!Array.isArray(entries)) {
      return entries as Failure;
    }
    const kept = (entries as readonly BaselineEntry[]).filter((entry) => entry.rule !== rule);
    try {
      writeFile(this.file, formatBaseline([...kept, ...baselineEntries(violations, format)]));
    } catch (error) {
      return { type: "baseline", message: `refpact: cannot write ${this.file}: ${(error as Error).message}` };
    }
    return undefined;
  }
}

/** Writes a file, and the directory of the file when it is not there. @internal */
export function writeFile(file: string, text: string): void {
  mkdirSync(path.dirname(file), { recursive: true });
  // Written beside the file and moved, so that a reader never sees half a file.
  const partial = `${file}.partial`;
  writeFileSync(partial, text);
  renameSync(partial, file);
}

/**
 * The baseline in the file at `file`. A relative path is relative to the working directory;
 * in a test, give a path from `import.meta.dirname`.
 */
export function baseline(file: string): Baseline {
  return new Baseline(file);
}
