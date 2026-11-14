/** Checks of the user: what a rule sees, and how its violations are made (SPEC 4.15). */
import type { Declaration, Detail, Fact, FactSet, Path, Reference, SelectResult, Step, Violation } from "./core/index.ts";

/** An import that the rule sees: the filters and the exemptions of the rule let it pass. */
export interface SeenImport {
  /** The fact of the import. */
  readonly fact: Fact;
  /** The reference of the fact. */
  readonly reference: Reference;
  /** The file that imports. */
  readonly file: Path;
  /** Whether the rule selects the file that imports. */
  readonly subject: boolean;
  /** The paths of the import, `[target, ...aliases]`. */
  readonly paths: readonly Path[];
  /** For each path, the file that has it, or `undefined` for a package or a module of Node. */
  readonly owners: readonly (Path | undefined)[];
  /**
   * For each list of targets that the check names: the first path of the import that the
   * list matches. A list that matches no path of the import has no entry.
   */
  readonly hits: Readonly<Record<string, Path>>;
}

/** A marker of a selected file that the rule sees. */
export interface SeenMarker {
  /** The fact of the marker. */
  readonly fact: Fact;
  /** The file. */
  readonly file: Path;
  /** The marker. */
  readonly marker: string;
}

/** An edge of the graph of files, with the first import that makes it. */
export interface ImportEdge {
  /** The file that the edge leads to. */
  readonly to: Path;
  /** The fact of the import. */
  readonly fact: Fact;
  /** The path of the import that leads to the file. */
  readonly target: Path;
}

interface NumberedEdge extends ImportEdge {
  /** The number of the file that the edge leads to. */
  readonly node: number;
}

/** The names of the lists of patterns of a check, in the order of the lists in the request. @internal */
export interface ListNames {
  readonly targets: readonly string[];
  readonly files: readonly string[];
}

type Selection = Extract<SelectResult, { ok: true }>;

/**
 * What a check of the user gets: the files that the rule selects and what the rule sees. The
 * rule has applied its filters and its exemptions.
 */
export class Scope {
  /** The name of the rule. */
  readonly ruleName: string;
  /** The facts of the project. */
  readonly facts: FactSet;
  /** The files that the rule selects, in order. */
  readonly subjects: readonly Path[];
  /** The imports that the rule sees, of all files, in the order of the facts. */
  readonly imports: readonly SeenImport[];
  /** The markers of the selected files that the rule sees. */
  readonly markers: readonly SeenMarker[];
  /** The declarations of the selected files that the rule sees. */
  readonly declarations: readonly Declaration[];
  /** @internal */
  protected readonly selection: Selection;
  private readonly selected: ReadonlySet<string>;
  private readonly lists: ReadonlyMap<string, readonly Path[]>;
  private firstFiles: Map<string, string> | undefined;
  private made: Graph | undefined;

  /** @internal */
  constructor(ruleName: string, facts: FactSet, selection: Selection, names: ListNames) {
    this.ruleName = ruleName;
    this.facts = facts;
    this.selection = selection;
    const units = facts.units;
    this.subjects = selection.subjects.map((unit) => units[unit] as Path);
    this.selected = new Set(this.subjects.map(key));
    this.lists = new Map(names.files.map((name, list) => [name, (selection.units[list] ?? []).map((unit) => units[unit] as Path)]));
    const subjectSet = new Set(selection.subjects);
    this.imports = selection.references.map((seen) => {
      const fact = facts.facts[seen.fact] as Fact;
      const reference = fact.what as Reference;
      const paths = [reference.target, ...reference.aliases];
      const hits: Record<string, Path> = {};
      seen.hits?.forEach((hit, list) => {
        if (hit !== null) {
          hits[names.targets[list] as string] = paths[hit] as Path;
        }
      });
      return {
        fact,
        reference,
        file: units[seen.unit] as Path,
        subject: subjectSet.has(seen.unit),
        paths,
        owners: seen.owners.map((owner) => (owner === null ? undefined : (units[owner] as Path))),
        hits,
      };
    });
    this.markers = selection.markers.map((position) => {
      const fact = facts.facts[position] as Fact;
      return { fact, file: fact.unit, marker: (fact.what as { marker: string }).marker };
    });
    const declarations = facts.declarations ?? [];
    this.declarations = selection.declarations.map((position) => declarations[position] as Declaration);
  }

  /** Whether the rule selects the file. */
  selects(file: Path): boolean {
    return this.selected.has(key(file));
  }

  /**
   * The files that a list of file patterns of the check matches, by the name of the list.
   * The files match as the files of a rule do. Throws when the check names no such list.
   */
  matched(list: string): readonly Path[] {
    const files = this.lists.get(list);
    if (files === undefined) {
      throw new TypeError(`refpact: rule \`${this.ruleName}\`: the check names no list of files \`${list}\``);
    }
    return files;
  }

  /**
   * The graph of all files, selected or not, from the imports that the rule sees. An import
   * leads to the file of its first path that a file has, and an import of the own file is
   * no edge.
   */
  graph(): Graph {
    this.made ??= new Graph(this.facts, this.selection);
    return this.made;
  }

