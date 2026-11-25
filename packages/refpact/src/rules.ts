/** Rules for TypeScript projects, written as plain values in a test file. */
import { isBuiltin } from "node:module";

import { updateRequested } from "./baseline.ts";
import type { Baseline } from "./baseline.ts";
import { applyBaseline, diagram, evaluate, graph, renderErrors, renderStale, renderViolations, select, tangles } from "./core/index.ts";
import type { DiagramFormat, DiagramNodes } from "./core/index.ts";
import { verifyDiagram } from "./diagram.ts";
import type {
  Constraint,
  CustomConstraint,
  EvaluateResult,
  RawPattern,
  Rule as CoreRule,
  RuleError,
  StaleEntry,
  Violation,
} from "./core/index.ts";
import { ItemScope, Scope } from "./custom.ts";
import { addFilters } from "./filters.ts";
import type { Filter, Filters } from "./filters.ts";
import { filePattern, subjects, targetPattern } from "./patterns.ts";
import type { ListNames } from "./custom.ts";
import { KINDS, MARKERS } from "./frontend/imports.ts";
import { ItemSelection } from "./items.ts";
import type { Kind, MarkerName as Marker } from "./frontend/imports.ts";
import { exportSegment, globalSegment, nodeSegment, packageSegment } from "./frontend/paths.ts";
import type { Project } from "./frontend/project.ts";
import { DEPENDENCY_KINDS, NAMED_DEPENDENCY_KINDS, PACKAGE_ITEM_KIND } from "./frontend/dependencies.ts";
import type { DependencyKind } from "./frontend/dependencies.ts";
import path from "node:path";

/** A package, a subpath of a package, or one export of it, as a target of a rule. */
export class PackageTarget {
  /** @internal */
  readonly segments: RawPattern;

  /** @internal */
  constructor(segments: RawPattern) {
    this.segments = segments;
  }

  /** One export of the package: `pkg("lodash").export("debounce")`. Use `default` for the default export. */
  export(name: string): PackageTarget {
    return new PackageTarget([...this.segments, exportSegment(name)]);
  }
}

/**
 * A package as a target: `pkg("lodash")`, `pkg("@scope/name")`, `pkg("lodash/fp")` for a
 * subpath, `pkg("node:fs")` or `pkg("fs")` for a module of Node. The target covers all
 * subpaths and all exports of the package. `*` matches a part of the name: `pkg("@aws-sdk/*")`.
 */
export function pkg(specifier: string): PackageTarget {
  if (specifier.startsWith("node:") || isBuiltin(specifier)) {
    const [name, ...rest] = specifier.replace(/^node:/, "").split("/") as [string, ...string[]];
    return new PackageTarget([nodeSegment(name), ...rest]);
  }
  const parts = specifier.split("/");
  const nameLength = specifier.startsWith("@") ? 2 : 1;
  return new PackageTarget([packageSegment(parts.slice(0, nameLength).join("/")), ...parts.slice(nameLength)]);
}

/**
 * A global of the host as a target: `globalApi("fetch")`, `globalApi("process")`, or one member
 * of it, `globalApi("process.env")`. The target of a global covers its members. A use of a
 * global counts as an import of it, of the kind `global`: see {@link KINDS}. `*` matches a
 * part of the name: `globalApi("*Storage")`.
 *
 * A global is a name that no file of the project declares and that the ECMAScript library
 * does not declare: the globals of Node, of the browser and of other hosts, such as
 * `process`, `fetch`, `window`, `document` and `localStorage`. `Math`, `Date`, `JSON` and
 * `Promise` are part of the language and are no globals here.
 */
export function globalApi(name: string): PackageTarget {
  const [global, ...members] = name.split(".") as [string, ...string[]];
  return new PackageTarget([globalSegment(global), ...members.slice(0, 1).map(exportSegment)]);
}

/** A file pattern, a package or a global. */
export type Target = string | PackageTarget;

/** A selection of files. */
export class Files {
  /** @internal */
  readonly include: readonly string[] | undefined;
  /** @internal */
  readonly excluded: readonly string[] | undefined;

  /** @internal */
  constructor(include: readonly string[] | undefined, excluded: readonly string[] | undefined) {
    this.include = include;
    this.excluded = excluded;
  }
}

/** A set of forbidden targets. */
export class Targets {
  /** @internal */
  readonly anyOf: readonly Target[];
  /** @internal */
  readonly unverified: readonly Target[];

