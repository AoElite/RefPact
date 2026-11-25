/**
 * The filters of a rule: what it leaves out, why it exists and how much it weighs.
 *
 * A rule, layers and an onion architecture have the same filters. Each filter is written
 * once, here: {@link addFilters} gives its method to all of them, so a new filter cannot be
 * missing from one.
 */
import type { KindFilter, Rule as CoreRule } from "./core/index.ts";
import type { Kind } from "./frontend/imports.ts";
import { subjects, targetPattern } from "./patterns.ts";
import type { AnyOf, Files } from "./rules.ts";

/** One filter, as a value that a set of rules keeps until its rules are made. @internal */
export interface Filter {
  /**
   * Whether the filter says which imports a rule sees. A rule that looks at no import, such
   * as "every file is in a layer", does not get such a filter: there, an exemption that lets
   * nothing pass would be an error.
   */
  readonly aboutImports: boolean;
  /** The rule of the core with the filter. */
  readonly apply: (rule: CoreRule) => CoreRule;
}

/** What has filters: it gives itself again with one more filter. @internal */
export interface Filtered<Self> {
  /** @internal */
  filtered(filter: Filter): Self;
}

/** The filters of a rule, of layers and of an onion architecture. */
export interface Filters<Self> {
  /**
   * Says why this exists. The report of a violation prints the reason below the name of the
   * rule, so whoever breaks the rule learns what it protects.
   */
  because(reason: string): Self;

  /**
   * Makes this a warning: the violations are reported with `console.warn`, and the check
   * passes. A rule that cannot fail as written is still an error, and so is a stale
   * baseline. {@link ImportRule.evaluate} gives the violations of a warning as of any rule.
   *
   * ```ts
   * rule("new code does not print").files(all()).mustNotContain("console").asWarning().check(app);
   * ```
   */
  asWarning(): Self;

  /** Counts only these kinds of import. With another filter of kinds, both hold: `onlyKinds` twice counts the kinds that both name. */
  onlyKinds(...kinds: Kind[]): Self;

  /** Counts all kinds of import except these, also with another filter of kinds, such as `ignoringTypeImports()`. */
  ignoringKinds(...kinds: Kind[]): Self;

  /**
   * Leaves out the imports that the analysis could not prove: the files that a computed
   * specifier such as `` import(`./locales/${name}.ts`) `` can name.
   */
  ignoringCandidates(): Self;

  /**
   * Leaves out `import type`, type-only names, `import("m")` in types, and `export type .. from`.
   * A filter of kinds before or after it holds too.
   */
  ignoringTypeImports(): Self;

  /**
   * The imports from these files of these targets are not seen: one known import that is
   * allowed. The files are a selection as in `files(..)`: `anyOf(..)`, `all()`,
   * `allExcept(..)` or `anyOf(..).except(..)`. The targets are as in `mustNotImport`. For a
   * cycle or a chain, the import is no edge.
   *
   * An exemption that lets no import pass is an error, so it goes when the code that it
   * allows goes. Call this again for more exemptions.
   */
  ignoringImport(from: AnyOf<string> | Files, to: AnyOf): Self;
}

const about = (apply: Filter["apply"]): Filter => ({ aboutImports: true, apply });

/**
 * The kinds that a rule counts when it also counts only `kinds`. Each filter of a rule
 * holds: a later one does not take the place of an earlier one.
 */
function only(rule: CoreRule, kinds: readonly string[]): CoreRule {
  const known: KindFilter = rule.kinds ?? { type: "all" };
  const left = known.type === "all" ? kinds : kinds.filter((kind) => known.kinds.includes(kind) === (known.type === "only"));
  return { ...rule, kinds: { type: "only", kinds: left } };
}

/** The kinds that a rule counts when it also leaves out `kinds`. */
function except(rule: CoreRule, kinds: readonly string[]): CoreRule {
  const known: KindFilter = rule.kinds ?? { type: "all" };
  if (known.type === "only") {
    return { ...rule, kinds: { type: "only", kinds: known.kinds.filter((kind) => !kinds.includes(kind)) } };
  }
  const ignored = known.type === "except" ? known.kinds : [];
  return { ...rule, kinds: { type: "except", kinds: [...ignored, ...kinds.filter((kind) => !ignored.includes(kind))] } };
}

/** The methods of {@link Filters}, written once for every class that has them. */
const methods: Filters<unknown> & ThisType<Filtered<unknown>> = {
  because(reason) {
    return this.filtered({ aboutImports: false, apply: (rule) => ({ ...rule, reason }) });
  },
  asWarning() {
    return this.filtered({ aboutImports: false, apply: (rule) => ({ ...rule, severity: "warning" }) });
  },
  onlyKinds(...kinds) {
    return this.filtered(about((rule) => only(rule, kinds)));
  },
  ignoringKinds(...kinds) {
    return this.filtered(about((rule) => except(rule, kinds)));
  },
  ignoringCandidates() {
    return this.filtered(about((rule) => ({ ...rule, candidates: "ignore" })));
  },
  ignoringTypeImports() {
    return this.filtered(about((rule) => except(rule, ["import-type", "export-type-from"])));
  },
  ignoringImport(from, to) {
    // Without the types, anything gets here: packages as the files, or a selection as the targets.
    const wrong = new TypeError("refpact: ignoringImport(..) takes a selection of files as in files(..), then anyOf(..) of targets");
    if (typeof from !== "object" || from === null) {
      throw wrong;
    }
    const files = "patterns" in from ? { include: from.patterns, excluded: undefined } : from;
    const lists: readonly unknown[] = [files.include ?? [], files.excluded ?? [], to.patterns];
    if (!lists.every(Array.isArray) || [files.include, files.excluded].some((list) => list?.some((pattern) => typeof pattern !== "string"))) {
      throw wrong;
    }
    const exemption = { from: subjects(files), to: to.patterns.map(targetPattern) };
    return this.filtered(about((rule) => ({ ...rule, exemptions: [...(rule.exemptions ?? []), exemption] })));
  },
};

/**
 * Gives a class the methods of {@link Filters}. The class says that it has them with
 * `interface Name extends Filters<Name> {}`.
 *
 * @internal
 */
export function addFilters(target: { readonly prototype: Filtered<unknown> }): void {
  Object.defineProperties(target.prototype, Object.getOwnPropertyDescriptors(methods));
  for (const name of Object.keys(methods)) {
    Object.defineProperty(target.prototype, name, { enumerable: false });
  }
}
