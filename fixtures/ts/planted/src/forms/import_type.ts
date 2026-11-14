import type { Client } from "../db/client"; //~ import-type src/db/client.ts#Client
import { type Row, query } from "../db/client"; //~ import-type src/db/client.ts#Row //~ import src/db/client.ts#query

export function first(client: Client): Row | undefined {
  return client.rows[0] ?? query("x")[0];
}