  /** @internal */
  constructor(anyOf: readonly Target[], unverified: readonly Target[]) {
    this.anyOf = anyOf;
    this.unverified = unverified;
  }
}

/** A list of patterns that form a union. */
export class AnyOf<T extends Target = Target> {
  /** @internal */
  readonly patterns: readonly T[];

  /** @internal */
  constructor(patterns: readonly T[]) {
    this.patterns = patterns;
  }

  /** Removes the files that match one of `patterns` from the selection. */
  except(this: AnyOf<string>, ...patterns: string[]): Files {
    return new Files(this.patterns, patterns);
  }

  /** Adds targets that are not checked for existence. See {@link unverified}. */
  andUnverified(...patterns: Target[]): Targets {
    return new Targets(this.patterns, patterns);
  }
}

/** The union of some patterns: files for `files(..)`, targets for `mustNotImport(..)`. */
export function anyOf(...patterns: string[]): AnyOf<string>;
export function anyOf(...patterns: Target[]): AnyOf<Target>;
export function anyOf(...patterns: Target[]): AnyOf<Target> {
  return new AnyOf(patterns);
}

/** All files of the project. */
export function all(): Files {
  return new Files(undefined, undefined);
}

/** All files of the project except those that match one of the patterns. */
export function allExcept(...patterns: string[]): Files {
  return new Files(undefined, patterns);
}

/**
 * Targets that are not checked for existence. Use this only for files of the project that
 * the compiler does not load, such as generated files that are not there yet.
 */
export function unverified(...patterns: Target[]): Targets {
  return new Targets([], patterns);
}

/** Starts a rule. The name is printed with every violation. */
export function rule(name: string): NamedRule {
  return new NamedRule(name);
}

/** A rule with a name. Say which files it applies to. */
export class NamedRule {
  private readonly name: string;

  /** @internal */
  constructor(name: string) {
    this.name = name;
  }

  /** The files that the rule constrains: `anyOf(..)`, `all()`, `allExcept(..)` or `anyOf(..).except(..)`. */
  files(files: AnyOf<string> | Files): ScopedRule {
    if (files instanceof AnyOf) {
      for (const pattern of files.patterns as readonly unknown[]) {
        if (typeof pattern !== "string") {
          throw new TypeError(`refpact: rule \`${this.name}\`: files(..) takes file patterns, not packages`);
        }
      }
      return new ScopedRule(this.name, new Files(files.patterns, undefined));
    }
    return new ScopedRule(this.name, files);
  }

  /**
   * The items that these files declare at their top level: functions, classes, interfaces,
   * types, enums, variables and namespaces. Narrow them with the filters of
   * {@link ItemSelection}, then say what they must meet, for example with
   * {@link ItemSelection.mustResideIn}.
   */
  items(files: AnyOf<string> | Files): ItemSelection {
    const selection = files instanceof AnyOf ? new Files(files.patterns, undefined) : files;
    return new ItemSelection(this.name, selection, {
      rule: (made, finder) => new ImportRule(made, finder),
      custom: customCheck,
      subjects,
      file: filePattern,
      target: targetPattern,
      targets: coreTargets,
    });
  }
}

/**
 * A rule with a name and files. Say what the files must or must not do, for example with
 * `mustNotImport`, `mustOnlyImport` or `mustBeAcyclic`.
 */
export class ScopedRule {
  private readonly name: string;
  private readonly selection: Files;

  /** @internal */
  constructor(name: string, selection: Files) {
    this.name = name;
    this.selection = selection;
  }

  /**
   * The files must not import any of the targets, in any form: `import`, `import type`,
   * `import()`, `require`, or `export .. from`. An import through a barrel file counts as
   * an import of the file that declares the name.
   */
  mustNotImport(targets: AnyOf | Targets): ImportRule {
    return this.rule({ type: "must-not-use", targets: this.targets(targets) });
  }

  /**
   * Every import of the files must lead to one of the targets. An import is allowed when the
   * module in the code is allowed, or when the file that declares the name is.
   *
   * The list must name everything the files import, also packages and modules of Node:
   * `pkg("*")` is every package, and `pkg("node:*")` is every module of Node.
   */
  mustOnlyImport(targets: AnyOf | Targets): ImportRule {
    return this.rule({ type: "must-only-use", targets: this.targets(targets) });
  }

