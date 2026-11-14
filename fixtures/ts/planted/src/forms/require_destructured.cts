const { query, Client: Database } = require("../db/client"); //~ require src/db/client.ts#query //~ require src/db/client.ts#Client

export const rows: unknown = [query("x"), new Database()];
