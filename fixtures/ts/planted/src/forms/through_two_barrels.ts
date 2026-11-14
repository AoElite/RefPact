import { query } from "../barrel/outer"; //~ import src/db/client.ts#query written=src/barrel/outer.ts#query
import * as outer from "../barrel/outer";

export const rows = [...query("x"), ...outer.query("y")]; //~ import src/db/client.ts#query written=src/barrel/outer.ts#query
