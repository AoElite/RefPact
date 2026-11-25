/** Rules about items: where they are, what they are named, what they extend or implement, and the types in their signatures. */
import type { CustomConstraint, Demand, RawPattern, Related, Rule as CoreRule, Violation } from "./core/index.ts";
import type { ItemScope, ListNames } from "./custom.ts";
import type { ItemKind, Modifier, Visibility } from "./frontend/declarations.ts";
import type { AnyOf, CheckPatterns, Files, Finder, ImportRule, Target, Targets } from "./rules.ts";

export type { ItemKind, Modifier, Visibility };

/** What an item selection needs from the rules, which import it. @internal */
export interface Maker {
  readonly rule: (rule: CoreRule, finder?: Finder) => ImportRule;
  readonly custom: (patterns: CheckPatterns | undefined) => { constraint: CustomConstraint; names: ListNames };
  readonly subjects: (files: Files) => CoreRule["subjects"];
  readonly file: (pattern: string) => RawPattern;
  readonly target: (target: Target) => RawPattern;
  readonly targets: (targets: AnyOf | Targets) => { anyOf: RawPattern[]; unverified: RawPattern[] };
}

interface Filters {
  readonly named?: readonly string[];
  readonly kinds?: readonly string[];
  readonly visibilities?: readonly string[];
  readonly modifiers?: readonly string[];
  readonly related: readonly Related[];
}

/**
 * Items of some files, narrowed by filters. Each filter narrows the selection; the values
 * that one filter takes form a union. End with a `must..` method.
 *
 * ```ts
 * rule("services live in the services folder")
 *   .items(all())
 *   .named("*Service")
 *   .ofKind("class")
 *   .mustResideIn(anyOf("src/services/**"))
 *   .check(app);
 * ```
 */
export class ItemSelection {
  private readonly name: string;
  private readonly files: Files;
  private readonly filters: Filters;
  private readonly maker: Maker;

  /** @internal */
  constructor(name: string, files: Files, maker: Maker, filters: Filters = { related: [] }) {
    this.name = name;
    this.files = files;
    this.maker = maker;
    this.filters = filters;
  }

  /** Only items whose name one of the globs matches. `*` matches any run of characters: `*Service`. */
  named(...globs: string[]): ItemSelection {
    return this.with({ named: [...(this.filters.named ?? []), ...globs] });
  }

  /**
   * Only items of these kinds. Without this filter, a selection holds every kind except
   * `field` and `method`: the members of the classes and the interfaces are selected only
   * when this filter names them.
   */
  ofKind(...kinds: ItemKind[]): ItemSelection {
    return this.with({ kinds: [...(this.filters.kinds ?? []), ...kinds] });
  }

  /**
   * Only items with one of these visibilities: `exported` or `local` for an item of a file,
   * and `public`, `protected` or `private` for a member.
   */
  withVisibility(...visibilities: Visibility[]): ItemSelection {
    return this.with({ visibilities: [...(this.filters.visibilities ?? []), ...visibilities] });
  }

  /**
   * Only items with one of these modifiers: `withModifier("async")` selects the `async`
   * functions, and the variables that hold an `async` arrow function or function expression.
   */
  withModifier(...modifiers: Modifier[]): ItemSelection {
    return this.with({ modifiers: [...(this.filters.modifiers ?? []), ...modifiers] });
  }

  /** Only classes and interfaces that extend one of the targets. A target that nothing extends is an error. */
  extending(targets: AnyOf): ItemSelection {
    return this.related("extends", targets);
  }

  /** Only classes that implement one of the targets. */
  implementing(targets: AnyOf): ItemSelection {
    return this.related("implements", targets);
  }

  /** Only classes with a decorator that is one of the targets. */
  decoratedWith(targets: AnyOf): ItemSelection {
    return this.related("decorator", targets);
  }

  /**
   * Only classes and interfaces that extend or implement one of the targets, directly or
   * through other types of the project: when `Order extends Entity` and `Entity implements
   * Saved`, `Order` is assignable to `Saved`. An item is not assignable to itself.
   */
  assignableTo(targets: AnyOf): ItemSelection {
    return this.related("assignable-to", targets);
  }

  /**
   * Only functions with one of the targets in their return type: `(): Promise<Order>` returns
   * the type `Order`. A variable that holds an arrow function or a function expression
   * counts as a function. Only the written type counts, not an inferred one.
   */
  returning(targets: AnyOf): ItemSelection {
    return this.related("returns", targets);
  }

  /** Only functions with one of the targets in the type of a parameter. `this` does not count. */
  taking(targets: AnyOf): ItemSelection {
    return this.related("takes", targets);
  }

  /** Only variables with one of the targets in their written type: `const pool: Pool = ..`. */
  ofType(targets: AnyOf): ItemSelection {
    return this.related("has-type", targets);
  }

  /** No item may be selected: "no exported item in `src/internal`". */
  mustNotExist(): ImportRule {
    return this.rule({ type: "not-exist" });
  }

