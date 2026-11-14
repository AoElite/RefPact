// Finds the `{ }` block of statements around each mutation, for the TypeScript schemata of
// `cargo xtask mutants` (xtask/src/mutants).
//
// Usage: node xtask/node/ts-blocks.mjs <request.json>. The request maps a file to the spans of
// its mutations, as [start, end] offsets. The answer on stdout maps the file to one entry for
// each span: [open, close], the offsets of the `{` and `}` of the innermost block of
// statements that holds the whole span, or null when no block holds it (code at the top of
// the module, in a class field, in an object literal outside a function).
//
// It parses with the TypeScript 6 of packages/api-docs, which has the JavaScript API. Only
// the shape of the syntax matters here, not the types.
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const ts = createRequire(path.join(here, "../../packages/api-docs/package.json"))("typescript");

const request = JSON.parse(readFileSync(process.argv[2], "utf8"));
const answer = {};
for (const [file, spans] of Object.entries(request)) {
  const text = readFileSync(file, "utf8");
  const source = ts.createSourceFile(file, text, ts.ScriptTarget.Latest, true);
  answer[file] = spans.map(([start, end]) => {
    let found = null;
    const visit = (node) => {
      if (node.getStart(source) > start || node.end < end) {
        return;
      }
      if (ts.isBlock(node)) {
        found = [node.getStart(source), node.end - 1];
      }
      ts.forEachChild(node, visit);
    };
    visit(source);
    return found;
  });
}
process.stdout.write(JSON.stringify(answer));
