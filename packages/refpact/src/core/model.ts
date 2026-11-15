/** A path: a non-empty list of non-empty segments (SPEC section 1). */
export type Path = readonly string[];

/** A raw pattern: segments that the core validates (SPEC section 2). */
export type RawPattern = readonly string[];

/** What a front end found in one project (SPEC section 3). */
export interface FactSet {
  /** Text that separates segments when a path is printed. */
  readonly separator: string;
  /** Paths of all units. Rust: modules. TypeScript: files. */
  readonly units: readonly Path[];
  /** Paths of all known named things inside units. */
  readonly items: readonly Path[];
  /** The number of lines of each unit, in the order of `units`. Absent or empty for front ends that do not count them. */
  readonly unitLines?: readonly number[];
  /**
   * The file of each unit, in the order of `units`. Absent or empty for front ends that do
   * not give them: the file of a unit is then that of its first fact.
   */
  readonly unitFiles?: readonly string[];
  /** All reference kinds the front end can produce. */
  readonly kinds: readonly string[];
  /** All markers the front end can produce. */
  readonly markers: readonly string[];
  /** All tags the front end can produce. */
  readonly tags: readonly string[];
  /** The facts, in a stable order. */
  readonly facts: readonly Fact[];
  /** The named items that the units declare, in a stable order. Default: none. */
  readonly declarations?: readonly Declaration[];
  /** All item kinds the front end can produce. Default: none. */
  readonly itemKinds?: readonly string[];
  /** All visibilities the front end can produce. Default: none. */
  readonly visibilities?: readonly string[];
  /** All relations the front end can produce. Default: none. */
  readonly relations?: readonly string[];
  /** All modifiers of declarations the front end can produce. Default: none. */
  readonly modifiers?: readonly string[];
  /** The names of all properties of references the front end can produce. Default: none. */
  readonly properties?: readonly string[];
  /** Segments that reports print without a separator before them, by prefix. Default: none. */
  readonly attachedPrefixes?: readonly string[];
  /** Prefixes that reports leave out of the first segment of a path. Default: none. */
  readonly hiddenPrefixes?: readonly string[];
}

/** One observation at one place in the source. */
export interface Fact {
  /** The unit that contains the fact. */
  readonly unit: Path;
  /** The file, as the front end names it. */
  readonly file: string;
  /** The line in the file. */
  readonly line: number;
  /** The column in the line. */
  readonly column: number;
  /** The tags of the fact. */
  readonly tags: readonly string[];
  /** Text that is copied to the violation. */
  readonly note?: string;
  /**
   * The path of the innermost declaration that contains the fact. Absent outside every
   * declaration, and for front ends that do not record it.
   */
  readonly item?: Path;
  /** What was observed. */
  readonly what: Reference | Marker;
}

/** A named item that a unit declares, such as a type or a function (SPEC section 3). */
export interface Declaration {
  /** The path of the item. */
  readonly path: Path;
  /** The name of the item as the code writes it. */
  readonly name: string;
  /** The unit that declares the item. */
  readonly unit: Path;
  /** The file, as the front end names it. */
  readonly file: string;
  /** The line of the name. */
  readonly line: number;
  /** The column of the name. */
  readonly column: number;
  /** The line on which the item ends. Absent for front ends that do not record it. */
  readonly endLine?: number;
  /** The tags of the declaration. */
  readonly tags: readonly string[];
  /** The kind of item, one of the item kinds of the fact set. */
  readonly kind: string;
  /** The visibility, one of the visibilities of the fact set. */
  readonly visibility: string;
  /** The paths that the item is tied to, such as the classes that it extends. */
  readonly relations: readonly Relation[];
  /**
   * What the declaration says of the item besides its kind and its visibility, such as
   * `async` or `documented`. Each is one of the modifiers of the fact set.
   */
  readonly modifiers: readonly string[];
}

/** A tie between a declared item and a path. */
export interface Relation {
  /** The kind of tie, one of the relations of the fact set. */
  readonly relation: string;
  /** The path. */
  readonly target: Path;
}

/** The unit refers to a path. */
export interface Reference {
  /** Tells a reference from a marker. */
  readonly type: "reference";
  /** The path the code refers to, resolved as far as the front end can. */
  readonly target: Path;
  /** Further paths the same reference denotes, such as the definition behind a re-export. */
  readonly aliases: readonly Path[];
  /** The kind of the reference, one of the kinds of the fact set. */
  readonly kind: string;
  /** Whether the front end proved the path. */
  readonly certainty: Certainty;
  /**
   * What the front end knows of the target besides its path, by name, such as the version
   * of a package. No built-in rule reads them; a check of your own does.
   */
  readonly properties?: Readonly<Record<string, string>>;
}

