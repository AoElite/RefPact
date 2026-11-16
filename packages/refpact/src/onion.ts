/**
 * The onion architecture, also called the hexagonal architecture or ports and adapters:
 * layers with the domain at the centre and the adapters outside.
 */
import { addFilters } from "./filters.ts";
import type { Filter, Filters } from "./filters.ts";
import { RuleSet } from "./group.ts";
import { layers, Layers } from "./layers.ts";
import type { DiagramFormat } from "./core/index.ts";
import type { Project } from "./frontend/project.ts";
import type { AnyOf, Failure, Files, ImportRule } from "./rules.ts";

interface Parts {
  readonly name: string;
  readonly domainModels: readonly string[] | undefined;
  readonly domainServices: readonly string[] | undefined;
  readonly applicationServices: readonly string[] | undefined;
  readonly adapters: readonly { readonly name: string; readonly files: readonly string[] }[];
  /** The filters of every rule of the architecture. */
  readonly filters: readonly Filter[];
  readonly covered: AnyOf<string> | Files | undefined;
}

/**
 * The parts of an onion architecture. From the inside out:
 *
 * - the **domain models**, which import no other part;
 * - the **domain services**, which may import the domain models;
 * - the **application services**, which may import the domain;
 * - the **adapters**, such as a web interface or a database, which may import the
 *   application services and the domain, but not each other.
 *
 * Give the parts that the code has; at least two of them. The rules are those of
 * {@link layers}, one layer for each part, and each adapter a layer of its own.
 *
 * ```ts
 * onionArchitecture("the shop")
 *   .domainModels("src/domain/model/**")
 *   .domainServices("src/domain/service/**")
 *   .applicationServices("src/application/**")
 *   .adapter("web", "src/adapter/web/**")
 *   .adapter("database", "src/adapter/db/**")
 *   .check(app);
 * ```
 *
 * The filters of a rule ({@link Filters}), such as `ignoringTypeImports()`, apply to every
 * part.
 */
export interface Onion extends Filters<Onion> {}

export class Onion extends RuleSet {
  private readonly parts: Parts;

  /** @internal */
  constructor(parts: Parts) {
    super();
    this.parts = parts;
  }

  /** The name of the architecture. */
  get name(): string {
    return this.parts.name;
  }

  /** The files of the domain models: the centre, which imports no other part. */
  domainModels(...files: string[]): Onion {
    return this.with({ domainModels: files });
  }

  /** The files of the domain services, which may import the domain models. */
  domainServices(...files: string[]): Onion {
    return this.with({ domainServices: files });
  }

  /** The files of the application services, which may import the domain. */
  applicationServices(...files: string[]): Onion {
    return this.with({ applicationServices: files });
  }

  /**
   * Adds an adapter. It may import the application services and the domain, and no other
   * adapter may import it. Its layer is named `adapter <name>`.
   */
  adapter(name: string, ...files: string[]): Onion {
    return this.with({ adapters: [...this.parts.adapters, { name, files }] });
  }

  /** @internal The architecture with one more filter. */
  filtered(filter: Filter): Onion {
    return this.with({ filters: [...this.parts.filters, filter] });
  }

  /** Each of these files must be in a part. See {@link Layers.everyFileInALayer}. */
  everyFileInALayer(files: AnyOf<string> | Files): Onion {
    return this.with({ covered: files });
  }

  /** The parts as layers, from the outside in. */
  asLayers(): Layers {
    const parts = this.parts;
    const adapter = (name: string): string => `adapter ${name}`;
    let made = layers(parts.name);
    for (const { name, files } of parts.adapters) {
      made = made.layer(adapter(name), ...files);
    }
    const inner = (
      [
        ["application services", parts.applicationServices],
        ["domain services", parts.domainServices],
        ["domain models", parts.domainModels],
      ] as const
    ).flatMap(([name, files]) => (files !== undefined ? [{ name, files }] : []));
    for (const { name, files } of inner) {
      made = made.layer(name, ...files);
    }
    // An adapter is above the adapters that follow it, but may not import them.
    for (const { name } of parts.adapters) {
      made = made.mayOnlyImport(adapter(name), ...inner.map((part) => part.name));
    }
    made = parts.filters.reduce((layers, filter) => layers.filtered(filter), made);
    if (parts.covered !== undefined) {
      made = made.everyFileInALayer(parts.covered);
    }
    return made;
  }

  /** A diagram of the parts as layers. See {@link Layers.diagram}. */
  diagram(project: Project, format: DiagramFormat = "mermaid"): string {
    return this.asLayers().diagram(project, format);
  }

  /** Like {@link Onion.checkDiagram}, but returns the failure, or `undefined` when the file is current. */
  verifyDiagram(project: Project, file: string): Failure | undefined {
    return this.asLayers().verifyDiagram(project, file);
  }

  /**
   * Throws an {@link ArchitectureError} when the file does not hold the
   * {@link Onion.diagram | diagram} of the parts. See {@link ImportRule.checkDiagram}.
   */
  checkDiagram(project: Project, file: string): void {
    this.asLayers().checkDiagram(project, file);
  }

  /** The rules of the architecture. See {@link Layers.rules}. */
  rules(): ImportRule[] {
    return this.asLayers().rules();
  }

  private with(change: Partial<Parts>): Onion {
    return new Onion({ ...this.parts, ...change });
  }
}

/** Starts an onion architecture. See {@link Onion}. */
export function onionArchitecture(name: string): Onion {
  return new Onion({
    name,
    domainModels: undefined,
    domainServices: undefined,
    applicationServices: undefined,
    adapters: [],
    filters: [],
    covered: undefined,
  });
}

addFilters(Onion);
