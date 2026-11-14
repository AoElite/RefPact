import { readFileSync } from "node:fs"; //~ import node:fs#readFileSync
import cp from "child_process"; //~ import node:child_process#default
import { readFile } from "fs/promises"; //~ import node:fs/promises#readFile

export const all = [readFileSync, cp.spawn, readFile];