/** The unit contains something that has a name but no path, such as an unsafe block. */
export interface Marker {
  /** Tells a marker from a reference. */
  readonly type: "marker";
  /** The marker, one of the markers of the fact set. */
  readonly marker: string;
}

/** `candidate`: the front end could not prove this path, for example a name behind a glob import. */
export type Certainty = "exact" | "candidate";

/** One architecture rule (SPEC section 4). */
export interface Rule {
  /** The name, printed with every violation. An empty name is an error. */
  readonly name: string;
  /** Why the rule exists, printed with its violations. Default: no reason. */
  readonly reason?: string;
  /**
   * `error`: a violation fails the check. `warning`: a violation is reported, and the check
   * passes. It does not change the evaluation. Default: `error`.
   */
  readonly severity?: Severity;
  /** The units the rule applies to. */
  readonly subjects: Subjects;
  /** What the units must do or must not do. */
  readonly constraint: Constraint;
  /** The reference kinds the rule looks at. Default: all kinds. */
  readonly kinds?: KindFilter;
  /** Facts with one of these tags are not checked. Default: none. */
  readonly ignoreTags?: readonly string[];
  /** Whether candidate references count. Default: `include`. */
  readonly candidates?: Candidates;
  /** References that the rule does not see. Default: none. */
  readonly exemptions?: readonly Exemption[];
  /**
   * The names of the properties of references that the check of the rule reads. One that the
   * front end does not produce is an error. Default: none.
   */
  readonly properties?: readonly string[];
}

/**
 * References that a rule does not see (SPEC 4.13): those of the units that `from` selects,
 * with a path that a `to` pattern matches within.
 */
export interface Exemption {
  /** The units, selected as the subjects of a rule are. */
  readonly from: Subjects;
  /** The paths. An empty list is an error. */
  readonly to: readonly RawPattern[];
}

/** A selection of units: those a rule applies to, or those of an exemption. */
export interface Subjects {
  /** The units that are selected before the exceptions. */
  readonly include: Include;
  /** Absent: no exceptions. An empty list is an error. */
  readonly except?: readonly RawPattern[];
}

/** The units a rule selects before its exceptions: all units, or those that a pattern matches exactly. */
export type Include =
  | { readonly type: "all" }
  | {
      readonly type: "any-of";
      /** The patterns. An empty list is an error. */
      readonly patterns: readonly RawPattern[];
    };

/**
 * What a rule demands of its subjects (SPEC section 4).
 *
 * - `must-not-use`: no reference may lead to one of the targets.
 * - `must-only-use`: every reference must lead to one of the targets, or stay inside its own unit.
 * - `must-not-contain`: the units must not contain one of the markers.
 * - `must-be-acyclic`: the references between the units, or between the slices, must not form a cycle.
 * - `must-be-independent`: no reference may lead from one slice to another slice.
 * - `must-be-used`: each unit must be referred to from outside itself and the units below it.
 * - `must-not-reach`: no chain of references may lead from the units to one of the targets.
 * - `must-be-reachable-from`: a chain of references must lead from one of the roots to each unit.
 * - `must-be-in`: each unit must be matched exactly by one of the places.
 * - `must-only-be-used-by`: only the users, and the selected units, may refer to the selected units.
 * - `items`: the declarations of the units that the filters select must meet a demand.
 * - `custom`: a check of the binding, which finds the violations itself (SPEC 4.15). Only
 *   `select` takes it.
 */
export type Constraint =
  | {
      readonly type: "must-not-use";
      /** The paths that the units must not refer to. */
      readonly targets: Targets;
    }
  | {
      readonly type: "must-only-use";
      /** The only paths that the units may refer to outside themselves. */
      readonly targets: Targets;
    }
  | {
      readonly type: "must-not-contain";
      /** The markers. An empty list is an error. */
      readonly markers: readonly string[];
    }
  | {
      readonly type: "must-be-acyclic";
      /** Without `slices`, each unit is a node. An empty list is an error. */
      readonly slices?: readonly RawPattern[];
    }
  | {
      readonly type: "must-be-independent";
      /** The patterns of the slices. An empty list is an error. */
      readonly slices: readonly RawPattern[];
    }
  | { readonly type: "must-be-used" }
  | {
      readonly type: "must-not-reach";
      /** The paths that no chain from the units may end at. */
      readonly targets: Targets;
    }
  | {
      readonly type: "must-be-reachable-from";
      /** The patterns of the root units. An empty list is an error. */
      readonly roots: readonly RawPattern[];
    }
  | {
      readonly type: "must-be-in";
      /** The places, which match units exactly. An empty list is an error. */
      readonly places: readonly RawPattern[];
    }
  | {
      readonly type: "must-only-be-used-by";
      /** The units that may refer to the selected units, which match units exactly. An empty list is an error. */
      readonly users: readonly RawPattern[];
    }
  | {
      /** Each unit must have at most a number of lines (SPEC 4.16). */
      readonly type: "must-not-exceed-lines";
      /** The number of lines that a unit may have. */
      readonly lines: number;
    }
  | ({ readonly type: "items" } & Items)
  | CustomConstraint;

