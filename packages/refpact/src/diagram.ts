/** The diagram file: a diagram that a test keeps current. */
import { readFileSync } from "node:fs";
import path from "node:path";

import { writeFile } from "./baseline.ts";
import { diagramFormat, renderStaleDiagram } from "./core/index.ts";
import type { DiagramFormat } from "./core/index.ts";
import type { Failure } from "./rules.ts";

/**
 * Nothing when the file holds the diagram of the rule that `draw` gives in the format of the
 * file. When the environment variable `REFPACT_DIAGRAM` is `update`, this writes the diagram
 * to a file that does not hold it. A relative path is relative to the working directory.
 *
 * @internal
 */
export function verifyDiagram(rule: string, name: string, draw: (format: DiagramFormat) => string | Failure): Failure | undefined {
  const file = path.resolve(name);
  const format = diagramFormat(file);
  if (format === undefined) {
    return { type: "diagram", message: `refpact: the name of the diagram file ${file} must end in .md, .mmd, .mermaid, .dot or .gv` };
  }
  const text = draw(format);
  if (typeof text !== "string") {
    return text;
  }
  let found: string | undefined;
  try {
    found = readFileSync(file, "utf8");
  } catch (error) {
    if ((error as NodeJS.ErrnoException).code !== "ENOENT") {
      return { type: "diagram", message: `refpact: cannot read ${file}: ${(error as Error).message}` };
    }
  }
  // A checkout can give the file the line ends of Windows.
  if (found?.replaceAll("\r\n", "\n") === text) {
    return undefined;
  }
  if (process.env["REFPACT_DIAGRAM"] === "update") {
    try {
      writeFile(file, text);
    } catch (error) {
      return { type: "diagram", message: `refpact: cannot write ${file}: ${(error as Error).message}` };
    }
    return undefined;
  }
  return { type: "stale-diagram", rule, file, message: renderStaleDiagram(rule, file) };
}