  /** Each item must be declared in one of these files. */
  mustResideIn(files: AnyOf<string>): ImportRule {
    return this.rule({ type: "reside-in", places: files.patterns.map(this.maker.file) });
  }

  /** The name of each item must match one of the globs. */
  mustBeNamed(...globs: string[]): ImportRule {
    return this.rule({ type: "be-named", names: globs });
  }

  /** Each item must extend one of the targets. */
  mustExtend(targets: AnyOf): ImportRule {
    return this.rule({ type: "have", ...this.relate("extends", targets) });
  }

  /**
   * Each item must have at most this number of lines, from the line of its name to the line
   * on which its declaration ends. The decorators and the comments above the name do not
   * count.
   *
   * ```ts
   * rule("no function is longer than 60 lines").items(anyOf("src/**")).ofKind("function").mustNotExceedLines(60).check(app);
   * ```
   */
  mustNotExceedLines(lines: number): ImportRule {
    return this.rule({ type: "not-exceed-lines", lines });
  }

  /** Each item must implement one of the targets. */
  mustImplement(targets: AnyOf): ImportRule {
    return this.rule({ type: "have", ...this.relate("implements", targets) });
  }

  /** Each item must extend or implement one of the targets, directly or through other types. */
  mustBeAssignableTo(targets: AnyOf): ImportRule {
    return this.rule({ type: "have", ...this.relate("assignable-to", targets) });
  }

  /**
   * Each item must have one of these modifiers.
   *
   * ```ts
   * rule("the exports are documented").items(anyOf("src/**")).withVisibility("exported").mustHaveModifier("documented").check(app);
   * ```
   */
  mustHaveModifier(...modifiers: Modifier[]): ImportRule {
    return this.rule({ type: "have-modifier", modifiers });
  }

  /**
   * The code of the items must not import any of the targets: the imported names that the
   * code inside the selected items uses, in their signatures and their bodies, and for a
   * class in its members. Unlike a rule about files, a target in the file of the item can
   * break the rule.
   *
   * The project must be loaded with `resolveUses`, which says which item each use is in.
   *
   * ```ts
   * rule("the handlers do not use the database")
   *   .items(anyOf("src/api/**"))
   *   .named("*Handler")
   *   .mustNotImport(anyOf("src/db/**"))
   *   .check(app);
   * ```
   */
  mustNotImport(targets: AnyOf | Targets): ImportRule {
    return this.rule({ type: "not-use", targets: this.maker.targets(targets) });
  }

  /**
   * Every imported name that the code of the items uses must lead to one of the targets, or
   * be declared in the file of the item. See {@link ItemSelection.mustNotImport} for what
   * the code of an item is.
   */
  mustOnlyImport(targets: AnyOf | Targets): ImportRule {
    return this.rule({ type: "only-use", targets: this.maker.targets(targets) });
  }

  /** Each item must have a decorator that is one of the targets. */
  mustBeDecoratedWith(targets: AnyOf): ImportRule {
    return this.rule({ type: "have", ...this.relate("decorator", targets) });
  }

  /**
   * The items must pass a check of your own: a function that gets the {@link ItemScope} of
   * the rule and returns the violations. The scope has the items that the filters above
   * select, and all that the scope of a rule about files has. The check names the lists of
   * patterns that it needs: see {@link CheckPatterns}.
   *
   * ```ts
   * rule("a subclass is exported")
   *   .items(anyOf("src/**"))
   *   .ofKind("class")
   *   .mustSatisfy((scope) =>
   *     scope.items
   *       .filter((item) => item.visibility === "local" && item.relations.some((tie) => tie.relation === "extends"))
   *       .map((item) => scope.violationOfItem(item, `declares ${item.name}, which extends a class and is not exported`)),
   *   )
   *   .check(app);
   * ```
   */
  mustSatisfy(check: (scope: ItemScope) => readonly Violation[], patterns?: CheckPatterns): ImportRule {
    const { constraint, names } = this.maker.custom(patterns);
    return this.rule(constraint, { names, find: (scope) => check(scope as ItemScope) });
  }

  private related(relation: string, targets: AnyOf): ItemSelection {
    return this.with({ related: [...this.filters.related, this.relate(relation, targets)] });
  }

  private relate(relation: string, targets: AnyOf): Related {
    return { relation, anyOf: targets.patterns.map(this.maker.target) };
  }

  private with(change: Partial<Filters>): ItemSelection {
    return new ItemSelection(this.name, this.files, this.maker, { ...this.filters, ...change });
  }

  private rule(must: Demand, finder?: Finder): ImportRule {
    const { named, kinds, visibilities, modifiers, related } = this.filters;
    const rule: CoreRule = {
      name: this.name,
      subjects: this.maker.subjects(this.files),
      constraint: {
        type: "items",
        ...(named !== undefined && { named }),
        ...(kinds !== undefined && { kinds }),
        ...(visibilities !== undefined && { visibilities }),
        ...(modifiers !== undefined && { modifiers }),
        related,
        must,
      },
    };
    return this.maker.rule(rule, finder);
  }
}