  /**
   * A violation at the place of a fact, with its note. A message gives a `custom` detail,
   * which a baseline counts by `key`, or by the message when there is no key.
   */
  violationAt(fact: Fact, detail: string | Detail, key?: string): Violation {
    return {
      rule: this.ruleName,
      unit: fact.unit,
      file: fact.file,
      line: fact.line,
      column: fact.column,
      ...(fact.note !== undefined ? { note: fact.note } : {}),
      detail: custom(detail, key),
    };
  }

  /** A violation at the place of a declaration. */
  violationOfItem(declaration: Declaration, detail: string | Detail, baselineKey?: string): Violation {
    const { unit, file, line, column } = declaration;
    return { rule: this.ruleName, unit, file, line, column, detail: custom(detail, baselineKey) };
  }

  /** A violation about a file as a whole, with no line. */
  violationOfFile(file: Path, detail: string | Detail, baselineKey?: string): Violation {
    if (this.firstFiles === undefined) {
      this.firstFiles = new Map();
      for (const fact of this.facts.facts) {
        const unit = key(fact.unit);
        if (!this.firstFiles.has(unit)) {
          this.firstFiles.set(unit, fact.file);
        }
      }
    }
    return { rule: this.ruleName, unit: file, file: this.firstFiles.get(key(file)) ?? "", line: 0, column: 0, detail: custom(detail, baselineKey) };
  }
}

/**
 * What a check of items gets: the {@link Scope} of the rule, and the items that the filters
 * of the rule select.
 */
export class ItemScope extends Scope {
  /** The items that the rule selects, in order. */
  readonly items: readonly Declaration[];
  /**
   * The imports that the rule sees inside the selected items. A front end that does not say
   * which item holds an import gives none: the TypeScript front end is one.
   */
  readonly itemImports: readonly SeenImport[];

  /** @internal */
  constructor(ruleName: string, facts: FactSet, selection: Selection, names: ListNames) {
    super(ruleName, facts, selection, names);
    const declarations = facts.declarations ?? [];
    this.items = (selection.items ?? []).map((position) => declarations[position] as Declaration);
    this.itemImports = this.imports.filter((_, index) => selection.references[index]?.inside === true);
  }
}

/** The graph of the files of a project: see {@link Scope.graph}. */
export class Graph {
  private readonly units: readonly Path[];
  private readonly numbers: Map<string, number>;
  /** For each file, by its number: the edges, each with the number of the file that it leads to. */
  private readonly edges: NumberedEdge[][];

  /** @internal */
  constructor(facts: FactSet, selection: Selection) {
    this.units = facts.units;
    this.numbers = new Map();
    this.units.forEach((unit, number) => {
      if (!this.numbers.has(key(unit))) {
        this.numbers.set(key(unit), number);
      }
    });
    this.edges = this.units.map(() => []);
    const known = new Set<number>();
    const count = this.units.length;
    for (const seen of selection.references) {
      const path = seen.owners.findIndex((owner) => owner !== null);
      const to = seen.owners[path];
      if (to === undefined || to === null || to === seen.unit || known.has(seen.unit * count + to)) {
        continue;
      }
      known.add(seen.unit * count + to);
      const fact = facts.facts[seen.fact] as Fact;
      const reference = fact.what as Reference;
      const target = [reference.target, ...reference.aliases][path] as Path;
      (this.edges[seen.unit] as NumberedEdge[]).push({ to: this.units[to] as Path, node: to, fact, target });
    }
  }

  /** The edges of a file, in the order of their first imports. A path that is no file has none. */
  edgesOf(file: Path): readonly ImportEdge[] {
    const node = this.numbers.get(key(file));
    return node === undefined ? [] : (this.edges[node] as NumberedEdge[]);
  }

  /**
   * A shortest chain of imports from a file to a file that is an end, as the steps of a
   * report. No steps: the file itself is an end. `undefined`: no chain leads to an end. The
   * search is breadth-first with the edges of each file in their order.
   */
  chain(from: Path, isEnd: (file: Path) => boolean): Step[] | undefined {
    const start = this.numbers.get(key(from));
    if (start === undefined) {
      return undefined;
    }
    // For each file that the search found: the file and the edge that led to it.
    const foundBy = new Map<number, { from: number; edge: ImportEdge }>();
    const queue = [start];
    for (let next = 0; next < queue.length; next++) {
      const node = queue[next] as number;
      if (isEnd(this.units[node] as Path)) {
        const steps: Step[] = [];
        for (let current = node, found = foundBy.get(current); found !== undefined; current = found.from, found = foundBy.get(current)) {
          const { fact, target } = found.edge;
          const kind = (fact.what as Reference).kind;
          steps.push({ from: this.units[found.from] as Path, to: this.units[current] as Path, file: fact.file, line: fact.line, column: fact.column, target, kind });
        }
        return steps.reverse();
      }
      for (const edge of this.edges[node] ?? []) {
        if (edge.node !== start && !foundBy.has(edge.node)) {
          foundBy.set(edge.node, { from: node, edge });
          queue.push(edge.node);
        }
      }
    }
    return undefined;
  }
}

function key(path: Path): string {
  return path.join("\u0000");
}

function custom(detail: string | Detail, key: string | undefined): Detail {
  if (typeof detail !== "string") {
    return detail;
  }
  return { type: "custom", message: detail, ...(key !== undefined ? { key } : {}) };
}
