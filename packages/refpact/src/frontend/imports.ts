/** Finds every construct in one source file that refers to another module. */
import { checks } from "./checks.ts";
import { jsDocBesideChildren, SyntaxKind } from "./compiler.ts";
import type { Node, SourceFile } from "./compiler.ts";
import type { DependencyKind } from "./dependencies.ts";

/** The reference kinds of the TypeScript front end. */
export const KINDS = ["import", "import-type", "dynamic-import", "require", "export-from", "export-type-from", "resolve", "global"] as const;

/**
 * The kind of a place that asks where a module is and does not load it: `require.resolve("m")`,
 * `import.meta.resolve("m")` and `new URL("./m", import.meta.url)`.
 */
export const RESOLVE_KIND: Kind = "resolve";

/**
 * The kind of a use of a global of the host, such as `process.env` or `fetch(..)`: a name
 * that no file of the project declares and that the ECMAScript library does not declare.
 */
export const GLOBAL_KIND: Kind = "global";

/**
 * The kind of a use of a member of a class, an interface or an enum: `order.total`. Only a
 * project that is loaded with `resolveMembers` has this kind.
 */
export const MEMBER_KIND: Kind = "member";

/** One of the reference kinds of the TypeScript front end. */
export type Kind = (typeof KINDS)[number] | "member" | DependencyKind;

/** One name taken from a module: `a` in `import { a as b } from "m"`. */
export interface NameUse {
  /** The name as the module exports it. `default` for a default import. */
  readonly exported: string;
  readonly kind: Kind;
  /** The node that carries the position and the symbol. */
  readonly node: Node;
  /**
   * True for a name in the braces of an `import` or an `export ... from`. The symbol of such
   * a name is an alias of the export that the module has under `exported`.
   */
  readonly specifier?: true;
}

/** A local name that stands for a whole module: `ns` in `import * as ns from "m"`. */
export interface NamespaceBinding {
  readonly local: string;
  readonly node: Node;
  readonly kind: Kind;
}

/** One statement or expression that names a module. */
export interface ModuleUse {
  readonly specifier: string;
  readonly specifierNode: Node;
  /** The node whose position is reported for a use of the module as a whole. */
  readonly at: Node;
  readonly kind: Kind;
  /** True when the construct uses the module as a whole and not only some names of it. */
  readonly whole: boolean;
  readonly names: readonly NameUse[];
  readonly namespace?: NamespaceBinding;
  /** The `require` identifier of a `require("m")` call. It may be a local name, not Node's. */
  readonly requireNode?: Node;
  /**
   * How the code gives the specifier, when it is not one string in the place.
   * - `constant`: the code computes it from strings that are known, and `specifier` is the result.
   * - `pattern`: a part is not known. `specifier` has a `*` for each such part.
   * - `glob`: the argument of `import.meta.glob`, which is a glob pattern.
   * - `unknown`: nothing of it is known. `specifier` is empty.
   */
  readonly computed?: "constant" | "pattern" | "glob" | "unknown";
  /**
   * The names of constants that the specifier is computed from, each with the name of the
   * declaration that was read. The value holds only when the compiler says that both are
   * one variable.
   */
  readonly constants?: readonly { readonly use: Node; readonly declaration: Node }[];
  /** True for `new URL(..)`, which may name a file that is no module, such as an image. */
  readonly asset?: true;
}

/** `object.member` or `object.member` in a type, where `object` may be a namespace binding. */
export interface MemberUse {
  readonly object: Node;
  readonly member: Node;
  readonly objectName: string;
  readonly memberName: string;
  /**
   * For a member of a `const` that stands for a namespace binding, `const f = ns; f.name`: the
   * name of the declaration of the `const`, and the name of the binding in its value.
   * `object` is then the `const` where it is used, and `objectName` is the name of the binding.
   */
  readonly alias?: { readonly declaration: Node; readonly of: Node };
}

// The syntax tree of the compiler is typed with one interface for each kind of node. This
// file reads a few fields of a few kinds, so it uses one loose shape instead.
interface Loose extends Node {
  readonly text?: string;
  readonly escapedText?: string;
  readonly name?: Loose;
  readonly propertyName?: Loose;
  readonly isTypeOnly?: boolean;
  readonly phaseModifier?: SyntaxKind;
  readonly importClause?: Loose;
  readonly namedBindings?: Loose;
  readonly exportClause?: Loose;
  readonly elements?: readonly Loose[];
  readonly moduleSpecifier?: Loose;
  readonly moduleReference?: Loose;
  readonly expression?: Loose;
  readonly arguments?: readonly Loose[];
  readonly argument?: Loose;
  readonly literal?: Loose;
  readonly qualifier?: Loose;
  readonly left?: Loose;
  readonly right?: Loose;
  readonly initializer?: Loose;
  readonly token?: SyntaxKind;
  readonly tagName?: Loose;
  readonly jsDoc?: readonly Loose[];
  readonly head?: Loose;
  readonly templateSpans?: readonly Loose[];
  readonly operatorToken?: Loose;
  readonly whenTrue?: Loose;
  readonly whenFalse?: Loose;
}