  /**
   * The files must not form a cycle: no file may reach itself through the imports between
   * them. Each tangle of files is reported once, with one of its cycles.
   */
  mustBeAcyclic(): ImportRule {
    return this.rule({ type: "must-be-acyclic" });
  }

  /**
   * Like {@link ScopedRule.mustBeAcyclic}, between slices. The slice of a file is the
   * shortest start of its path that matches one of the patterns: with `src/features/*`, the
   * file `src/features/cart/view/list.ts` is in the slice `src/features/cart`. Imports inside
   * a slice do not count, and a file without a slice is left out.
   */
  mustBeAcyclicBetween(slices: AnyOf<string>): ImportRule {
    return this.rule({ type: "must-be-acyclic", slices: slices.patterns.map(filePattern) });
  }

  /**
   * No file of one slice may import a file of another slice. Slices are as in
   * {@link ScopedRule.mustBeAcyclicBetween}: with `src/features/*`, no feature may import
   * another feature.
   *
   * A file that the rule does not select is in no slice. Leave a shared part out with
   * `allExcept("src/features/shared/**")`.
   */
  mustBeIndependentBetween(slices: AnyOf<string>): ImportRule {
    return this.rule({ type: "must-be-independent", slices: slices.patterns.map(filePattern) });
  }

  /**
   * The files must not contain any of the markers: `console`, a use of the global
   * `console`, `debugger`, a `debugger` statement, and `unresolved-import`, an import of a
   * relative path, a `paths` alias or a `#name` of `imports` that leads to no file. A local
   * variable named `console` is no marker. An import of a package is never an `unresolved-import`:
   * refpact does not check that a package is installed.
   *
   * ```ts
   * rule("every import resolves").files(all()).mustNotContain("unresolved-import").check(app);
   * ```
   */
  mustNotContain(...markers: Marker[]): ImportRule {
    return this.rule({ type: "must-not-contain", markers });
  }

  /**
   * Each file must have at most this number of lines. The blank lines and the comments
   * count.
   *
   * ```ts
   * rule("no file is longer than 400 lines").files(anyOf("src/**")).mustNotExceedLines(400).check(app);
   * ```
   */
  mustNotExceedLines(lines: number): ImportRule {
    return this.rule({ type: "must-not-exceed-lines", lines });
  }

  /**
   * Each file must be imported by another file. Leave the entry points out of the files:
   * `allExcept("src/main.ts")`.
   */
  mustBeImported(): ImportRule {
    return this.rule({ type: "must-be-used" });
  }

  /**
   * No chain of imports may lead from the files to one of the targets: not the files
   * themselves, and not a file that they import, at any distance. The report shows the
   * shortest chain.
   */
  mustNotReach(targets: AnyOf | Targets): ImportRule {
    return this.rule({ type: "must-not-reach", targets: this.targets(targets) });
  }

  /** A chain of imports must lead from one of the root files to each file. Give the entry points as roots. */
  mustBeReachableFrom(roots: AnyOf<string>): ImportRule {
    return this.rule({ type: "must-be-reachable-from", roots: roots.patterns.map(filePattern) });
  }

  /**
   * Only the importers, and the files of the rule themselves, may import the files of the
   * rule. Every other file that imports one of them breaks the rule, also through a barrel
   * file. Importers match files as the files of a rule do.
   */
  mustOnlyBeImportedBy(importers: AnyOf<string>): ImportRule {
    return this.rule({ type: "must-only-be-used-by", users: importers.patterns.map(filePattern) });
  }

  /**
   * Each file must be one that a place matches. The layers use it for `everyFileInALayer`.
   *
   * @internal
   */
  mustBeIn(places: readonly string[]): ImportRule {
    return this.rule({ type: "must-be-in", places: places.map(filePattern) });
  }

  /**
   * Every package that the files import must be in the `package.json` that the file belongs
   * to: the nearest one above it. Any of `dependencies`, `devDependencies`,
   * `peerDependencies` and `optionalDependencies` counts. A type import of `name` is also
   * declared by `@types/name`. A package may import itself by its own name.
   *
   * The project must be loaded with `readDependencies`: the rule reads the references that
   * the `package.json` files give, as a check of your own can.
   */
  mustOnlyImportDeclaredPackages(): ImportRule {
    return this.custom(ANY_PACKAGE, PACKAGE_NAMES, (scope) => undeclared(scope, "declared"));
  }

