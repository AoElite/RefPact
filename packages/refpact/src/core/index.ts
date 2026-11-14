/**
 * The language-neutral core: rules evaluated over facts, reports and baselines.
 *
 * The core is the Rust crate `refpact-core`, run as WebAssembly. This module turns its JSON
 * requests (SPEC section 8) into functions. A fact set crosses into the core once, on its
 * first evaluation, and stays there while the object lives, so each further rule sends only
 * the rule.
 */
import type { FactSet, Path, RawPattern, Rule, Severity } from "./model.ts";
import type {
  Applied,
  BaselineEntry,
  BaselineKey,
  EvaluateResult,
  GraphResult,
  ParsedBaseline,
  PathFormat,
  RuleError,
  SelectResult,
  StaleEntry,
  TanglesResult,
  Violation,
} from "./results.ts";
import { call, currentGeneration } from "./wasm.ts";

export type * from "./model.ts";
export type * from "./results.ts";

interface Loaded {
  readonly handle: number;
  readonly generation: number;
}

const loaded = new WeakMap<FactSet, Loaded>();
const unload = new FinalizationRegistry<Loaded>(({ handle, generation }) => {
  if (generation === currentGeneration()) {
    call(JSON.stringify({ op: "drop-facts", handle }));
  }
});

/** The handle of a fact set in the core, after loading it when it is not loaded. */
function handleOf(facts: FactSet): number {
  const generation = currentGeneration();
  const known = loaded.get(facts);
  if (known !== undefined && known.generation === generation) {
    return known.handle;
  }
  const { handle } = JSON.parse(call(`{"op":"load-facts","facts":${JSON.stringify(facts)}}`)) as { handle: number };
  const entry = { handle, generation };
  loaded.set(facts, entry);
  unload.register(facts, entry);
  return handle;
}

/** Only the fields of a path format, so that a fact set given as the format does not cross whole. */
function formatOf(format: PathFormat): PathFormat {
  return {
    separator: format.separator,
    ...(format.attachedPrefixes !== undefined && { attachedPrefixes: format.attachedPrefixes }),
    ...(format.hiddenPrefixes !== undefined && { hiddenPrefixes: format.hiddenPrefixes }),
  };
}

function request<T>(op: string, body: object): T {
  return JSON.parse(call(JSON.stringify({ op, ...body }))) as T;
}

/** Evaluates one rule against one fact set (SPEC section 4). */
export function evaluate(rule: Rule, facts: FactSet): EvaluateResult {
  return request("evaluate", { handle: handleOf(facts), rule });
}

/**
 * Applies the subjects, the filters and the exemptions of a rule with a custom check, and
 * answers what the rule sees (SPEC section 8), so that the caller finds the violations. The
 * check is the constraint of the rule, or the demand of a rule about items.
 */
export function select(rule: Rule, facts: FactSet): SelectResult {
  return request("select", { handle: handleOf(facts), rule });
}

/**
 * The edges of a graph of what a rule sees, as the core makes them (SPEC section 8): the unit
 * graph, or with `subjects` the graph of the subjects.
 */
export function graph(rule: Rule, facts: FactSet, subjects: boolean): GraphResult {
  return request("graph", { handle: handleOf(facts), rule, subjects });
}

/** The tangles of the graph that {@link graph} gives. The core searches them only for this request. */
export function tangles(rule: Rule, facts: FactSet, subjects: boolean): TanglesResult {
  return request("graph", { handle: handleOf(facts), rule, subjects, tangles: true });
}

/** The text form of a diagram: a Mermaid flowchart, or a Graphviz graph in DOT. */
export type DiagramFormat = "mermaid" | "dot";

/** The diagram of a rule, or the reasons why the rule cannot be evaluated. */
export type DiagramResult =
  | {
      /** The rule can be evaluated. */
      readonly ok: true;
      /** The diagram. */
      readonly text: string;
    }
  | {
      /** The rule cannot be evaluated. */
      readonly ok: false;
      /** The reasons, in the order of the specification. */
      readonly errors: readonly RuleError[];
    };

