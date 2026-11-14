/**
 * How the TypeScript front end writes paths for the core.
 *
 * - A file is its path from the root, one segment for each directory and the file name:
 *   `src/db/client.ts` is `["src", "db", "client.ts"]`.
 * - An export is one more segment that starts with `#`: `["src", "db", "client.ts", "#query"]`.
 * - A package is one segment `npm:name`, also when the name has a scope: `["npm:@scope/name"]`.
 *   A subpath adds segments: `lodash/fp` is `["npm:lodash", "fp"]`.
 * - A module of Node is one segment `node:name`: `["node:fs"]`, `["node:fs", "promises"]`.
 * - A global of the host is one segment `global:name`, and a member of it one more segment
 *   that starts with `#`: `["global:process", "#env"]`.
 */
import type { Path } from "../core/index.ts";

export function exportSegment(name: string): string {
  return `#${name}`;
}

export function packageSegment(name: string): string {
  return `npm:${name}`;
}

export function globalSegment(name: string): string {
  return `global:${name}`;
}

export function nodeSegment(name: string): string {
  return `node:${name}`;
}

/**
 * How reports print these paths, the way an import names them: `src/db/client.ts#query`,
 * `lodash/fp#map`. An export joins without a separator, and a package loses `npm:`.
 */
export const ATTACHED_PREFIXES: readonly string[] = ["#"];
export const HIDDEN_PREFIXES: readonly string[] = ["npm:"];
