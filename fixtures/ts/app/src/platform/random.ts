import { randomBytes } from "node:crypto";

export function token(): string {
  return randomBytes(8).toString("hex");
}
