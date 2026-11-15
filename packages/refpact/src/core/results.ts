/** What the core gives back: violations, errors, and baselines (SPEC sections 4, 5 and 7). */
import type { Certainty, FactSet, Path } from "./model.ts";

/** The violations in fact order, or the reasons why the rule cannot be evaluated. */
export type EvaluateResult =
  | {
      readonly ok: true;
      /** The violations in fact order. May be empty. */
      readonly violations: readonly Violation[];
    }
  | {
      readonly ok: false;
      /** The reasons, in the order of the specification. */
      readonly errors: readonly RuleError[];
    };

/** One place where the code breaks a rule. */
export interface Violation {
  /** The name of the rule. */
  readonly rule: string;
  /** The unit that breaks the rule. */
  readonly unit: Path;
  /**
   * The file of the place. For a violation about a unit as a whole, the file of the first
   * fact of the unit, or empty.
   */
  readonly file: string;
  /** The line, or 0 for a violation about a unit as a whole. */
  readonly line: number;
  /** The column, or 0 for a violation about a unit as a whole. */
  readonly column: number;
  /** The note of the fact, when it has one. */
  readonly note?: string;
  /** What the unit does. */
  readonly detail: Detail;
}

/**
 * What a violation is about.
 *
 * - `uses`: the unit refers to a path that the rule does not allow.
 * - `contains`: the unit contains a marker that the rule forbids.
 * - `cycle`: one cycle of a tangle, which is a group of nodes that all reach each other.
 * - `unused`: nothing outside the unit and the units below it refers to it.
 * - `reaches`: a chain of references leads from the unit to a forbidden target.
 * - `unreachable`: no chain of references leads from a root to the unit.
 * - `outside`: the unit is in none of the places of the rule.
 * - `item`: a declaration that does not meet a rule about items.
 * - `custom`: what a check of the user found.
 */
export type Detail =
  | {
      readonly type: "uses";
      /** For a rule about items: the item whose code has the reference. */
      readonly item?: Path;
      /** The forbidden path. */
      readonly target: Path;
      /** The path as the code names it, when it differs from `target`. */
      readonly written?: Path;
      /** The kind of the reference. */
      readonly kind: string;
      /** Whether the front end proved the path. */
      readonly certainty: Certainty;
    }
  | {
      readonly type: "contains";
      /** The marker. */
      readonly marker: string;
    }
  | {
      readonly type: "cycle";
      /** The edges of the cycle, in order. The last step leads back to the first node. */
      readonly steps: readonly Step[];
      /** The number of nodes in the tangle. */
      readonly size: number;
    }
  | { readonly type: "unused" }
  | {
      readonly type: "reaches";
      /** The edges from the unit to the unit that uses the target. Empty when the unit uses the target itself. */
      readonly steps: readonly Step[];
      /** The use of the target at the end of the chain. */
      readonly end: End;
    }
  | { readonly type: "unreachable" }
  | { readonly type: "outside" }
  | {
      readonly type: "item";
      /** The path of the item. */
      readonly item: Path;
      /** The kind of the item. */
      readonly kind: string;
    }
  | {
      /** The unit, or one declaration of it, has more lines than the rule allows (SPEC 4.16). */
      readonly type: "too-long";
      /** For a rule about items: the path of the item. */
      readonly item?: Path;
      /** For a rule about items: the kind of the item. */
      readonly kind?: string;
      /** The number of lines. */
      readonly lines: number;
      /** The number of lines that the rule allows. */
      readonly limit: number;
    }
  | {
      readonly type: "custom";
      /** What the unit does, printed after the unit. */
      readonly message: string;
      /** What a baseline counts the violation as. Absent: the message. */
      readonly key?: string;
    };

/** What a rule sees (SPEC section 8), or the reasons why the rule cannot be evaluated. Units, facts and declarations are given by their position in the fact set. */
export type SelectResult =
  | {
      /** The rule can be evaluated. */
      readonly ok: true;
      /** The units that the rule selects. */
      readonly subjects: readonly number[];
      /** For each list of unit patterns of the check: the units that it matches. */
      readonly units: readonly (readonly number[])[];
      /** The references that the rule sees, of all units, in the order of the facts. */
      readonly references: readonly SelectedReference[];
      /** The markers of the subjects that the rule sees. */
      readonly markers: readonly number[];
      /** The declarations of the subjects that the rule sees. */
      readonly declarations: readonly number[];
      /** For a rule about items: the declarations that its filters select. */
      readonly items?: readonly number[];
    }
  | {
      /** The rule cannot be evaluated. */
      readonly ok: false;
      /** The reasons, in the order of the specification. */
      readonly errors: readonly RuleError[];
    };

/** The tangles of a graph of what a rule sees (SPEC section 8), or the reasons why the rule cannot be evaluated. */
export type TanglesResult =
  | {
      /** The rule can be evaluated. */
      readonly ok: true;
      /** The tangles. A step of `cycle` is `[node, edge]`, with the place of the edge in the edges of the node. */
      readonly tangles: readonly {
        /** The nodes of the tangle, in ascending order. */
        readonly members: readonly number[];
        /** A shortest cycle through the first member. */
        readonly cycle: readonly (readonly [number, number])[];
      }[];
    }
  | {
      /** The rule cannot be evaluated. */
      readonly ok: false;
      /** The reasons, in the order of the specification. */
      readonly errors: readonly RuleError[];
    };