const K = SyntaxKind;

function textOf(node: Loose | undefined): string {
  return node?.text ?? node?.escapedText ?? "";
}

function isStringLiteral(node: Loose | undefined): node is Loose {
  return node !== undefined && (node.kind === K.StringLiteral || node.kind === K.NoSubstitutionTemplateLiteral);
}

/**
 * The places in the text of a file where one of some words is written.
 *
 * A node that names a module has a known word in its text, such as `import`. So a node
 * whose text has no such word needs no visit, and neither do the nodes below it. That
 * matters with the native compiler, where each visited node is built from bytes that
 * another process sent, and most nodes of a file are in function bodies that name no module.
 */
class Mentions {
  /** Where the words begin, in ascending order. `undefined`: every node is visited. */
  private readonly starts: number[] | undefined;

  constructor(sourceFile: SourceFile, words: Iterable<string>, prune: boolean) {
    const text = sourceFile.text;
    // An identifier can be written with an escape, as in `r\u0065quire`. Such a file is
    // read in full.
    if (!prune || text.includes("\\u")) {
      return;
    }
    const starts: number[] = [];
    for (const word of words) {
      for (let at = text.indexOf(word); at !== -1; at = text.indexOf(word, at + 1)) {
        starts.push(at);
      }
    }
    this.starts = starts.sort((a, b) => a - b);
  }

  /** True when a word begins in the text of the node. */
  in(node: Node): boolean {
    const starts = this.starts;
    if (starts === undefined) {
      return true;
    }
    let low = 0;
    let high = starts.length;
    while (low < high) {
      const middle = (low + high) >> 1;
      if ((starts[middle] as number) < node.pos) {
        low = middle + 1;
      } else {
        high = middle;
      }
    }
    return low < starts.length && (starts[low] as number) < node.end;
  }
}

// Each construct that names a module has one of these words in its text.
const MODULE_WORDS = ["import", "export", "require"];

/**
 * Finds every construct of the file that names a module.
 *
 * `prune` lets the visit leave out nodes, see {@link Mentions}. It must be `false` for a
 * JavaScript file: the compiler makes nodes from the types in JSDoc comments, and such a node
 * is in the text before the node that it belongs to, not in it.
 */
export function collectModuleUses(sourceFile: SourceFile, prune: boolean): ModuleUse[] {
  const uses = moduleUses(sourceFile, prune);
  if (checks.walk && prune) {
    same(uses, moduleUses(sourceFile, false), (use) => use.specifierNode);
  }
  return uses;
}

/** With the test switch: the visit that leaves nodes out must find what the full visit finds. */
function same<T>(found: readonly T[], all: readonly T[], node: (use: T) => Node): void {
  if (found.length !== all.length || found.some((use, index) => node(use) !== node(all[index] as T))) {
    throw new Error("refpact: a visit that leaves out nodes without a known word does not find what a visit of all nodes finds");
  }
}

function moduleUses(sourceFile: SourceFile, prune: boolean): ModuleUse[] {
  const mentions = new Mentions(sourceFile, MODULE_WORDS, prune);
  const constants = new Constants(sourceFile);
  const uses: ModuleUse[] = [];
  const visit = (node: Loose, parent: Loose | undefined): void => {
    if (!mentions.in(node)) {
      return;
    }
    uses.push(...moduleUse(node, parent, constants));
    if (!prune && jsDocBesideChildren) {
      node.jsDoc?.forEach((comment) => visit(comment, node));
    }
    node.forEachChild((child) => visit(child as Loose, node));
  };
  sourceFile.forEachChild((child) => visit(child as Loose, undefined));
  return uses;
}

function moduleUse(node: Loose, parent: Loose | undefined, constants: Constants): ModuleUse[] {
  const one = (use: ModuleUse | undefined): ModuleUse[] => (use === undefined ? [] : [use]);
  switch (node.kind) {
    case K.ImportDeclaration:
      return one(importDeclaration(node));
    case K.ExportDeclaration:
      return one(exportDeclaration(node));
    case K.ImportEqualsDeclaration:
      return one(importEquals(node));
    case K.CallExpression:
      return call(node, parent, constants);
    case K.NewExpression:
      return newUrl(node, constants);
    case K.ImportType:
      return one(importType(node));
    default:
      return [];
  }
}

/** The part of a specifier that the code does not say. No specifier has this character. */
const HOLE = "\0";

/**
 * The `const` variables of a file that have a value, by name. A name that the file declares
 * more than once is not in it. The file is read in full, and only when a specifier needs it.
 */
class Constants {
  private readonly sourceFile: SourceFile;
  private found: Map<string, { readonly name: Loose; readonly value: Loose } | undefined> | undefined;

  constructor(sourceFile: SourceFile) {
    this.sourceFile = sourceFile;
  }

  /** The text of a node as the file writes it. */
  text(node: Node): string {
    return this.sourceFile.text.slice(node.getStart(this.sourceFile), node.end);
  }

