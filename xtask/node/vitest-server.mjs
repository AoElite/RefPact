// Runs Vitest test files again and again in one process, for `cargo xtask mutants`.
//
// Start it in the directory of the package. The first argument, if given, is JSON with more
// options for Vitest, such as `{"isolate": false, "setupFiles": [...]}`. Each line on stdin
// is a request in JSON: `{"files": [...], "changed": [...]}`. The files that changed since the
// last request are read again, and then the test files run. The answer is one line on stdout
// that begins with `@@result ` and holds `{"failed": [...], "byFile": {...}}`: the full names
// of the failed tests, and the name of each test file that failed without a failed test (an
// error while it loads); `byFile` gives the same for each test file, by its path in the
// package. Starting Vitest once and not for each request saves about half the time of a run.
import path from "node:path";
import { createInterface } from "node:readline";
import { pathToFileURL } from "node:url";

const root = process.cwd();
const { createVitest } = await import(pathToFileURL(path.join(root, "node_modules/vitest/dist/node.js")).href);
const options = process.argv[2] ? JSON.parse(process.argv[2]) : {};
const vitest = await createVitest("test", { watch: false, reporters: [{}], ...options });

for await (const line of createInterface({ input: process.stdin })) {
  const { files, changed = [] } = JSON.parse(line);
  for (const file of changed) {
    vitest.invalidateFile(path.resolve(root, file));
  }
  const failed = [];
  const byFile = {};
  try {
    const result = await vitest.runTestFiles(files.map((file) => path.resolve(root, file)), true);
    for (const module of result.testModules) {
      const own = (byFile[path.relative(root, module.moduleId)] = []);
      for (const test of module.children.allTests()) {
        if (test.result().state === "failed") {
          own.push(test.fullName.replaceAll(" > ", " "));
        }
      }
      if (!module.ok() && own.length === 0) {
        own.push(path.basename(module.moduleId));
      }
      failed.push(...own);
    }
    if (result.unhandledErrors.length > 0 && failed.length === 0) {
      failed.push("unhandled error");
    }
  } catch (error) {
    failed.push(`vitest: ${error}`);
  }
  process.stdout.write(`@@result ${JSON.stringify({ failed, byFile })}\n`);
}
await vitest.close();
