// The rules under Vitest.
import { test } from "vitest";

import { app, rules } from "./architecture.rules.ts";

for (const rule of rules) {
  test(rule.toCore().name, () => rule.check(app));
}