  get(name: string): { readonly name: Loose; readonly value: Loose } | undefined {
    if (this.found === undefined) {
      const found = new Map<string, { readonly name: Loose; readonly value: Loose } | undefined>();
      const visit = (node: Loose, parent: Loose | undefined): void => {
        if (node.kind === K.VariableDeclaration && node.name?.kind === K.Identifier) {
          const name = textOf(node.name);
          const constant = parent?.kind === K.VariableDeclarationList && this.text(parent).startsWith("const") && node.initializer !== undefined;
          found.set(name, found.has(name) || !constant ? undefined : { name: node.name, value: node.initializer as Loose });
        }
        node.forEachChild((child) => visit(child as Loose, node));
      };
      this.sourceFile.forEachChild((child) => visit(child as Loose, undefined));
      this.found = found;
    }
    return this.found.get(name);
  }
}

/** What the code gives as a specifier: each string that it can be, with {@link HOLE} for a part that is not known. */
interface Specifiers {
  readonly values: string[];
  readonly constants: { readonly use: Node; readonly declaration: Node }[];
}

const MOST_SPECIFIERS = 16;

/**
 * The strings that an expression can be: a string, a template, strings joined with `+`, the
 * two sides of `? :`, and a `const` variable of the file with such a value.
 */
function specifiers(node: Loose | undefined, constants: Constants): Specifiers {
  const used: { use: Node; declaration: Node }[] = [];
  const join = (left: string[], right: string[]): string[] => left.flatMap((start) => right.map((end) => start + end)).slice(0, MOST_SPECIFIERS);
  const read = (node: Loose | undefined, depth: number): string[] => {
    if (node === undefined || depth > 8) {
      return [HOLE];
    }
    switch (node.kind) {
      case K.StringLiteral:
      case K.NoSubstitutionTemplateLiteral:
        return [textOf(node)];
      case K.ParenthesizedExpression:
      case K.AsExpression:
      case K.SatisfiesExpression:
      case K.NonNullExpression:
        return read(node.expression, depth + 1);
      case K.TemplateExpression: {
        let values = [textOf(node.head)];
        for (const span of node.templateSpans ?? []) {
          values = join(join(values, read(span.expression, depth + 1)), [textOf(span.literal)]);
        }
        return values;
      }
      case K.BinaryExpression:
        return node.operatorToken?.kind === K.PlusToken ? join(read(node.left, depth + 1), read(node.right, depth + 1)) : [HOLE];
      case K.ConditionalExpression:
        return [...read(node.whenTrue, depth + 1), ...read(node.whenFalse, depth + 1)].slice(0, MOST_SPECIFIERS);
      case K.Identifier: {
        const constant = constants.get(textOf(node));
        if (constant === undefined) {
          return [HOLE];
        }
        used.push({ use: node, declaration: constant.name });
        return read(constant.value, depth + 1);
      }
      default:
        return [HOLE];
    }
  };
  return { values: [...new Set(read(node, 0))], constants: used };
}

/** One module use for each string that a specifier can be. */
function computedUses(argument: Loose, constants: Constants, make: (specifier: string) => ModuleUse): ModuleUse[] {
  if (isStringLiteral(argument)) {
    return [make(textOf(argument))];
  }
  const found = specifiers(argument, constants);
  return found.values.map((value) => {
    // Parts that are not known and that follow each other are one part.
    const pattern = value.replace(/\0+/g, HOLE);
    const computed = pattern === HOLE ? "unknown" : pattern.includes(HOLE) ? "pattern" : "constant";
    const specifier = computed === "unknown" ? "" : pattern.replaceAll(HOLE, "*");
    // The place of a binding is the place of one module only.
    const { namespace: _namespace, names: _names, ...use } = make(specifier);
    return { ...use, whole: true, names: [], computed, constants: found.constants };
  });
}

function importDeclaration(node: Loose): ModuleUse | undefined {
  const specifierNode = node.moduleSpecifier;
  if (!isStringLiteral(specifierNode)) {
    return undefined;
  }
  const clause = node.importClause;
  const typeOnly = clause?.isTypeOnly === true || clause?.phaseModifier === K.TypeKeyword;
  const kind: Kind = typeOnly ? "import-type" : "import";
  const names: NameUse[] = [];
  let namespace: NamespaceBinding | undefined;
  if (clause?.name !== undefined) {
    names.push({ exported: "default", kind, node: clause.name });
  }
  const bindings = clause?.namedBindings;
  if (bindings?.kind === K.NamespaceImport && bindings.name !== undefined) {
    namespace = { local: textOf(bindings.name), node: bindings.name, kind };
  } else if (bindings?.kind === K.NamedImports) {
    for (const element of bindings.elements ?? []) {
      if (element.name !== undefined) {
        names.push({
          exported: textOf(element.propertyName ?? element.name),
          kind: typeOnly || element.isTypeOnly === true ? "import-type" : "import",
          node: element.name,
          specifier: true,
        });
      }
    }
  }
  return {
    specifier: textOf(specifierNode),
    specifierNode,
    at: namespace?.node ?? node,
    kind,
    // `import "m"`, `import {} from "m"` and `import * as ns from "m"` use the module as a whole.
    whole: names.length === 0 || namespace !== undefined,
    names,
    ...(namespace !== undefined ? { namespace } : {}),
  };
}

