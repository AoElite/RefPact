// The same rules under the test runner of Node: `node --test architecture.node.test.ts`.
import { test } from "node:test";

import { app, rules } from "./architecture.rules.ts";

for (const rule of rules) {
  test(rule.toCore().name, () => rule.check(app));
}