  /**
   * The files must not import a package that their `package.json` has only in
   * `devDependencies`. Select the files that are shipped: `allExcept("**\/*.test.ts")`.
   * The project must be loaded with `readDependencies`.
   */
  mustNotImportDevDependencies(): ImportRule {
    return this.custom(ANY_PACKAGE, PACKAGE_NAMES, (scope) => undeclared(scope, "not-development"));
  }

  /**
   * The files must pass a check of your own: a function that gets the {@link Scope} of the
   * rule and returns the violations. The rule selects the files, applies its filters and its
   * exemptions, and reports and baselines the violations, as for the rules above.
   *
   * A function cannot match a pattern, so the check names the lists of patterns that it
   * needs, and the rule matches them: see {@link CheckPatterns}. A pattern that matches
   * nothing is an error, as it is for `mustNotImport`.
   *
   * ```ts
   * rule("a file imports at most ten files")
   *   .files(anyOf("src/**"))
   *   .mustSatisfy((scope) =>
   *     scope.subjects
   *       .filter((file) => scope.imports.filter((seen) => seen.file === file).length > 10)
   *       .map((file) => scope.violationOfFile(file, "imports more than ten files")),
   *   )
   *   .check(app);
   * ```
   */
  mustSatisfy(check: (scope: Scope) => readonly Violation[], patterns?: CheckPatterns): ImportRule {
    const { constraint, names } = customCheck(patterns);
    return this.custom(constraint, names, (scope) => check(scope));
  }

  private targets(targets: AnyOf | Targets): CoreTargets {
    return coreTargets(targets);
  }

  private rule(constraint: Constraint): ImportRule {
    return new ImportRule({ name: this.name, subjects: subjects(this.selection), constraint });
  }

  private custom(constraint: CustomConstraint, names: ListNames, find: Finder["find"]): ImportRule {
    return new ImportRule({ name: this.name, subjects: subjects(this.selection), constraint }, { names, find });
  }
}

/**
 * The lists of patterns that a check of the user needs. The rule matches them, and the
 * {@link Scope} of the check gives the result by the name of each list.
 *
 * ```ts
 * rule("the database is imported with its types only")
 *   .files(anyOf("src/**"))
 *   .mustSatisfy(
 *     (scope) =>
 *       scope.imports
 *         .filter((seen) => seen.subject && seen.hits["db"] !== undefined && seen.reference.kind !== "import-type")
 *         .map((seen) => scope.violationAt(seen.fact, "imports a value of the database")),
 *     { targets: { db: anyOf("src/db/**") } },
 *   )
 *   .check(app);
 * ```
 */
export interface CheckPatterns {
  /**
   * Lists of targets, as in `mustNotImport`. Each import in the scope has the first of its
   * paths that a list matches in {@link SeenImport.hits}, by the name of the list.
   */
  readonly targets?: Readonly<Record<string, AnyOf | Targets>>;
  /**
   * Lists of file patterns, which match as the files of a rule do. {@link Scope.matched}
   * gives the files that a list matches, by the name of the list.
   */
  readonly files?: Readonly<Record<string, AnyOf<string>>>;
}

type CoreTargets = { anyOf: RawPattern[]; unverified: RawPattern[] };

function coreTargets(targets: AnyOf | Targets): CoreTargets {
  const set = targets instanceof AnyOf ? new Targets(targets.patterns, []) : targets;
  // Without the types, a selection of files gets here.
  if (!(set instanceof Targets)) {
    throw new TypeError("refpact: the targets of a rule are anyOf(..), not a selection of files as in files(..)");
  }
  return { anyOf: set.anyOf.map(targetPattern), unverified: set.unverified.map(targetPattern) };
}

/** The custom check that the core gets for these lists, and the names of the lists in their order. */
function customCheck(patterns: CheckPatterns | undefined): { constraint: CustomConstraint; names: ListNames } {
  const targets = Object.entries(patterns?.targets ?? {});
  const files = Object.entries(patterns?.files ?? {});
  return {
    constraint: {
      type: "custom",
      targets: targets.map(([, list]) => coreTargets(list)),
      units: files.map(([, list]) => list.patterns.map(filePattern)),
    },
    names: { targets: targets.map(([name]) => name), files: files.map(([name]) => name) },
  };
}

/** The check of a rule with a custom check: what finds the violations in what the rule sees. @internal */
export interface Finder {
  readonly names: ListNames;
  readonly find: (scope: Scope, project: Project) => readonly Violation[];
}