/**
 * A check of the binding (SPEC 4.15), as a constraint or as the demand of a rule about
 * items. The core applies the rest of the rule, and `select` answers what the rule sees.
 */
export interface CustomConstraint {
  /** Tells a custom check from the built-in ones. */
  readonly type: "custom";
  /** Lists of paths that the check looks for. For each list, `select` marks the first path of each reference that it matches. */
  readonly targets?: readonly Targets[];
  /** Lists of unit patterns. For each list, `select` answers the units that it matches exactly. */
  readonly units?: readonly (readonly RawPattern[])[];
}

/** A rule about declarations (SPEC 4.12): filters, which all must hold, and a demand. */
export interface Items {
  /** Absent: any name. Otherwise a glob must match the name. An empty list is an error. */
  readonly named?: readonly string[];
  /** Absent: any kind. An empty list is an error. */
  readonly kinds?: readonly string[];
  /** Absent: any visibility. An empty list is an error. */
  readonly visibilities?: readonly string[];
  /** The relations that the item must have, each to one of its paths. Absent or empty: any. */
  readonly related?: readonly Related[];
  /** Absent: any modifiers. Otherwise the item must have one of these. An empty list is an error. */
  readonly modifiers?: readonly string[];
  /** What each selected declaration must meet. */
  readonly must: Demand;
}

/** A relation to one of some paths. */
export interface Related {
  /** The kind of tie, such as `extends`. */
  readonly relation: string;
  /** The paths, which match the target within. An empty list is an error. */
  readonly anyOf: readonly RawPattern[];
}

/**
 * What each declaration of a rule about items must meet: not to exist, to reside in one of
 * some units, to have a name that a glob matches, to have a relation or a modifier.
 */
export type Demand =
  | {
      /** No declaration may be selected. */
      readonly type: "not-exist";
    }
  | {
      /** The unit of the declaration must be one of the places. */
      readonly type: "reside-in";
      /** The places, which match units exactly. An empty list is an error. */
      readonly places: readonly RawPattern[];
    }
  | {
      /** A glob must match the name. */
      readonly type: "be-named";
      /** The globs. An empty list is an error. */
      readonly names: readonly string[];
    }
  | ({
      /** The item must have the relation to one of the paths. */
      readonly type: "have";
    } & Related)
  | {
      /** The item must have one of the modifiers. */
      readonly type: "have-modifier";
      /** The modifiers. An empty list is an error. */
      readonly modifiers: readonly string[];
    }
  | {
      /** No reference inside the item may lead to one of the targets (SPEC 4.12). */
      readonly type: "not-use";
      /** The paths that the code of the item must not refer to. */
      readonly targets: Targets;
    }
  | {
      /** Every reference inside the item must lead to one of the targets, or stay in its unit (SPEC 4.12). */
      readonly type: "only-use";
      /** The only paths that the code of the item may refer to outside its unit. */
      readonly targets: Targets;
    }
  | {
      /** The item must have at most a number of lines (SPEC 4.16). */
      readonly type: "not-exceed-lines";
      /** The number of lines that an item may have. */
      readonly lines: number;
    }
  | CustomConstraint;

/** Targets of a rule about uses: the union of both lists. */
export interface Targets {
  /** Checked for existence when they are inside the analysed project. */
  readonly anyOf: readonly RawPattern[];
  /** Never checked for existence. */
  readonly unverified: readonly RawPattern[];
}

/** How much a violation of a rule weighs (SPEC section 4). */
export type Severity = "error" | "warning";

/** Which reference kinds a rule looks at: all kinds, some kinds only, or all kinds except some. */
export type KindFilter =
  | { readonly type: "all" }
  | {
      readonly type: "only";
      /** The kinds the rule looks at. */
      readonly kinds: readonly string[];
    }
  | {
      readonly type: "except";
      /** The kinds the rule does not look at. */
      readonly kinds: readonly string[];
    };

/** Whether a rule looks at candidate references. `include`: they count like exact references. */
export type Candidates = "include" | "ignore";
