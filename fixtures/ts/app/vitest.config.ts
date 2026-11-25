import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    include: ["architecture.test.ts"],
    // The test loads the project through the compiler, which a busy machine does slowly.
    testTimeout: 60_000,
    hookTimeout: 60_000,
    teardownTimeout: 30_000,
  },
});