/** The edges of a graph of what a rule sees (SPEC section 8), or the reasons why the rule cannot be evaluated. */
export type GraphResult =
  | {
      /** The rule can be evaluated. */
      readonly ok: true;
      /** The unit of each node, by position in the fact set. Absent for the unit graph, where a node is its unit. */
      readonly nodes?: readonly number[];
      /** For each node: its edges, each as `[to, fact, path]`, with the place of the path in `[target, ...aliases]`. */
      readonly edges: readonly (readonly (readonly [number, number, number])[])[];
    }
  | {
      /** The rule cannot be evaluated. */
      readonly ok: false;
      /** The reasons, in the order of the specification. */
      readonly errors: readonly RuleError[];
    };

/** A reference that a rule sees. */
export interface SelectedReference {
  /** The position of the fact. */
  readonly fact: number;
  /** The unit of the fact. */
  readonly unit: number;
  /** For each path of the reference, `[target, ...aliases]`: the unit that owns it, or `null`. */
  readonly owners: readonly (number | null)[];
  /**
   * For each list of targets of the check: the first path that it matches, by its place in
   * `[target, ...aliases]`, or `null`. Absent when no list matches a path.
   */
  readonly hits?: readonly (number | null)[];
  /** For a rule about items: the reference is inside an item that the filters select. */
  readonly inside?: true;
}

/** The reference at the end of a chain: the use of the forbidden target. */
export interface End {
  /** The unit that uses the target. */
  readonly unit: Path;
  /** The file of the use. */
  readonly file: string;
  /** The line of the use. */
  readonly line: number;
  /** The column of the use. */
  readonly column: number;
  /** The forbidden path. */
  readonly target: Path;
  /** The kind of the reference. */
  readonly kind: string;
}

/** One edge of a cycle or of a chain, with the reference that makes it. */
export interface Step {
  /** The node the edge leaves. */
  readonly from: Path;
  /** The node the edge leads to. */
  readonly to: Path;
  /** The file of the reference. */
  readonly file: string;
  /** The line of the reference. */
  readonly line: number;
  /** The column of the reference. */
  readonly column: number;
  /** The path the reference names. */
  readonly target: Path;
  /** The kind of the reference. */
  readonly kind: string;
}

/** Why a rule cannot be evaluated (SPEC section 5). */
export interface RuleError {
  /** The check that failed. */
  readonly code: ErrorCode;
  /** The part of the rule or of the fact set that the error is about, when there is one. */
  readonly role?: Role;
  /** The offending value, such as a pattern, when there is one. */
  readonly value?: string;
}

/** The check that failed (SPEC section 5). */
export type ErrorCode =
  | "invalid-facts"
  | "empty-name"
  | "empty-list"
  | "invalid-pattern"
  | "pattern-matches-nothing"
  | "no-subjects"
  | "too-few-nodes"
  | "unknown-marker"
  | "unknown-kind"
  | "unknown-tag"
  | "unused-exemption"
  | "unknown-item-kind"
  | "unknown-visibility"
  | "unknown-relation"
  | "unknown-modifier"
  | "unknown-property";

/** The part of a rule or of a fact set that an error is about. */
export type Role =
  | "unit"
  | "subject"
  | "subject-exception"
  | "target"
  | "unverified-target"
  | "marker"
  | "slice"
  | "root"
  | "kind"
  | "tag"
  | "place"
  | "user"
  | "exemption-from"
  | "exemption-exception"
  | "exemption-to"
  | "exemption"
  | "name"
  | "item-kind"
  | "visibility"
  | "relation"
  | "related"
  | "required-name"
  | "required"
  | "modifier"
  | "required-modifier"
  | "layer"
  | "property";

/**
 * How reports and baselines print paths (SPEC section 3). A fact set is a path format, so a
 * report can take the fact set itself.
 */
export type PathFormat = Pick<FactSet, "separator" | "attachedPrefixes" | "hiddenPrefixes">;

/** One line of a baseline: `count` accepted violations with one key. */
export interface BaselineEntry {
  /** The name of the rule. */
  readonly rule: string;
  /** The unit, printed as in reports. */
  readonly unit: string;
  /** What the violations are, as SPEC section 7 writes it for each kind of violation. */
  readonly what: string;
  /** The number of accepted violations with this key. */
  readonly count: number;
}

/** An entry that accepts more violations than the code has. */
export interface StaleEntry {
  /** The unit of the entry. */
  readonly unit: string;
  /** What the entry accepts, as in {@link BaselineEntry.what}. */
  readonly what: string;
  /** The number of violations that the baseline accepts for this key. */
  readonly count: number;
  /** The number of violations that the code has. */
  readonly found: number;
}

/** The entries, or the number of the first invalid line, from 1. */
export type ParsedBaseline =
  | {
      readonly ok: true;
      /** The entries, in the order of the text. */
      readonly entries: readonly BaselineEntry[];
    }
  | {
      readonly ok: false;
      /** The number of the first invalid line, from 1. */
      readonly line: number;
    };

/** The result of `applyBaseline`. */
export interface Applied {
  /** The violations that the baseline does not accept. */
  readonly remaining: readonly Violation[];
  /** The entries that accept more violations than the code has. */
  readonly stale: readonly StaleEntry[];
}

/** The key of a violation in a baseline. */
export interface BaselineKey {
  /** The unit, printed as in reports. */
  readonly unit: string;
  /** What the violation is, as SPEC section 7 writes it. */
  readonly what: string;
}
