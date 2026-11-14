import { query } from "../db"; //~ import src/db/client.ts#query written=src/db/index.ts#query
import { run } from "../db/index"; //~ import src/db/client.ts#query written=src/db/index.ts#run
import type { Row } from "../db"; //~ import-type src/db/client.ts#Row written=src/db/index.ts#Row

export const rows: Row[] = [...query("x"), ...run("y")];
