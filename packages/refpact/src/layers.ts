/** Layers: rules about which layers of an architecture may import which. */
import { addFilters } from "./filters.ts";
import type { Filter, Filters } from "./filters.ts";
import { RuleSet } from "./group.ts";
import type { DiagramFormat } from "./core/index.ts";
import { verifyDiagram } from "./diagram.ts";
import type { Project } from "./frontend/project.ts";
import { filePattern } from "./patterns.ts";
import { all, AnyOf, anyOf, ArchitectureError, Files, orFailure, passes, rule } from "./rules.ts";
import type { Failure, ImportRule } from "./rules.ts";

interface Settings {
  readonly name: string;
  readonly list: readonly { readonly name: string; readonly files: readonly string[] }[];
  /** The filters of every rule of the layers. */
  readonly filters: readonly Filter[];
  readonly strict: boolean;
  /** For a layer, the only layers it may import. */
  readonly mayOnlyImport: readonly (readonly [string, readonly string[]])[];
  /** For a layer, the only layers that may import it. */
  readonly mayOnlyBeImportedBy: readonly (readonly [string, readonly string[]])[];
  /** The files that must each be in a layer. */
  readonly covered: AnyOf<string> | Files | undefined;
}

/**
 * Layers from the top to the bottom. A layer may import the layers below it. It must not
 * import a layer above it.
 *
 * ```ts
 * layers("the layers of the shop")
 *   .layer("ui", "src/ui/**")
 *   .layer("domain", "src/domain/**")
 *   .layer("db", "src/db/**")
 *   .mayOnlyBeImportedBy("db", "domain")
 *   .everyFileInALayer(allExcept("src/main.ts"))
 *   .check(app);
 * ```
 *
 * The layers have the filters of a rule ({@link Filters}), such as `ignoringTypeImports()`,
 * `ignoringImport(..)` and `asWarning()`. A filter applies to every rule of the layers and to
 * their diagram.
 */
export interface Layers extends Filters<Layers> {}

export class Layers extends RuleSet {
  private readonly settings: Settings;

  /** @internal */
  constructor(settings: Settings) {
    super();
    this.settings = settings;
  }

  /** The name of the layers. */
  get name(): string {
    return this.settings.name;
  }

  /** Adds a layer below the layers that are there. */
  layer(name: string, ...files: string[]): Layers {
    return this.with({ list: [...this.settings.list, { name, files }] });
  }

  /** @internal The layers with one more filter. */
  filtered(filter: Filter): Layers {
    return this.with({ filters: [...this.settings.filters, filter] });
  }

  /** Makes the layers strict: a layer may import only the layer right below it, not the layers further down. */
  strict(): Layers {
    return this.with({ strict: true });
  }

  /**
   * The layer may import only these layers. Each of them must be below it, and right below
   * it when the layers are {@link Layers.strict | strict}. No layers forbids the layer to
   * import any other layer.
   */
  mayOnlyImport(layer: string, ...layers: string[]): Layers {
    return this.with({ mayOnlyImport: [...this.settings.mayOnlyImport, [layer, layers]] });
  }

  /**
   * Only these layers may import the layer. Each of them must be above it, and right above
   * it when the layers are {@link Layers.strict | strict}. No layers forbids every other layer
   * to import it.
   *
   * This is about the layers only. To keep every other file away as well, add a rule with
   * {@link ScopedRule.mustOnlyBeImportedBy}.
   */
  mayOnlyBeImportedBy(layer: string, ...layers: string[]): Layers {
    return this.with({ mayOnlyBeImportedBy: [...this.settings.mayOnlyBeImportedBy, [layer, layers]] });
  }

  /**
   * Each of these files must be in a layer. A new file then cannot stay out of the
   * architecture unseen. Leave out the files that belong to no layer, such as the entry
   * point: `allExcept("src/main.ts")`.
   */
  everyFileInALayer(files: AnyOf<string> | Files): Layers {
    return this.with({ covered: files });
  }

  /**
   * The rules of the layers: one `mustNotImport` rule for each layer that must not import
   * some other layers, and the rule of {@link Layers.everyFileInALayer} when it is given.
   *
   * The rule of a layer has the name of the layers and the name of the layer. It is
   * `<layers>: <layer> does not import the layers above it` when only the order of the
   * layers applies to the layer, and `<layers>: <layer> imports only the layers it may
   * import` when more applies.
   *
   * Throws an {@link ArchitectureError} of type `malformed` when the layers cannot fail as
   * written or name a layer that does not exist.
   */
  rules(): ImportRule[] {
    const settings = this.settings;
    const allowed = this.allowed();
    if (!Array.isArray(allowed)) {
      throw new ArchitectureError(allowed);
    }
    const made: ImportRule[] = [];
    settings.list.forEach((layer, from) => {
      const forbidden = forbiddenFor(allowed, from);
      if (forbidden.length === 0) {
        return;
      }
      const onlyAbove = forbidden.length === from && forbidden.every((to, index) => to === index);
      const name = onlyAbove
        ? `${settings.name}: ${layer.name} does not import the layers above it`
        : `${settings.name}: ${layer.name} imports only the layers it may import`;
      const targets = forbidden.flatMap((to) => settings.list[to]?.files ?? []);
      const one = rule(name)
        .files(anyOf(...layer.files))
        .mustNotImport(anyOf(...targets));
      made.push(this.withFilters(one, true));
    });
    if (settings.covered !== undefined) {
      const places = settings.list.flatMap((layer) => layer.files);
      const covered = rule(`${settings.name}: every file is in a layer`).files(settings.covered).mustBeIn(places);
      made.push(this.withFilters(covered, false));
    }
    return made;
  }