/** The rules about `package.json` look at the imports of packages: the hit of an import is its package. */
const ANY_PACKAGE: CustomConstraint = { type: "custom", targets: [{ anyOf: [], unverified: [[packageSegment("*")]] }] };
const PACKAGE_NAMES: ListNames = { targets: ["package"], files: [] };

type PackageCheck = "declared" | "not-development";

/** The imports of packages by the selected files, as violations of a rule about uses. */
function packageImports(scope: Scope): Violation[] {
  const found: Violation[] = [];
  for (const seen of scope.imports) {
    const hit = seen.hits["package"];
    const { reference } = seen;
    // What a package.json names is no import.
    if (!seen.subject || hit === undefined || DEPENDENCY_KINDS.includes(reference.kind as DependencyKind)) {
      continue;
    }
    const written = hit.length !== reference.target.length || hit.some((segment, index) => segment !== reference.target[index]);
    found.push(
      scope.violationAt(seen.fact, {
        type: "uses",
        target: hit,
        ...(written ? { written: reference.target } : {}),
        kind: reference.kind,
        certainty: reference.certainty,
      }),
    );
  }
  return found;
}

/** What a `package.json` of the project declares, from the facts of the project. */
interface Declared {
  /** The directory of the file, from the root. */
  readonly directory: string;
  name: string | undefined;
  /** What `dependencies`, `peerDependencies` and `optionalDependencies` name. */
  readonly production: Set<string>;
  /** What `devDependencies` name. */
  readonly development: Set<string>;
}

/**
 * From the imports of packages by the selected files, those that break a rule about
 * `package.json`. The rule reads what a check of the user can read: the references and the
 * items that the `package.json` files give to a project that is loaded with
 * `readDependencies`. Throws an {@link ArchitectureError} when no `package.json` can make
 * the rule fail.
 */
function undeclared(scope: Scope, check: PackageCheck): Violation[] {
  const rule = scope.ruleName;
  const malformed = (problem: string): ArchitectureError =>
    new ArchitectureError({ type: "malformed", rule, message: `rule \`${rule}\` cannot be checked:\n  ${problem}` });
  const { facts } = scope;
  if (!facts.kinds.includes("dependency")) {
    throw malformed("the project is loaded without `readDependencies`, so it has no package.json");
  }
  const manifests = new Map<string, Declared>();
  const manifestAt = (unit: readonly string[]): Declared => {
    const file = unit.join("/");
    const known = manifests.get(file);
    if (known !== undefined) {
      return known;
    }
    const made: Declared = { directory: unit.slice(0, -1).join("/"), name: undefined, production: new Set(), development: new Set() };
    manifests.set(file, made);
    return made;
  };
  for (const declaration of facts.declarations ?? []) {
    if (declaration.kind === PACKAGE_ITEM_KIND) {
      manifestAt(declaration.unit).name = declaration.name;
    }
  }
  for (const fact of facts.facts) {
    if (fact.what.type === "reference" && NAMED_DEPENDENCY_KINDS.includes(fact.what.kind)) {
      const name = (fact.what.target[0] as string).slice(packageSegment("").length);
      const manifest = manifestAt(fact.unit);
      (fact.what.kind === "dev-dependency" ? manifest.development : manifest.production).add(name);
    }
  }
  // The package.json of a file is the nearest one above it.
  const nearest = new Map<string, Declared | undefined>();
  const manifestOf = (file: string): Declared => {
    if (!nearest.has(file)) {
      let found: Declared | undefined;
      for (const manifest of manifests.values()) {
        const inside = !path.posix.relative(manifest.directory, file).startsWith("..");
        if (inside && (found === undefined || path.posix.resolve(manifest.directory).length > path.posix.resolve(found.directory).length)) {
          found = manifest;
        }
      }
      nearest.set(file, found);
    }
    const manifest = nearest.get(file);
    if (manifest === undefined) {
      throw malformed(`no package.json is above ${file}`);
    }
    return manifest;
  };
  if (check === "not-development" && ![...manifests.values()].some((manifest) => manifest.development.size > 0)) {
    throw malformed("no package.json of the project has devDependencies");
  }
  return packageImports(scope).filter((violation) => {
    const detail = violation.detail;
    if (detail.type !== "uses") {
      return false;
    }
    const manifest = manifestOf(violation.file);
    const name = (detail.target[0] as string).slice(packageSegment("").length);
    const declares = (entry: string): boolean => manifest.production.has(entry) || manifest.development.has(entry);
    const types = typesPackage(name);
    // A type import of `name` can come from `@types/name`.
    const entry = declares(name) ? name : TYPE_KINDS.includes(detail.kind) && declares(types) ? types : undefined;
    if (check === "declared") {
      return entry === undefined && name !== manifest.name;
    }
    return entry !== undefined && !manifest.production.has(entry);
  });
}

