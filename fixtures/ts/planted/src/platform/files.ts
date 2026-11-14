// Not a subject of the rules: the platform layer may use node:fs.
import { existsSync } from "node:fs";

export function exists(path: string): boolean {
  return existsSync(path);
}
