const client = require("../db/client"); //~ require src/db/client.ts

export const rows: unknown = client.query("x"); //~ require src/db/client.ts#query

// Not a use: this `require` is a parameter, not the function of Node.
export function local(require: (name: string) => unknown): unknown {
  return require("node:fs");
}
