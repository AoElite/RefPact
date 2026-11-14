// Diagrams of what a rule sees, in Mermaid and DOT.
import { mkdirSync, writeFileSync } from "node:fs";
import path from "node:path";
import { expect, it } from "vitest";
import { scratchDirectory } from "./scratch.ts";

import { all, anyOf, ArchitectureError, layers, loadProject, onionArchitecture, rule } from "../src/index.ts";

const directory = scratchDirectory("refpact-diagrams-");
const files: Record<string, string[]> = {
  "tsconfig.json": ['{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }'],
  "src/features/cart/index.ts": ['import { price } from "../prices/index";', 'import { log } from "../../shared/log";', "export const cart = price + log;"],
  "src/features/prices/index.ts": ['import { cart } from "../cart/index";', "export const price = 1;", "export const both = cart;"],
  "src/shared/log.ts": ["export const log = 1;"],
};
for (const [file, lines] of Object.entries(files)) {
  mkdirSync(path.dirname(path.join(directory, file)), { recursive: true });
  writeFileSync(path.join(directory, file), `${lines.join("\n")}\n`);
}
const app = loadProject(directory);

it("a Mermaid diagram of the files, with the cycle in red", () => {
  expect(rule("the files").files(all()).mustBeAcyclic().diagram(app)).toBe(
    [
      "---",
      'title: "the files"',
      "---",
      "flowchart LR",
      '    n0["src/features/cart/index.ts"]',
      '    n1["src/features/prices/index.ts"]',
      '    n2["src/shared/log.ts"]',
      "    n0 --> n1",
      "    n0 --> n2",
      "    n1 --> n0",
      "    classDef cycle stroke:#d62728,stroke-width:2px",
      "    class n0,n1 cycle",
      "    linkStyle 0,2 stroke:#d62728,stroke-width:2px",
      "",
    ].join("\n"),
  );
});

it("a DOT diagram of the slices", () => {
  const slices = rule("the parts").files(all()).mustBeIndependentBetween(anyOf("src/features", "src/shared"));
  expect(slices.diagram(app, "dot")).toBe(
    [
      "digraph {",
      '    label="the parts";',
      "    rankdir=LR;",
      "    node [shape=box];",
      '    n0 [label="src/features"];',
      '    n1 [label="src/shared"];',
      "    n0 -> n1;",
      "}",
      "",
    ].join("\n"),
  );
});

it("a diagram sees what the rule sees", () => {
  const exempt = rule("r")
    .files(anyOf("src/features/**"))
    .mustNotImport(anyOf("node:fs"))
    .ignoringImport(anyOf("src/features/prices/index.ts"), anyOf("src/features/cart/index.ts"))
    .diagram(app);
  expect(exempt).toContain("    n0 --> n1\n");
  expect(exempt).not.toContain("cycle");
});

it("a diagram of a rule that cannot fail throws", () => {
  expect(() => rule("r").files(anyOf("src/web/**")).mustBeAcyclic().diagram(app)).toThrow(ArchitectureError);
});

const shop = layers("the layers")
  .layer("prices", "src/features/prices/**")
  .layer("cart", "src/features/cart/**")
  .layer("shared", "src/shared/**");

it("a diagram of layers has a node for each layer, and the imports they forbid are red", () => {
  expect(shop.diagram(app)).toBe(
    [
      "---",
      'title: "the layers"',
      "---",
      "flowchart TB",
      '    n0["prices"]',
      '    n1["cart"]',
      '    n2["shared"]',
      "    n0 --> n1",
      "    n1 --> n0",
      "    n1 --> n2",
      "    linkStyle 1 stroke:#d62728,stroke-width:2px",
      "",
    ].join("\n"),
  );
});

it("a diagram of strict layers in DOT", () => {
  expect(shop.strict().diagram(app, "dot")).toContain('    n1 -> n0 [color="#d62728"];\n    n1 -> n2;\n');
  const skipping = layers("the layers").layer("cart", "src/features/cart/**").layer("prices", "src/features/prices/**").layer("shared", "src/shared/**").strict();
  expect(skipping.diagram(app, "dot")).toContain('    n0 -> n2 [color="#d62728"];\n');
});

it("a diagram of an onion is that of its layers", () => {
  const onion = onionArchitecture("the onion").domainModels("src/shared/**").adapter("cart", "src/features/cart/**").adapter("prices", "src/features/prices/**");
  expect(onion.diagram(app)).toBe(onion.asLayers().diagram(app));
  expect(onion.diagram(app)).toContain("    linkStyle 0,2 stroke:#d62728,stroke-width:2px\n");
});

it("a diagram of layers leaves out type imports when they do", () => {
  const typed = scratchDirectory("refpact-layer-diagram-");
  const sources: Record<string, string> = {
    "tsconfig.json": '{ "compilerOptions": { "module": "preserve", "noEmit": true, "types": [] }, "include": ["src"] }',
    "src/ui/view.ts": "export type View = number;\n",
    "src/db/row.ts": 'import type { View } from "../ui/view";\nexport const row: View = 1;\n',
  };
  for (const [file, text] of Object.entries(sources)) {
    mkdirSync(path.dirname(path.join(typed, file)), { recursive: true });
    writeFileSync(path.join(typed, file), text);
  }
  const project = loadProject(typed);
  const made = layers("the layers").layer("ui", "src/ui/**").layer("db", "src/db/**");
  expect(made.diagram(project)).toContain("    n1 --> n0\n");
  expect(made.ignoringTypeImports().diagram(project)).not.toContain("-->");
});

it("a layer that matches no file throws an invalid failure", () => {
  try {
    layers("the layers").layer("ui", "src/features/**").layer("web", "src/web/**").diagram(app);
    expect.unreachable();
  } catch (error) {
    expect(error).toBeInstanceOf(ArchitectureError);
    const { failure: thrown } = error as ArchitectureError;
    expect(thrown.type).toBe("invalid");
    expect(thrown.message).toBe("rule `the layers` cannot be checked:\n  layer pattern `src/web/**` matches nothing in the analysed code");
  }
});

it("malformed layers have no diagram", () => {
  expect(() => layers("the layers").layer("ui", "src/features/**").diagram(app)).toThrow(/fewer than two layers/);
});