  /** The rule with the filters of the layers. A rule that looks at no import gets only the filters that are not about imports. */
  private withFilters(made: ImportRule, seesImports: boolean): ImportRule {
    const filters = this.settings.filters.filter((filter) => seesImports || !filter.aboutImports);
    return filters.reduce((one, filter) => one.filtered(filter), made);
  }

  /**
   * A diagram of the layers, in Mermaid (the default), DOT or Markdown, from the top to the bottom. Each
   * layer is a node, and an edge is an import between two layers. An import that the layers
   * forbid is red. A file is in the first layer that matches it, and a file in no layer is
   * not drawn. The files of {@link Layers.everyFileInALayer} are not checked.
   *
   * Throws an {@link ArchitectureError}: `malformed` as {@link Layers.rules} does, or
   * `invalid` when a pattern of a layer matches no file.
   *
   * ```ts
   * const text = layers("the layers").layer("ui", "src/ui/**").layer("db", "src/db/**").diagram(app);
   * ```
   */
  diagram(project: Project, format: DiagramFormat = "mermaid"): string {
    const settings = this.settings;
    const allowed = this.allowed();
    if (!Array.isArray(allowed)) {
      throw new ArchitectureError(allowed);
    }
    const list = settings.list.map((layer, from) => ({
      name: layer.name,
      units: layer.files.map(filePattern),
      mustNotUse: forbiddenFor(allowed, from),
    }));
    const drawn = rule(settings.name).files(all()).mustBeImported();
    return this.withFilters(drawn, true).draw(project, { layers: list }, format);
  }

  /** Like {@link Layers.checkDiagram}, but returns the failure, or `undefined` when the file is current. */
  verifyDiagram(project: Project, file: string): Failure | undefined {
    return verifyDiagram(this.settings.name, file, (format) => orFailure(() => this.diagram(project, format)));
  }

  /**
   * Throws an {@link ArchitectureError} when the file does not hold the
   * {@link Layers.diagram | diagram} of the layers. See {@link ImportRule.checkDiagram}.
   */
  checkDiagram(project: Project, file: string): void {
    passes(this.verifyDiagram(project, file));
  }

  /** For each layer, by position from the top, the layers it may import; or every problem of the layers. */
  private allowed(): boolean[][] | Failure {
    const { name, list, strict } = this.settings;
    const malformed = (problems: readonly string[]): Failure => ({
      type: "malformed",
      rule: name,
      message: `rule \`${name}\` cannot be checked:\n  ${problems.join("\n  ")}`,
    });
    const count = list.length;
    if (count < 2) {
      return malformed(["fewer than two layers are given, so no layer can import a layer above it"]);
    }
    const problems: string[] = [];
    list.forEach((layer, position) => {
      if (list.slice(0, position).some((other) => other.name === layer.name)) {
        problems.push(`two layers have the name \`${layer.name}\``);
      }
    });
    // What the order allows: the layers below, or only the next one when strict.
    const ordered = (from: number, to: number): boolean => to > from && (!strict || to === from + 1);
    const allowed = list.map((_, from) => list.map((__, to) => ordered(from, to)));
    const position = (layer: string): number | undefined => {
      const found = list.findIndex((entry) => entry.name === layer);
      if (found === -1) {
        problems.push(`\`${layer}\` is not a layer`);
        return undefined;
      }
      return found;
    };
    const restrict = (
      lists: Settings["mayOnlyImport"],
      twice: (layer: string) => string,
      forbidden: (layer: string, other: string) => string,
      edge: (layer: number, other: number) => [number, number],
    ): void => {
      const seen = new Set<number>();
      for (const [layer, others] of lists) {
        const at = position(layer);
        if (at === undefined) {
          continue;
        }
        if (seen.has(at)) {
          problems.push(twice(layer));
        }
        seen.add(at);
        const keep = list.map(() => false);
        for (const other of others) {
          const otherAt = position(other);
          if (otherAt === undefined) {
            continue;
          }
          const [from, to] = edge(at, otherAt);
          if (ordered(from, to)) {
            keep[otherAt] = true;
          } else {
            problems.push(forbidden(layer, other));
          }
        }
        keep.forEach((kept, otherAt) => {
          const [from, to] = edge(at, otherAt);
          const row = allowed[from] as boolean[];
          row[to] = row[to] === true && kept;
        });
      }
    };
    restrict(
      this.settings.mayOnlyImport,
      (layer) => `\`${layer}\` has two lists of the layers it may import`,
      (layer, other) => `\`${layer}\` may only import \`${other}\`, which the order of the layers forbids`,
      (layer, other) => [layer, other],
    );
    restrict(
      this.settings.mayOnlyBeImportedBy,
      (layer) => `\`${layer}\` has two lists of the layers that may import it`,
      (layer, other) => `\`${layer}\` may only be imported by \`${other}\`, which the order of the layers forbids`,
      (layer, other) => [other, layer],
    );
    return problems.length === 0 ? allowed : malformed(problems);
  }

  private with(change: Partial<Settings>): Layers {
    return new Layers({ ...this.settings, ...change });
  }
}

/** The layers that a layer must not import, by position from the top, from what each layer may import. */
function forbiddenFor(allowed: readonly (readonly boolean[])[], from: number): number[] {
  return allowed.map((_, to) => to).filter((to) => to !== from && allowed[from]?.[to] !== true);
}

/** Starts a set of layers. Add the layers from the top to the bottom. */
export function layers(name: string): Layers {
  return new Layers({
    name,
    list: [],
    filters: [],
    strict: false,
    mayOnlyImport: [],
    mayOnlyBeImportedBy: [],
    covered: undefined,
  });
}

addFilters(Layers);