function exportDeclaration(node: Loose): ModuleUse | undefined {
  const specifierNode = node.moduleSpecifier;
  if (!isStringLiteral(specifierNode)) {
    return undefined;
  }
  const typeOnly = node.isTypeOnly === true;
  const kind: Kind = typeOnly ? "export-type-from" : "export-from";
  const names: NameUse[] = [];
  const clause = node.exportClause;
  if (clause?.kind === K.NamedExports) {
    for (const element of clause.elements ?? []) {
      if (element.name !== undefined) {
        names.push({
          exported: textOf(element.propertyName ?? element.name),
          kind: typeOnly || element.isTypeOnly === true ? "export-type-from" : "export-from",
          node: element.name,
          specifier: true,
        });
      }
    }
  }
  return {
    specifier: textOf(specifierNode),
    specifierNode,
    at: node,
    kind,
    // `export * from "m"`, `export * as ns from "m"` and `export {} from "m"` use the module as a whole.
    whole: names.length === 0,
    names,
  };
}

function importEquals(node: Loose): ModuleUse | undefined {
  const reference = node.moduleReference;
  if (reference?.kind !== K.ExternalModuleReference || !isStringLiteral(reference.expression) || node.name === undefined) {
    return undefined;
  }
  const kind: Kind = node.isTypeOnly === true ? "import-type" : "require";
  return {
    specifier: textOf(reference.expression),
    specifierNode: reference.expression,
    at: node,
    kind,
    whole: true,
    names: [],
    namespace: { local: textOf(node.name), node: node.name, kind },
  };
}

function call(node: Loose, parent: Loose | undefined, constants: Constants): ModuleUse[] {
  const argument = node.arguments?.[0];
  const callee = node.expression;
  if (argument === undefined || callee === undefined) {
    return [];
  }
  if (callee.kind === K.ImportKeyword) {
    return computedUses(argument, constants, (specifier) => ({ specifier, specifierNode: argument, at: node, kind: "dynamic-import", whole: true, names: [] }));
  }
  if (callee.kind === K.PropertyAccessExpression && callee.expression !== undefined) {
    const member = textOf(callee.name);
    const object = callee.expression;
    // `require.resolve("m")` and `import.meta.resolve("m")` ask where a module is.
    if (member === "resolve" && object.kind === K.Identifier && textOf(object) === "require") {
      return computedUses(argument, constants, (specifier) => ({ specifier, specifierNode: argument, at: node, kind: "resolve", whole: true, names: [], requireNode: object }));
    }
    if (object.kind === K.MetaProperty && constants.text(object) === "import.meta") {
      if (member === "resolve") {
        return computedUses(argument, constants, (specifier) => ({ specifier, specifierNode: argument, at: node, kind: "resolve", whole: true, names: [] }));
      }
      // `import.meta.glob("./pages/*.ts")` of Vite imports each file that the pattern matches.
      if (member === "glob") {
        const patterns = argument.kind === K.ArrayLiteralExpression ? (argument.elements ?? []) : [argument];
        return patterns
          .filter(isStringLiteral)
          .map((pattern) => ({ specifier: textOf(pattern), specifierNode: pattern, at: pattern, kind: "dynamic-import" as const, whole: true, names: [], computed: "glob" as const }));
      }
    }
    return [];
  }
  if (callee.kind !== K.Identifier || textOf(callee) !== "require" || node.arguments?.length !== 1) {
    return [];
  }
  // `const m = require("m")` binds the module; `const { a } = require("m")` takes names.
  const names: NameUse[] = [];
  let namespace: NamespaceBinding | undefined;
  const target = parent?.kind === K.VariableDeclaration && parent.initializer === node ? parent.name : undefined;
  if (target?.kind === K.Identifier) {
    namespace = { local: textOf(target), node: target, kind: "require" };
  } else if (target?.kind === K.ObjectBindingPattern) {
    for (const element of target.elements ?? []) {
      const exported = element.propertyName ?? element.name;
      if (exported?.kind === K.Identifier && element.name !== undefined) {
        names.push({ exported: textOf(exported), kind: "require", node: element.name });
      }
    }
  }
  return computedUses(argument, constants, (specifier) => ({
    specifier,
    specifierNode: argument,
    at: node,
    kind: "require",
    whole: names.length === 0,
    names,
    requireNode: callee,
    ...(namespace !== undefined ? { namespace } : {}),
  }));
}

