import * as client from "../db/client"; //~ import src/db/client.ts

export const rows = client.query("x"); //~ import src/db/client.ts#query
export type Rows = client.Row[]; //~ import src/db/client.ts#Row

// Not a use of the namespace: this `client` is a parameter.
export function local(client: { query(text: string): number }): number {
  return client.query("x");
}
