/**
 * The filters of a rule: what it leaves out, why it exists and how much it weighs.
 *
 * A rule, layers and an onion architecture have the same filters. Each filter is written
 * once, here: {@link addFilters} gives its method to all of them, so a new filter cannot be
 * missing from one.
 */
import type { Rule as CoreRule } from "./core/index.ts";
import type { Kind } from "./frontend/imports.ts";
import { filePattern, targetPattern } from "./patterns.ts";
import type { AnyOf } from "./rules.ts";

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

  /** Counts only these kinds of import. */
  onlyKinds(...kinds: Kind[]): Self;

  /** Counts all kinds of import except these. */
  ignoringKinds(...kinds: Kind[]): Self;

  /**
   * Leaves out the imports that the analysis could not prove: the files that a computed
   * specifier such as `` import(`./locales/${name}.ts`) `` can name.
   */
  ignoringCandidates(): Self;

  /** Leaves out `import type`, type-only names, `import("m")` in types, and `export type .. from`. */
  ignoringTypeImports(): Self;

  /**
   * The imports from these files of these targets are not seen: one known import that is
   * allowed. Files match as the files of a rule do; targets as in `mustNotImport`. For a
   * cycle or a chain, the import is no edge.
   *
   * An exemption that lets no import pass is an error, so it goes when the code that it
   * allows goes. Call this again for more exemptions.
   */
  ignoringImport(from: AnyOf<string>, to: AnyOf): Self;
}

const about = (apply: Filter["apply"]): Filter => ({ aboutImports: true, apply });

/** The methods of {@link Filters}, written once for every class that has them. */
const methods: Filters<unknown> & ThisType<Filtered<unknown>> = {
  because(reason) {
    return this.filtered({ aboutImports: false, apply: (rule) => ({ ...rule, reason }) });
  },
  asWarning() {
    return this.filtered({ aboutImports: false, apply: (rule) => ({ ...rule, severity: "warning" }) });
  },
  onlyKinds(...kinds) {
    return this.filtered(about((rule) => ({ ...rule, kinds: { type: "only", kinds } })));
  },
  ignoringKinds(...kinds) {
    return this.filtered(about((rule) => ({ ...rule, kinds: { type: "except", kinds } })));
  },
  ignoringCandidates() {
    return this.filtered(about((rule) => ({ ...rule, candidates: "ignore" })));
  },
  ignoringTypeImports() {
    return this.filtered(about((rule) => ({ ...rule, kinds: { type: "except", kinds: ["import-type", "export-type-from"] } })));
  },
  ignoringImport(from, to) {
    const exemption = { from: from.patterns.map(filePattern), to: to.patterns.map(targetPattern) };
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