/** `new URL("./worker.ts", import.meta.url)` names a file beside this one. */
function newUrl(node: Loose, constants: Constants): ModuleUse[] {
  const [argument, base] = node.arguments ?? [];
  const callee = node.expression;
  if (argument === undefined || base === undefined || callee?.kind !== K.Identifier || textOf(callee) !== "URL" || constants.text(base) !== "import.meta.url") {
    return [];
  }
  return computedUses(argument, constants, (specifier) => ({ specifier, specifierNode: argument, at: node, kind: "resolve", whole: true, names: [], asset: true }));
}

/** `import("m")` in a type, and `import("m").Name`. */
function importType(node: Loose): ModuleUse | undefined {
  const specifierNode = node.argument?.literal;
  if (!isStringLiteral(specifierNode)) {
    return undefined;
  }
  let first = node.qualifier;
  while (first?.kind === K.QualifiedName) {
    first = first.left;
  }
  const names: NameUse[] = first !== undefined ? [{ exported: textOf(first), kind: "import-type", node: first }] : [];
  return {
    specifier: textOf(specifierNode),
    specifierNode,
    at: node,
    kind: "import-type",
    whole: names.length === 0,
    names,
  };
}

/** Finds `object.member` where `object` is one of the given local names. */
export function collectMemberUses(sourceFile: SourceFile, locals: ReadonlySet<string>, prune: boolean): MemberUse[] {
  if (locals.size === 0) {
    return [];
  }
  const uses = memberUses(sourceFile, locals, prune);
  if (checks.walk && prune) {
    same(uses, memberUses(sourceFile, locals, false), (use) => use.member);
  }
  return uses;
}

function memberUses(sourceFile: SourceFile, locals: ReadonlySet<string>, prune: boolean): MemberUse[] {
  const mentions = new Mentions(sourceFile, locals, prune);
  const uses: MemberUse[] = [];
  // The `const` variables whose value is a binding, `const f = ns`, by their name. A name
  // that the file declares more than once this way is not followed.
  const aliases = new Map<string, { readonly declaration: Loose; readonly of: Loose } | undefined>();
  const isConst = (list: Loose | undefined): boolean =>
    list?.kind === K.VariableDeclarationList && sourceFile.text.startsWith("const", list.getStart(sourceFile));
  const visit = (node: Loose, parent: Loose | undefined): void => {
    if (!mentions.in(node)) {
      return;
    }
    const object =
      node.kind === K.PropertyAccessExpression ? node.expression : node.kind === K.QualifiedName ? node.left : undefined;
    const member = node.kind === K.PropertyAccessExpression ? node.name : node.kind === K.QualifiedName ? node.right : undefined;
    if (object?.kind === K.Identifier && member?.kind === K.Identifier && locals.has(textOf(object))) {
      uses.push({ object, member, objectName: textOf(object), memberName: textOf(member) });
    }
    const value = node.kind === K.VariableDeclaration ? node.initializer : undefined;
    if (value?.kind === K.Identifier && locals.has(textOf(value))) {
      if (node.name?.kind === K.ObjectBindingPattern) {
        // `const { a, b: c } = ns` takes `ns.a` and `ns.b`.
        for (const element of node.name.elements ?? []) {
          const taken = element.propertyName ?? element.name;
          if (taken?.kind === K.Identifier && element.name !== undefined && sourceFile.text.slice(element.getStart(sourceFile), element.end).slice(0, 3) !== "...") {
            uses.push({ object: value, member: taken, objectName: textOf(value), memberName: textOf(taken) });
          }
        }
      } else if (node.name?.kind === K.Identifier && isConst(parent)) {
        const name = textOf(node.name);
        aliases.set(name, aliases.has(name) ? undefined : { declaration: node.name, of: value });
      }
    }
    node.forEachChild((child) => visit(child as Loose, node));
  };
  sourceFile.forEachChild((child) => visit(child as Loose, undefined));

  // The members of the `const` variables, where the file names them.
  const known = new Map([...aliases].flatMap(([name, alias]) => (alias !== undefined && !locals.has(name) ? [[name, alias] as const] : [])));
  if (known.size > 0) {
    const named = new Mentions(sourceFile, known.keys(), prune);
    const follow = (node: Loose): void => {
      if (!named.in(node)) {
        return;
      }
      const alias = node.kind === K.PropertyAccessExpression && node.expression?.kind === K.Identifier ? known.get(textOf(node.expression)) : undefined;
      if (alias !== undefined && node.expression !== undefined && node.name?.kind === K.Identifier) {
        uses.push({ object: node.expression, member: node.name, objectName: textOf(alias.of), memberName: textOf(node.name), alias });
      }
      node.forEachChild((child) => follow(child as Loose));
    };
    sourceFile.forEachChild((child) => follow(child as Loose));
  }
  return uses;
}

/**
 * The names after the dot of each `object.name` of a file, in the order of the text. What a
 * name is a member of depends on the type of the object, which only the compiler knows.
 * Every node of the file is visited.
 */
export function collectAccesses(sourceFile: SourceFile): Node[] {
  const names: Node[] = [];
  const visit = (node: Loose): void => {
    if (node.kind === K.PropertyAccessExpression && node.name?.kind === K.Identifier) {
      names.push(node.name);
    }
    node.forEachChild((child) => visit(child as Loose));
  };
  sourceFile.forEachChild((child) => visit(child as Loose));
  return names;
}

