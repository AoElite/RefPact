import { readFileSync } from "node:fs";
import { thing } from "fakepkg";
import { query } from "../db";
import { beta } from "./beta";
import type { Gamma } from "./gamma";

export const strict = (gamma: Gamma): unknown[] => [readFileSync, thing, query, beta, gamma];
