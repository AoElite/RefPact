/** The diagram file: a diagram that a test keeps current. */
import { readFileSync } from "node:fs";

import { updateRequested, writeFile } from "./baseline.ts";
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
export function verifyDiagram(rule: string, file: string, draw: (format: DiagramFormat) => string | Failure): Failure | undefined {
  const named = diagramFormat(file);
  if ("message" in named) {
    return { type: "diagram", message: `refpact: ${named.message}` };
  }
  const text = draw(named.format);
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
  // A checkout can give the file the line ends of Windows, and a name in the diagram can
  // have them too.
  if (found?.replaceAll("\r\n", "\n") === text.replaceAll("\r\n", "\n")) {
    return undefined;
  }
  if (updateRequested("REFPACT_DIAGRAM")) {
    try {
      writeFile(file, text);
    } catch (error) {
      return { type: "diagram", message: `refpact: cannot write ${file}: ${(error as Error).message}` };
    }
    return undefined;
  }
  return { type: "stale-diagram", rule, file, message: renderStaleDiagram(rule, file) };
}
