// No violation in this file. The names look like forbidden imports and are not.
import { join } from "node:path";

const fs = { readFileSync: (path: string) => path };
const client = { query: (text: string) => text };

export const text = fs.readFileSync(join("a", "b")) + client.query("node:fs") + "fakepkg";

export function load(importer: (name: string) => unknown): unknown {
  return importer("../db/client");
}