/** A place where a file writes a name that it imports. */
export interface NameSite {
  readonly node: Node;
  /** The shorthand property that the name is, `{ query }`: the compiler is asked for its value apart. */
  readonly shorthand?: Node;
}

/**
 * The places where a file writes a name that one of its imports binds, outside the import
 * and export declarations, in the order of the text: `query` in `query(sql)` after
 * `import { query } from "./db"`. A name after a dot and the name of a member are left out.
 * The name of a namespace import is such a place where the code uses the module as a value,
 * `consume(db)`. In `db.query`, the member is the use, which {@link collectMemberUses} gives.
 * Only the compiler can tell whether a place is a use of the import or of another variable
 * with its name. Every node of the file is visited.
 */
export function collectNameSites(sourceFile: SourceFile, uses: readonly ModuleUse[]): NameSite[] {
  const bound = uses.flatMap((use) => use.names.filter((name) => !name.kind.startsWith("export")));
  const names = new Set(bound.map((name) => textOf(name.node as Loose)));
  const namespaces = new Set(uses.flatMap((use) => (use.namespace !== undefined ? [use.namespace.local] : [])));
  namespaces.forEach((name) => names.add(name));
  const sites: NameSite[] = [];
  if (names.size === 0) {
    return sites;
  }
  const visit = (node: Loose, parent: Loose): void => {
    if (node.kind === K.ImportDeclaration || node.kind === K.ImportEqualsDeclaration || node.kind === K.ExportDeclaration) {
      return;
    }
    const tag = parent.kind === K.JsxOpeningElement || parent.kind === K.JsxSelfClosingElement || parent.kind === K.JsxClosingElement;
    const ofMember = (parent.kind === K.PropertyAccessExpression && parent.expression === node) || (parent.kind === K.QualifiedName && parent.left === node);
    const taken = node.kind === K.Identifier && names.has(textOf(node)) && !(ofMember && namespaces.has(textOf(node)));
    if (taken && ((tag && parent.tagName === node) || isValueName(node, parent))) {
      sites.push({ node, ...(parent.kind === K.ShorthandPropertyAssignment && { shorthand: parent }) });
    }
    node.forEachChild((child) => visit(child as Loose, node));
  };
  sourceFile.forEachChild((child) => visit(child as Loose, sourceFile as Loose));
  return sites;
}

/**
 * A name in a file that may be a global: `fetch` in `fetch(..)`, or `process` and its member
 * `env` in `process.env`. Through `globalThis`, `window`, `self` and `global`, the global is
 * the name after the dot: `window.localStorage` is `localStorage`. Only the compiler can tell
 * whether the names are globals: `node` and `through` are the nodes to ask.
 */
export interface GlobalUse {
  /** The node that carries the position. */
  readonly at: Node;
  /** The node of the name of the global. */
  readonly node: Node;
  /** The name of the global. */
  readonly name: string;
  /** The member of the global that is used, if any: `env` in `process.env`. */
  readonly member?: string;
  /** `globalThis`, `window`, `self` or `global`, when the global is written as its member. */
  readonly through?: Node;
  /** The shorthand property that the name is, `{ fetch }`: the compiler is asked for its value apart. */
  readonly shorthand?: Node;
}

/** The names that stand for the global object. */
const GLOBAL_OBJECTS = new Set(["globalThis", "window", "self", "global"]);

/**
 * The names in value positions that may be globals, in the order of the text. Types,
 * the names after a dot, JSX tags and the callee of `require(..)` are left out.
 */
