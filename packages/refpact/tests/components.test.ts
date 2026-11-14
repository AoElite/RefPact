// `components`: the code of `.svelte`, `.vue` and `.astro` files.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { printPath } from "../src/core/index.ts";
import { scratchDirectory } from "./scratch.ts";

import { anyOf, loadProject, rule } from "../src/index.ts";
import type { Project } from "../src/index.ts";

const root = scratchDirectory("refpact-components-");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [], "lib": ["es2022", "dom"] }, "include": ["src"] }'],
  "src/api.ts": ["export const load = (): number => 1;", "export const save = (): number => 2;"],
  "src/main.ts": ['import App from "./App.svelte";', "export const app = App;"],
  "src/App.svelte": [
    '<script lang="ts" module>',
    '  import { load } from "./api";',
    "</script>",
    "<h1>import nothing from './api'</h1>",
    '<script lang="ts">',
    '  import Row from "./Row.vue";',
    "  const rows = [load(), fetch];",
    "</script>",
    "<Row {rows} on:click={() => localStorage.clear()} />",
  ],
  "src/Row.vue": [
    "<template><li>{{ row }}</li></template>",
    "<script setup>",
    'import { save } from "./api";',
    "defineProps(['row']);",
    "save();",
    "</script>",
    '<script type="application/json">{ "import": "./api" }</script>',
  ],
  "src/Page.astro": ["---", 'import App from "./App.svelte";', "const title = App;", "---", "<title>{title}</title>"],
  "src/state.svelte.ts": ['import { load } from "./api";', "export const state = load();"],
  "src/state.svelte": ["<p>not read: a file of this name with .ts is on the disk</p>"],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(root, file)), { recursive: true });
  writeFileSync(path.join(root, file), `${lines.join("\n")}\n`);
}

function references(project: Project, file: string): string[] {
  return project.facts.facts.flatMap((fact) =>
    fact.file === file && fact.what.type === "reference" ? [`${fact.line}:${fact.column} ${fact.what.kind} ${printPath(fact.what.target, project.facts)}`] : [],
  );
}

const app = loadProject(root, { components: true });

it("a component file is a file of the project, with the code of its scripts", () => {
  // `src/state.svelte` is not read: `src/state.svelte.ts` is a file on the disk.
  expect(app.files).toEqual(["src/App.svelte", "src/Page.astro", "src/Row.vue", "src/api.ts", "src/main.ts", "src/state.svelte.ts"]);
  // The places are those of the component, and the markup is not read.
  expect(references(app, "src/App.svelte")).toEqual(["2:12 import src/api.ts#load", "6:10 import src/Row.vue#default", "7:25 global global:fetch"]);
  // A script that holds data is no code.
  expect(references(app, "src/Row.vue")).toEqual(["3:10 import src/api.ts#save"]);
  // The part between the `---` lines of an Astro file.
  expect(references(app, "src/Page.astro")).toEqual(["2:8 import src/App.svelte#default"]);
  expect(references(app, "src/main.ts")).toEqual(["1:8 import src/App.svelte#default"]);
  expect(app.facts.unitLines).toEqual([9, 5, 7, 2, 2, 2]);
});

it("the rules see the component files", () => {
  const failure = rule("only the pages load data").files(anyOf("src/*.svelte", "src/*.vue")).mustNotImport(anyOf("src/api.ts")).verify(app);
  expect(failure?.message).toBe(
    "rule `only the pages load data` is violated in 2 place(s):\n" +
      "  src/App.svelte:2:12: [only the pages load data] src/App.svelte uses src/api.ts#load (import)\n" +
      "  src/Row.vue:3:10: [only the pages load data] src/Row.vue uses src/api.ts#save (import)",
  );
  rule("no cycles").files(anyOf("src/**")).mustBeAcyclic().check(app);
  const reach = rule("the entry does not reach the api").files(anyOf("src/main.ts")).mustNotReach(anyOf("src/api.ts")).verify(app);
  expect(reach?.message).toContain("src/App.svelte uses src/api.ts#load  at src/App.svelte:2:12 (import)");
});

it("a list names the extensions that are read, and the default reads none", () => {
  const svelte = loadProject(root, { components: [".svelte"] });
  expect(svelte.files).toEqual(["src/App.svelte", "src/api.ts", "src/main.ts", "src/state.svelte.ts"]);
  const plain = loadProject(root);
  expect(plain.files).toEqual(["src/api.ts", "src/main.ts", "src/state.svelte.ts"]);
  expect(references(plain, "src/main.ts")).toEqual(["1:8 import src/App.svelte#default"]);
});
