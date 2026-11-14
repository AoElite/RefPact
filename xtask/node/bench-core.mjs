// Times the core of the TypeScript package on a large synthetic fact set.
//
//   node xtask/node/bench-core.mjs [units]
//
// The fact set has `units` files (default 3000), each with 40 imports and 4 declarations.
// The script loads the core through the public entry of the package source, so it measures
// what a test of a user measures. Round 1 includes the transfer of the fact set into the core;
// the later rounds show the cost of the rules alone. Build the core first: `cargo xtask wasm`.
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
// REFPACT_BENCH_CORE names another entry of a core, to compare two builds.
const entry = pathToFileURL(process.env.REFPACT_BENCH_CORE ?? path.join(here, "../../packages/refpact/src/core/index.ts")).href;
const { evaluate } = await import(entry);

const unitCount = Number(process.argv[2] ?? 3000);
const layers = ["api", "app", "domain", "storage", "util"];
const units = [];
for (let index = 0; index < unitCount; index += 1) {
  units.push(["src", layers[index % layers.length], `m${index}.ts`]);
}
const facts = [];
const declarations = [];
let seed = 7;
const next = () => {
  seed = (seed * 1103515245 + 12345) % 2147483648;
  return seed;
};
units.forEach((unit, index) => {
  for (let count = 0; count < 40; count += 1) {
    const target = units[next() % unitCount];
    facts.push({
      unit,
      file: unit.join("/"),
      line: count + 1,
      column: 1,
      tags: count === 39 ? ["test"] : [],
      what: { type: "reference", target: [...target, `#f${count % 4}`], aliases: [], kind: "import", certainty: "exact" },
    });
  }
  for (let count = 0; count < 4; count += 1) {
    declarations.push({
      path: [...unit, `#f${count}`],
      name: `f${count}${index % 3 === 0 ? "Repository" : ""}`,
      unit,
      file: unit.join("/"),
      line: count + 1,
      column: 1,
      tags: [],
      kind: count % 2 === 0 ? "function" : "class",
      visibility: "exported",
      relations: count === 1 ? [{ relation: "extends", target: units[next() % unitCount].concat("#f1") }] : [],
    });
  }
});
const items = declarations.map((declaration) => declaration.path);
const factSet = {
  separator: "/",
  units,
  items,
  kinds: ["import"],
  markers: [],
  tags: ["test"],
  facts,
  declarations,
  itemKinds: ["function", "class"],
  visibilities: ["exported", "local"],
  relations: ["extends"],
};

const all = { type: "all" };
const rules = [
  { name: "domain does not import the api", subjects: { include: { type: "any-of", patterns: [["src", "domain", "**"]] } }, constraint: { type: "must-not-use", targets: { anyOf: [["src", "api", "**"]], unverified: [] } } },
  { name: "util imports only util", subjects: { include: { type: "any-of", patterns: [["src", "util", "**"]] } }, constraint: { type: "must-only-use", targets: { anyOf: [["src", "util", "**"]], unverified: [] } }, ignoreTags: ["test"] },
  { name: "layers are acyclic", subjects: { include: all }, constraint: { type: "must-be-acyclic", slices: [["src", "*"]] } },
  { name: "files are acyclic", subjects: { include: all }, constraint: { type: "must-be-acyclic" } },
  { name: "every file is used", subjects: { include: all }, constraint: { type: "must-be-used" } },
  { name: "the api does not reach storage", subjects: { include: { type: "any-of", patterns: [["src", "api", "**"]] } }, constraint: { type: "must-not-reach", targets: { anyOf: [["src", "storage", "**"]], unverified: [] } } },
  { name: "reachable from the api", subjects: { include: all }, constraint: { type: "must-be-reachable-from", roots: [["src", "api", "**"]] } },
  { name: "repositories live in storage", subjects: { include: all }, constraint: { type: "items", named: ["*Repository"], must: { type: "reside-in", places: [["src", "storage", "*"]] } } },
];

const time = (label, run) => {
  const start = process.hrtime.bigint();
  const result = run();
  const ms = Number(process.hrtime.bigint() - start) / 1e6;
  console.log(`${label.padEnd(40)} ${ms.toFixed(1).padStart(9)} ms`);
  return result;
};

console.log(`${unitCount} units, ${facts.length} facts, ${declarations.length} declarations`);
let total = 0;
for (let round = 0; round < 3; round += 1) {
  const start = process.hrtime.bigint();
  for (const rule of rules) {
    const result = round === 2 ? time(rule.name, () => evaluate(rule, factSet)) : evaluate(rule, factSet);
    if (!result.ok) {
      throw new Error(`${rule.name}: ${JSON.stringify(result.errors)}`);
    }
  }
  total = Number(process.hrtime.bigint() - start) / 1e6;
  if (round < 2) {
    console.log(`round ${round + 1}: ${total.toFixed(1)} ms`);
  }
}
console.log(`round 3: ${total.toFixed(1)} ms`);