export function collectGlobalUses(sourceFile: SourceFile): GlobalUse[] {
  // A name that an enclosing scope declares, or that the language declares, is no global of
  // the host: the compiler need not be asked. A scope that is not modelled here only makes
  // the compiler be asked more.
  const scopes: Set<string>[] = [];
  const declared = (name: string): boolean => LANGUAGE_NAMES.has(name) || scopes.some((scope) => scope.has(name));
  // In each scope: the `const` variables whose value is a name that may be a global,
  // `const p = process`, with that name.
  const aliases: Map<string, Loose>[] = [];
  const aliasOf = (name: string): Loose | undefined => {
    for (let depth = scopes.length - 1; depth >= 0; depth--) {
      if (scopes[depth]?.has(name) === true) {
        return aliases[depth]?.get(name);
      }
    }
    return undefined;
  };
  const found: GlobalUse[] = [];
  const member = (access: Loose | undefined, object: Loose): Loose | undefined =>
    access?.kind === K.PropertyAccessExpression && access.expression === object && access.name?.kind === K.Identifier ? access.name : undefined;
  const visit = (node: Loose, parent: Loose | undefined, grand: Loose | undefined): void => {
    if (SKIPPED_NODES.has(node.kind) || isTypeHeritage(node, parent)) {
      return;
    }
    const value = node.kind === K.VariableDeclaration ? node.initializer : undefined;
    if (value?.kind === K.Identifier && node.name?.kind === K.Identifier && !declared(textOf(value))) {
      const constant = parent?.kind === K.VariableDeclarationList && sourceFile.text.startsWith("const", parent.getStart(sourceFile));
      if (constant) {
        aliases[aliases.length - 1]?.set(textOf(node.name), value);
      }
    }
    const through = node.kind === K.Identifier && parent !== undefined ? aliasOf(textOf(node)) : undefined;
    const taken = through !== undefined ? member(parent, node) : undefined;
    if (through !== undefined && taken !== undefined) {
      // `p.exit`, where `const p = process`, is a use of `process.exit`.
      found.push({ at: node, node: through, name: textOf(through), member: textOf(taken) });
    }
    const pattern = parent?.kind === K.VariableDeclaration && parent.initializer === node && parent.name?.kind === K.ObjectBindingPattern ? parent.name : undefined;
    const elements = (pattern?.elements ?? []).flatMap((element) => {
      const name = element.propertyName ?? element.name;
      const rest = sourceFile.text.startsWith("...", element.getStart(sourceFile));
      return name?.kind === K.Identifier && !rest ? [name] : [];
    });
    if (node.kind === K.Identifier && parent !== undefined && !declared(textOf(node)) && elements.length > 0 && !GLOBAL_OBJECTS.has(textOf(node))) {
      // `const { env } = process` takes `process.env`.
      for (const element of elements) {
        found.push({ at: element, node, name: textOf(node), member: textOf(element) });
      }
    } else if (node.kind === K.Identifier && parent !== undefined && !declared(textOf(node)) && isValueName(node, parent)) {
      const after = member(parent, node);
      if (GLOBAL_OBJECTS.has(textOf(node)) && after !== undefined) {
        const next = member(grand, parent);
        found.push({ at: node, node: after, name: textOf(after), through: node, ...(next !== undefined && { member: textOf(next) }) });
      } else {
        const shorthand = parent.kind === K.ShorthandPropertyAssignment ? { shorthand: parent } : {};
        found.push({ at: node, node, name: textOf(node), ...(after !== undefined && { member: textOf(after) }), ...shorthand });
      }
    }
    const scope = scopeNames(node);
    if (scope !== undefined) {
      scopes.push(scope);
      aliases.push(new Map());
    }
    node.forEachChild((child) => visit(child as Loose, node, parent));
    if (scope !== undefined) {
      scopes.pop();
      aliases.pop();
    }
  };
  scopes.push(statementNames(sourceFile as Loose));
  aliases.push(new Map());
  sourceFile.forEachChild((child) => visit(child as Loose, sourceFile as Loose, undefined));
  return found;
}

/** The kinds of functions, whose parameters are a scope. */
const FUNCTIONS = new Set<SyntaxKind>([
  K.FunctionDeclaration,
  K.FunctionExpression,
  K.ArrowFunction,
  K.MethodDeclaration,
  K.Constructor,
  K.GetAccessor,
  K.SetAccessor,
]);

/** The kinds of nodes whose statements or head declare names for the code inside them. */
const STATEMENT_SCOPES = new Set<SyntaxKind>([K.Block, K.ForStatement, K.ForOfStatement, K.ForInStatement, K.CatchClause]);

/**
 * The names that a node declares for the code inside it, if it is a scope: a block with its
 * declarations, a function with its parameters, a loop or a `catch` with its variable. A
 * `var` counts in its block only, which is narrower than the language has it.
 */
function scopeNames(node: Loose): Set<string> | undefined {
  if (STATEMENT_SCOPES.has(node.kind)) {
    return statementNames(node);
  }
  if (!FUNCTIONS.has(node.kind)) {
    return undefined;
  }
  const names = new Set<string>();
  if (node.kind === K.FunctionExpression && node.name !== undefined) {
    names.add(textOf(node.name));
  }
  node.forEachChild((child) => {
    const parameter = child as Loose;
    if (parameter.kind === K.Parameter && parameter.name !== undefined) {
      bindingNames(parameter.name, names);
    }
  });
  return names;
}

/** Adds the names that a binding declares: `a`, or those of `{ a, b: [c] }`. */
function bindingNames(node: Loose, names: Set<string>): void {
  if (node.kind === K.Identifier) {
    names.add(textOf(node));
  } else if (node.kind === K.ObjectBindingPattern || node.kind === K.ArrayBindingPattern) {
    node.forEachChild((element) => {
      const name = (element as Loose).name;
      if (name !== undefined) {
        bindingNames(name, names);
      }
    });
  }
}

/**
 * Some names of the ECMAScript library. Its names are never globals of the host, so these
 * need not be asked. The list is short on purpose: a name that is missing is asked.
 */
const LANGUAGE_NAMES = new Set([
  "undefined",
  "NaN",
  "Infinity",
  "Array",
  "Boolean",
  "Date",
  "Error",
  "JSON",
  "Map",
  "Math",
  "Number",
  "Object",
  "Promise",
  "Reflect",
  "RegExp",
  "Set",
  "String",
  "Symbol",
  "TypeError",
  "WeakMap",
  "WeakSet",
  "parseFloat",
  "parseInt",
]);

