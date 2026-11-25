import { defineConfig } from "vitest/config";

export default defineConfig({
  test: {
    // A test may load several projects through the compiler. On a busy machine that takes
    // longer than the 5 seconds that Vitest gives a test by default.
    testTimeout: 60_000,
    hookTimeout: 60_000,
    teardownTimeout: 30_000,
    // The test files of a worker share the modules that they import: the compiler, the core,
    // and what a load keeps of the installed packages. A few workers that share them are
    // faster than one new worker for each file and each core.
    isolate: false,
    maxWorkers: 4,
  },
});
