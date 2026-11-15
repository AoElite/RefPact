/** Patterns of rules as the core sees them: lists of segments. */
import type { RawPattern, Subjects } from "./core/index.ts";

/** What a target of a rule that is no file pattern has: its segments. */
interface Segmented {
  readonly segments: RawPattern;
}

/**
 * A file pattern as segments. `src/db/**` selects by path, `src/db/client.ts#query` names
 * one export of a file, and `src/domain/order.ts#Order#total` one member of an export.
 */
export function filePattern(pattern: string): RawPattern {
  const hash = pattern.indexOf("#");
  const file = (hash === -1 ? pattern : pattern.slice(0, hash)).replace(/^\.\//, "");
  const segments = file.split("/");
  const names = hash === -1 ? [] : pattern.slice(hash + 1).split("#");
  return [...segments, ...names.map((name) => `#${name}`)];
}

/** A target as segments: a file pattern, or a package or a global. */
export function targetPattern(target: string | Segmented): RawPattern {
  return typeof target === "string" ? filePattern(target) : target.segments;
}

/** What a selection of files has: `all()`, `allExcept(..)` or `anyOf(..).except(..)`. */
interface Selected {
  readonly include: readonly string[] | undefined;
  readonly excluded: readonly string[] | undefined;
}

/** These files as a selection of the core: the subjects of a rule, or the files of an exemption. */
export function subjects(files: Selected): Subjects {
  return {
    include: files.include === undefined ? { type: "all" } : { type: "any-of", patterns: files.include.map(filePattern) },
    ...(files.excluded !== undefined ? { except: files.excluded.map(filePattern) } : {}),
  };
}
