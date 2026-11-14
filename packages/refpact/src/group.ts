/** Groups: several rules that are checked together and report every failure. */
import type { Baseline } from "./baseline.ts";
import type { Project } from "./frontend/project.ts";
import { ArchitectureError } from "./rules.ts";
import type { Failure, ImportRule } from "./rules.ts";

/**
 * Rules that are checked together, so that one check reports every rule that fails, not only
 * the first. {@link rules}, {@link layers} and {@link onionArchitecture} give sets of rules,
 * and a class of your own can be one: say which rules it stands for, and it is checked as
 * they are.
 *
 * ```ts
 * class NoDatabase extends RuleSet {
 *   readonly name = "no part imports the database";
 *   private readonly parts: readonly string[];
 *
 *   constructor(parts: readonly string[]) {
 *     super();
 *     this.parts = parts;
 *   }
 *
 *   rules(): ImportRule[] {
 *     return this.parts.map((part) =>
 *       rule(`${part} does not import the database`).files(anyOf(`src/${part}/**`)).mustNotImport(anyOf("src/db/**")),
 *     );
 *   }
 * }
 *
 * new NoDatabase(["ui", "domain"]).check(app);
 * ```
 */
export abstract class RuleSet {
  /** The name of the set. It names the failure of a set without rules. */
  abstract readonly name: string;

  /**
   * The rules of the set. Throw an {@link ArchitectureError} of type `malformed` when the
   * set cannot fail as written.
   */
  abstract rules(): ImportRule[];

  /**
   * Checks all rules and returns the failure, or `undefined` when all pass. The failure is
   * that of one rule, or `several` in the order of the rules. A set without rules cannot
   * fail, so it is `malformed`.
   */
  verify(project: Project): Failure | undefined {
    return this.all((made) => made.verify(project));
  }

  /** Throws an {@link ArchitectureError} when a rule is violated or cannot be evaluated. The report has every rule that fails. */
  check(project: Project): void {
    const failure = this.verify(project);
    if (failure !== undefined) {
      throw new ArchitectureError(failure);
    }
  }

  /** Like {@link RuleSet.verify}, with a baseline. See {@link ImportRule.verifyWithBaseline}. */
  verifyWithBaseline(project: Project, baseline: Baseline): Failure | undefined {
    return this.all((made) => made.verifyWithBaseline(project, baseline));
  }

  /** Like {@link RuleSet.check}, with a baseline. See {@link ImportRule.checkWithBaseline}. */
  checkWithBaseline(project: Project, baseline: Baseline): void {
    const failure = this.verifyWithBaseline(project, baseline);
    if (failure !== undefined) {
      throw new ArchitectureError(failure);
    }
  }

  private all(verify: (made: ImportRule) => Failure | undefined): Failure | undefined {
    let list: readonly ImportRule[];
    try {
      list = this.rules();
    } catch (error) {
      if (error instanceof ArchitectureError) {
        return error.failure;
      }
      throw error;
    }
    if (list.length === 0) {
      const message = `rule \`${this.name}\` cannot be checked:\n  the group has no rules, so it cannot fail`;
      return { type: "malformed", rule: this.name, message };
    }
    const failures = list.flatMap((made) => verify(made) ?? []);
    if (failures.length <= 1) {
      return failures[0];
    }
    return { type: "several", failures, message: failures.map((failure) => failure.message).join("\n") };
  }
}

/**
 * Rules that are checked together: one check reports every rule that fails, not only the
 * first.
 *
 * ```ts
 * rules(
 *   "the platform rules",
 *   rule("files are read in the platform only").files(allExcept("src/platform/**")).mustNotImport(anyOf(pkg("node:fs"))),
 *   rule("the platform forms no cycle").files(anyOf("src/platform/**")).mustBeAcyclic(),
 * ).check(app);
 * ```
 */
export class Rules extends RuleSet {
  readonly name: string;
  private readonly list: readonly ImportRule[];

  /** @internal */
  constructor(name: string, list: readonly ImportRule[]) {
    super();
    this.name = name;
    this.list = list;
  }

  /** The rules of the group. */
  rules(): ImportRule[] {
    return [...this.list];
  }
}

/** Groups rules, so that one check reports every rule that fails. */
export function rules(name: string, ...list: ImportRule[]): Rules {
  return new Rules(name, list);
}