/**
 * The names that the statements of a file or a block, the head of a loop or a `catch`
 * declare: imports, variables, functions, classes and enums.
 */
function statementNames(container: Loose): Set<string> {
  const names = new Set<string>();
  const bindings = (node: Loose): void => bindingNames(node, names);
  const visit = (node: Loose): void => {
    switch (node.kind) {
      case K.VariableStatement:
      case K.VariableDeclarationList:
        node.forEachChild((child) => visit(child as Loose));
        break;
      case K.VariableDeclaration:
      case K.FunctionDeclaration:
      case K.ClassDeclaration:
      case K.EnumDeclaration:
      case K.ImportEqualsDeclaration:
        if (node.name !== undefined) {
          bindings(node.name);
        }
        break;
      case K.ImportDeclaration:
        node.forEachChild((child) => visit(child as Loose));
        break;
      case K.ImportClause:
        if (node.name !== undefined) {
          names.add(textOf(node.name));
        }
        node.forEachChild((child) => visit(child as Loose));
        break;
      case K.NamedImports:
        node.forEachChild((child) => visit(child as Loose));
        break;
      case K.ImportSpecifier:
      case K.NamespaceImport:
        names.add(textOf(node.name));
        break;
      default:
        break;
    }
  };
  container.forEachChild((child) => visit(child as Loose));
  return names;
}

/**
 * The kinds of nodes whose names are no uses of values: types, and the names of imports and
 * exports, which are those of other modules.
 */
const SKIPPED_NODES = new Set<SyntaxKind>([
  K.TypeReference,
  K.TypeQuery,
  K.QualifiedName,
  K.InterfaceDeclaration,
  K.TypeAliasDeclaration,
  K.ImportDeclaration,
  K.ImportEqualsDeclaration,
  K.ExportDeclaration,
]);

/** The `implements` clause of a class, which names types. */
function isTypeHeritage(node: Loose, parent: Loose | undefined): boolean {
  return node.kind === K.HeritageClause && (node.token === K.ImplementsKeyword || parent?.kind === K.InterfaceDeclaration);
}

/** Whether an identifier is a name that a value can stand behind, not a property or a tag. */
function isValueName(node: Loose, parent: Loose): boolean {
  switch (parent.kind) {
    case K.PropertyAccessExpression:
      return parent.name !== node;
    case K.JsxOpeningElement:
    case K.JsxSelfClosingElement:
    case K.JsxClosingElement:
      return parent.tagName !== node;
    case K.PropertyAssignment:
    case K.PropertyDeclaration:
    case K.PropertySignature:
    case K.MethodDeclaration:
    case K.GetAccessor:
    case K.SetAccessor:
    case K.EnumMember:
      return parent.name !== node;
    case K.BindingElement:
      return parent.propertyName !== node && parent.name !== node;
    case K.JsxAttribute:
    case K.MetaProperty:
      return false;
    case K.CallExpression:
      return !(parent.expression === node && textOf(node) === "require");
    default:
      return true;
  }
}

/**
 * The markers that a file can contain: a use of the global `console`, a `debugger`
 * statement, and `unresolved-import`, an import of a relative path, a `paths` alias or a
 * `#name` of `imports` that leads to no file.
 */
export const MARKERS = ["console", "debugger", "unresolved-import", "computed-import"] as const;

/** One of {@link MARKERS}. */
export type MarkerName = (typeof MARKERS)[number];

/**
 * A place that may be a marker. A `debugger` statement is one. `console.name` is one when
 * `console` is the global, which only the compiler can tell: `object` is the node to ask.
 */
export interface MarkerUse {
  readonly marker: MarkerName;
  readonly at: Node;
  readonly object?: Node;
}

/** The `debugger` statements of a file, and its `console.name`, in the order of the text. */
export function collectMarkers(sourceFile: SourceFile, prune: boolean): MarkerUse[] {
  const found = markers(sourceFile, prune);
  if (checks.walk && prune) {
    same(found, markers(sourceFile, false), (use) => use.at);
  }
  return found;
}

const MARKER_WORDS = ["console", "debugger"];

function markers(sourceFile: SourceFile, prune: boolean): MarkerUse[] {
  const mentions = new Mentions(sourceFile, MARKER_WORDS, prune);
  const found: MarkerUse[] = [];
  const visit = (node: Loose): void => {
    if (!mentions.in(node)) {
      return;
    }
    if (node.kind === K.DebuggerStatement) {
      found.push({ marker: "debugger", at: node });
    } else if (node.kind === K.PropertyAccessExpression && node.expression?.kind === K.Identifier && textOf(node.expression) === "console") {
      found.push({ marker: "console", at: node.expression, object: node.expression });
    }
    node.forEachChild((child) => visit(child as Loose));
  };
  sourceFile.forEachChild((child) => visit(child as Loose));
  return found;
}