const TYPE_KINDS: readonly string[] = ["import-type", "export-type-from"];

/** `@types/name` for `name`, and `@types/scope__name` for `@scope/name`. */
function typesPackage(name: string): string {
  return `@types/${name.startsWith("@") ? name.slice(1).replace("/", "__") : name}`;
}

/**
 * Why a rule did not pass.
 *
 * - `violated`: the code breaks the rule.
 * - `invalid`: the rule cannot fail as written, for example because a pattern matches nothing.
 * - `stale-baseline`: the baseline accepts violations that the code no longer has.
 * - `baseline`: the baseline file cannot be read or written.
 * - `stale-diagram`: the diagram file does not hold the diagram of the rule.
 * - `diagram`: the diagram file cannot be read or written, or its name gives no format.
 * - `malformed`: a rule or a group of rules cannot fail as written, for example layers with one layer only.
 * - `several`: several rules of one group did not pass.
 */
export type Failure =
  | {
      readonly type: "violated";
      /** The name of the rule. */
      readonly rule: string;
      /** Why the rule exists, when it says so. See {@link ImportRule.because}. */
      readonly reason?: string;
      /** The violations that no baseline accepts. */
      readonly violations: readonly Violation[];
      /** The report. */
      readonly message: string;
    }
  | {
      readonly type: "invalid";
      /** The name of the rule. */
      readonly rule: string;
      /** Every reason, in the order of the specification. */
      readonly errors: readonly RuleError[];
      /** The report. */
      readonly message: string;
    }
  | {
      readonly type: "stale-baseline";
      /** The name of the rule. */
      readonly rule: string;
      /** The baseline file. */
      readonly file: string;
      /** The entries that accept too much. */
      readonly entries: readonly StaleEntry[];
      /** The report. */
      readonly message: string;
    }
  | {
      readonly type: "baseline";
      /** What went wrong. */
      readonly message: string;
    }
  | {
      readonly type: "stale-diagram";
      /** The name of the rule. */
      readonly rule: string;
      /** The diagram file. */
      readonly file: string;
      /** The report. */
      readonly message: string;
    }
  | {
      readonly type: "diagram";
      /** What went wrong. */
      readonly message: string;
    }
  | {
      readonly type: "malformed";
      /** The name of the rule or of the group. */
      readonly rule: string;
      /** What is wrong with it. */
      readonly message: string;
    }
  | {
      readonly type: "several";
      /** The failures of the rules, in the order of the rules. */
      readonly failures: readonly Failure[];
      /** The reports of all failures. */
      readonly message: string;
    };

/** An answer of the core without errors. Throws the errors of the rule. */
function answered<T extends { readonly ok: boolean }>(rule: string, answer: T | { readonly ok: false; readonly errors: readonly RuleError[] }): Extract<T, { ok: true }> {
  if (!answer.ok) {
    const { errors } = answer as { readonly errors: readonly RuleError[] };
    throw new ArchitectureError({ type: "invalid", rule, errors, message: renderErrors(rule, errors) });
  }
  return answer as Extract<T, { ok: true }>;
}

/** What `make` gives, or the failure of the {@link ArchitectureError} that it throws. @internal */
export function orFailure<T>(make: () => T): T | Failure {
  try {
    return make();
  } catch (error) {
    if (error instanceof ArchitectureError) {
      return error.failure;
    }
    throw error;
  }
}

/** Throws an {@link ArchitectureError} when a rule failed. @internal */
export function passes(failure: Failure | undefined): void {
  if (failure !== undefined) {
    throw new ArchitectureError(failure);
  }
}

/** The error that `check` throws. Its message is the report. */
export class ArchitectureError extends Error {
  /** Why the rule did not pass. */
  readonly failure: Failure;

  constructor(failure: Failure) {
    super(failure.message);
    this.name = "ArchitectureError";
    this.failure = failure;
  }
}

