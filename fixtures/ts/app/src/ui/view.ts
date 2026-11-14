import { loadRows } from "../app/service.js";
// Not a violation of the first rule: a type-only import. The rule says `ignoringTypeImports`.
import type { Row } from "../db/index.js";

// Not a violation: `fs` is a local name here, not the module of Node.
const fs = { readFileSync: (row: Row): string => row.name };

export function render(configPath: string): string {
  return loadRows(configPath).map(fs.readFileSync).join("\n");
}
