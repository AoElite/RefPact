// Not a violation of the randomness rule: `createHash` is not a source of randomness. The
// rule names `randomBytes`, `randomUUID` and `getRandomValues`, not the whole of
// `node:crypto`.
import { createHash } from "node:crypto";

import { query } from "../db/index.js";
import type { Row } from "../db/index.js";
import { readConfig } from "../platform/files.js";
import { token } from "../platform/random.js";

export function loadRows(configPath: string): Row[] {
  const digest = createHash("sha256").update(readConfig(configPath)).digest("hex");
  return query(`${digest}${token()}`);
}