/** A layer of a diagram of layers (SPEC 4.17). */
export interface DiagramLayer {
  /** The name of the layer, which is the label of its node. */
  readonly name: string;
  /** The units of the layer: unit patterns, which match units exactly. */
  readonly units: readonly RawPattern[];
  /** The layers that the layer must not use, by their position in the list of layers. */
  readonly mustNotUse: readonly number[];
}

/**
 * The nodes of a diagram: the subjects of the rule when this is `undefined`, the slices of
 * the subjects, or the layers from the top to the bottom.
 */
export type DiagramNodes =
  | {
      /** The slice patterns, as for `must-be-acyclic` with slices. */
      readonly slices: readonly RawPattern[];
    }
  | {
      /** The layers, from the top to the bottom. */
      readonly layers: readonly DiagramLayer[];
    }
  | undefined;

/**
 * The diagram of what a rule sees (SPEC 4.17): its subjects, the slices or the layers, and the
 * references between them. The constraint of the rule is not evaluated.
 */
export function diagram(rule: Rule, facts: FactSet, nodes: DiagramNodes, format: DiagramFormat = "mermaid"): DiagramResult {
  return request("diagram", { handle: handleOf(facts), rule, ...nodes, format });
}

/** Prints a path as reports print it. */
export function printPath(path: Path, format: PathFormat): string {
  return request<{ text: string }>("print-path", { path, format: formatOf(format) }).text;
}

/** One line for one violation: file, line, column, rule name, unit and offending path. */
export function renderViolation(violation: Violation, format: PathFormat): string {
  return request<{ text: string }>("render-violation", { violation, format: formatOf(format) }).text;
}

/** All violations of one rule, one per line, with a heading, and the reason of the rule when it has one. */
export function renderViolations(rule: string, violations: readonly Violation[], format: PathFormat, reason?: string, severity: Severity = "error"): string {
  return request<{ text: string }>("render-violations", { rule, reason, severity, violations, format: formatOf(format) }).text;
}

/** One sentence for one error. */
export function renderError(error: RuleError): string {
  return request<{ text: string }>("render-error", { error }).text;
}

/** All errors of one rule, one per line, with a heading. */
export function renderErrors(rule: string, errors: readonly RuleError[]): string {
  return request<{ text: string }>("render-errors", { rule, errors }).text;
}

/** The report of the stale entries of one rule in a baseline file. */
export function renderStale(rule: string, file: string, entries: readonly StaleEntry[]): string {
  return request<{ text: string }>("render-stale", { rule, file, entries }).text;
}

/** The `unit` and `what` of the key of a violation (SPEC 7.1). */
export function baselineKey(violation: Violation, format: PathFormat): BaselineKey {
  return request("baseline-key", { violation, format: formatOf(format) });
}

/** One entry for each key of the violations, with the number of violations that have it. */
export function baselineEntries(violations: readonly Violation[], format: PathFormat): BaselineEntry[] {
  return request<{ entries: BaselineEntry[] }>("baseline-entries", { violations, format: formatOf(format) }).entries;
}

/** Removes the violations of one rule that the baseline accepts (SPEC 7.3). */
export function applyBaseline(rule: string, violations: readonly Violation[], baseline: readonly BaselineEntry[], format: PathFormat): Applied {
  return request("apply-baseline", { rule, violations, entries: baseline, format: formatOf(format) });
}

/** Writes the text form of a baseline, sorted, so that a change shows as a small difference. */
export function formatBaseline(entries: readonly BaselineEntry[]): string {
  return request<{ text: string }>("format-baseline", { entries }).text;
}

/** Reads the text form of a baseline (SPEC 7.2). */
export function parseBaseline(text: string): ParsedBaseline {
  return request("parse-baseline", { text });
}