/**
 * A complete rule: a plain value that can be checked against a project. Its filters are
 * those of {@link Filters}, such as `because`, `asWarning` and `ignoringKinds`.
 */
export interface ImportRule extends Filters<ImportRule> {}

export class ImportRule {
  private readonly rule: CoreRule;
  /** The check of a rule with a custom check. */
  private readonly finder: Finder | undefined;

  /** @internal */
  constructor(rule: CoreRule, finder?: Finder) {
    this.rule = rule;
    this.finder = finder;
  }

  /** The name of the rule. */
  get name(): string {
    return this.rule.name;
  }

  /** @internal The rule with one more filter. */
  filtered(filter: Filter): ImportRule {
    return new ImportRule(filter.apply(this.rule), this.finder);
  }

  /**
   * Names the properties of imports that a check of your own reads, such as `license`. A
   * name that the load does not produce is then an error, as a kind that it does not produce
   * is: a check that reads `licence` for `license`, or the properties of a load without
   * `readDependencies`, cannot pass because it finds nothing.
   */
  readingProperties(...names: string[]): ImportRule {
    return new ImportRule({ ...this.rule, properties: [...(this.rule.properties ?? []), ...names] }, this.finder);
  }

  /**
   * Evaluates the rule and returns the violations, which may be none, or the errors of the
   * rule. A rule about `package.json` throws an {@link ArchitectureError} when no
   * `package.json` can make it fail.
   */
  evaluate(project: Project): EvaluateResult {
    if (this.finder === undefined) {
      return evaluate(this.rule, project.facts);
    }
    // The core applies the subjects, the filters and the exemptions; the check does the rest.
    const { rule, finder } = this;
    const selection = select(rule, project.facts);
    if (!selection.ok) {
      return selection;
    }
    const scope = new (rule.constraint.type === "items" ? ItemScope : Scope)(rule.name, project.facts, selection, finder.names, {
      graph: (subjects) => answered(rule.name, graph(rule, project.facts, subjects)),
      tangles: (subjects) => answered(rule.name, tangles(rule, project.facts, subjects)).tangles,
    });
    return { ok: true, violations: finder.find(scope, project) };
  }

  /**
   * Evaluates the rule and returns the failure, or `undefined` when the rule passes. The
   * violations of a warning are reported with `console.warn` and are no failure: see
   * {@link ImportRule.asWarning}.
   */
  verify(project: Project): Failure | undefined {
    const found = this.found(project);
    if (!Array.isArray(found)) {
      return found as Failure;
    }
    return this.violated(found, project);
  }

  /** The violations, or the reason why the rule cannot be evaluated. */
  private found(project: Project): readonly Violation[] | Failure {
    const rule = this.rule.name;
    const result = orFailure(() => this.evaluate(project));
    if ("type" in result) {
      return result;
    }
    if (!result.ok) {
      return { type: "invalid", rule, errors: result.errors, message: renderErrors(rule, result.errors) };
    }
    return result.violations;
  }

  /**
   * A diagram of what the rule sees, in Mermaid (the default), DOT or Markdown. The nodes are the files
   * of the rule, or its slices when the rule is about slices, such as
   * {@link ScopedRule.mustBeAcyclicBetween}. An edge is an import between two nodes that the
   * filters and the exemptions of the rule let pass. The files and imports of a cycle are
   * red. The constraint of the rule is not evaluated, so a rule of any kind draws its files.
   * Throws an {@link ArchitectureError} when the rule cannot be evaluated.
   *
   * A Markdown viewer that knows Mermaid, such as GitHub's, draws the text in a block
   * ` ```mermaid `.
   *
   * ```ts
   * const text = rule("the features").files(all()).mustBeAcyclicBetween(anyOf("src/features/*")).diagram(app);
   * ```
   */
  diagram(project: Project, format: DiagramFormat = "mermaid"): string {
    const { constraint } = this.rule;
    const slices = constraint.type === "must-be-acyclic" || constraint.type === "must-be-independent" ? constraint.slices : undefined;
    return this.draw(project, slices === undefined ? undefined : { slices }, format);
  }

  /** Like {@link ImportRule.checkDiagram}, but returns the failure, or `undefined` when the file is current. */
  verifyDiagram(project: Project, file: string): Failure | undefined {
    return verifyDiagram(this.rule.name, file, (format) => orFailure(() => this.diagram(project, format)));
  }

  /**
   * Throws an {@link ArchitectureError} when the file does not hold the
   * {@link ImportRule.diagram | diagram} of the rule, so a diagram that is committed with the
   * code stays current.
   *
   * The name of the file gives the format: `.md` is a Markdown file with the Mermaid diagram
   * in a block ` ```mermaid `, `.mmd` and `.mermaid` are Mermaid, `.dot` and `.gv` are DOT.
   * The diagram is the whole file. When the environment variable `REFPACT_DIAGRAM` is
   * `update`, this writes the diagram to the file and passes.
   *
   * A relative path is relative to the working directory; in a test, give a path from
   * `import.meta.dirname`.
   */
  checkDiagram(project: Project, file: string): void {
    passes(this.verifyDiagram(project, file));
  }

  /** @internal The diagram of the rule with these nodes. */
  draw(project: Project, nodes: DiagramNodes, format: DiagramFormat): string {
    const { name } = this.rule;
    const result = diagram(this.rule, project.facts, nodes, format);
    if (!result.ok) {
      throw new ArchitectureError({ type: "invalid", rule: name, errors: result.errors, message: renderErrors(name, result.errors) });
    }
    return result.text;
  }

  /** Throws an {@link ArchitectureError} when the rule is violated or cannot be evaluated. */
  check(project: Project): void {
    passes(this.verify(project));
  }

  /**
   * Like {@link ImportRule.verify}, but violations that the baseline file accepts do not fail.
   *
   * A baseline is used only through this method and {@link ImportRule.checkWithBaseline}. It
   * can only shrink: an entry that accepts more violations than the code has is a failure.
   * When the environment variable `REFPACT_BASELINE` is `update`, this writes the violations
   * of the rule to the file and passes.
   */
  verifyWithBaseline(project: Project, baseline: Baseline): Failure | undefined {
    if (updateRequested("REFPACT_BASELINE")) {
      return this.writeBaseline(project, baseline);
    }
    const found = this.violationsForBaseline(project);
    if (!Array.isArray(found)) {
      return found as Failure;
    }
    const entries = baseline.read();
    if (!Array.isArray(entries)) {
      return entries as Failure;
    }
    const rule = this.rule.name;
    const applied = applyBaseline(rule, found, entries, project.facts);
    const violated = this.violated(applied.remaining, project);
    if (violated !== undefined) {
      return violated;
    }
    if (applied.stale.length > 0) {
      const message = renderStale(rule, baseline.file, applied.stale);
      return { type: "stale-baseline", rule, file: baseline.file, entries: applied.stale, message };
    }
    return undefined;
  }

  /**
   * Throws an {@link ArchitectureError} when the rule has violations that the baseline file
   * does not accept, when the baseline has stale entries, or when the rule cannot be evaluated.
   */
  checkWithBaseline(project: Project, baseline: Baseline): void {
    passes(this.verifyWithBaseline(project, baseline));
  }

  /**
   * Writes the current violations of this rule to the baseline file. Entries of other rules
   * in the file are kept. Returns the failure, or `undefined` when the file is written.
   */
  writeBaseline(project: Project, baseline: Baseline): Failure | undefined {
    const found = this.violationsForBaseline(project);
    if (!Array.isArray(found)) {
      return found as Failure;
    }
    return baseline.write(this.rule.name, found, project.facts);
  }

  /** The failure of the violations. A warning reports them and fails not. */
  private violated(violations: readonly Violation[], project: Project): Failure | undefined {
    if (violations.length === 0) {
      return undefined;
    }
    const { name: rule, reason, severity } = this.rule;
    if (severity === "warning") {
      console.warn(renderViolations(rule, violations, project.facts, reason, severity));
      return undefined;
    }
    const message = renderViolations(rule, violations, project.facts, reason);
    return { type: "violated", rule, ...(reason !== undefined ? { reason } : {}), violations, message };
  }

  private violationsForBaseline(project: Project): readonly Violation[] | Failure {
    const rule = this.rule.name;
    if (/[\t\r\n]/.test(rule)) {
      const message = `refpact: the name of rule ${JSON.stringify(rule)} has a tab or a line break, so it cannot be in a baseline`;
      return { type: "baseline", message };
    }
    return this.found(project);
  }

  /** The rule as the core sees it. */
  toCore(): CoreRule {
    return this.rule;
  }
}

addFilters(ImportRule);

export { KINDS, MARKERS };
export type { Kind, Marker };
