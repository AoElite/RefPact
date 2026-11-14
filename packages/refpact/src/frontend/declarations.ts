/** The named items that a file declares at its top level, for the rules about items. */
import { SyntaxKind } from "./compiler.ts";
import type { Node, SourceFile } from "./compiler.ts";

/** A kind of item: `function`, `class`, `interface`, `type`, `enum`, `variable` or `namespace`. */
export type ItemKind = "function" | "class" | "interface" | "type" | "enum" | "variable" | "namespace" | "package";

/** The kinds of item a declaration can have. */
export const ITEM_KINDS: readonly ItemKind[] = ["function", "class", "interface", "type", "enum", "variable", "namespace"];

/** `exported`: the module exports the item, also under another name. `local`: it does not. */
export type Visibility = "exported" | "local";

/** The visibilities a declaration can have. */
export const VISIBILITIES: readonly Visibility[] = ["exported", "local"];

/**
 * The relations that the syntax of a declaration gives: `extends` and `implements` of a class
 * or an interface, the decorators of a class, and the types that a function takes and
 * returns and that a variable has.
 */
export const TIES = ["extends", "implements", "decorator", "returns", "takes", "has-type"] as const;

/**
 * What a declaration says of its item besides its kind and its visibility: `async`,
 * `abstract`, `declare`, `const` (a `const` variable or a `const enum`), `default` (an
 * `export default` function or class), and `documented` (a JSDoc comment before it).
 */
export type Modifier = "async" | "abstract" | "declare" | "const" | "default" | "documented";

/** The modifiers a declaration can have, in the order of their description. */
export const MODIFIERS: readonly Modifier[] = ["async", "abstract", "declare", "const", "default", "documented"];

/** All relations of a declaration: the ties, and the types that it extends or implements at any depth. */
export const RELATIONS = [...TIES, "assignable-to"] as const;

/** One tie of a declaration, by the node that names its target. */
export interface Tie {
  readonly relation: (typeof TIES)[number];
  readonly node: Node;
}

/** A top-level declaration of a file, before its names are resolved. */
export interface Declared {
  readonly name: string;
  readonly kind: ItemKind;
  /** The node of the name, or of the declaration when it has no name. */
  readonly at: Node;
  /** The node of the whole declaration: the item ends where it ends. */
  readonly whole: Node;
  /** True when the declaration has the `export` modifier. */
  readonly exported: boolean;
  readonly ties: readonly Tie[];
  readonly modifiers: readonly Modifier[];
}

// The fields of the few kinds of node that are read here.
interface Loose extends Node {
  readonly text?: string;
  readonly escapedText?: string;
  readonly name?: Loose;
  readonly modifiers?: readonly Loose[];
  readonly heritageClauses?: readonly Loose[];
  readonly token?: number;
  readonly types?: readonly Loose[];
  readonly expression?: Loose;
  readonly typeName?: Loose;
  readonly declarationList?: Loose;
  readonly declarations?: readonly Loose[];
  readonly parameters?: readonly Loose[];
  readonly type?: Loose;
  readonly initializer?: Loose;
  readonly exprName?: Loose;
  readonly qualifier?: Loose;
}

const K = SyntaxKind;

const KINDS: ReadonlyMap<number, ItemKind> = new Map<number, ItemKind>([
  [K.FunctionDeclaration, "function"],
  [K.ClassDeclaration, "class"],
  [K.InterfaceDeclaration, "interface"],
  [K.TypeAliasDeclaration, "type"],
  [K.EnumDeclaration, "enum"],
  [K.ModuleDeclaration, "namespace"],
]);

function identifier(node: Loose | undefined): string | undefined {
  return node !== undefined && node.kind === K.Identifier ? (node.text ?? node.escapedText) : undefined;
}

/** The node that names what a decorator or a heritage type refers to: `a` in `@a()`, `a.B` in `extends a.B`. */
function named(node: Loose | undefined): Node | undefined {
  let current = node;
  while (current !== undefined && current.kind === K.CallExpression) {
    current = current.expression;
  }
  return current;
}

/**
 * The nodes that name a type in a type, at any depth: `Order` and `Error` in
 * `Result<Order, Error>`, `x` in `typeof x`, and `Order` in `import("./order").Order`.
 */
function typeNames(type: Loose | undefined, found: Node[]): void {
  if (type === undefined) {
    return;
  }
  const name = type.kind === K.TypeReference ? type.typeName : type.kind === K.TypeQuery ? type.exprName : type.kind === K.ImportType ? type.qualifier : undefined;
  if (name !== undefined) {
    found.push(name);
  }
  type.forEachChild((child) => typeNames(child as Loose, found));
}

/** The ties of a signature: the types of its parameters, without `this`, and its return type. */
function signatureTies(signature: Loose, ties: Tie[]): void {
  for (const parameter of signature.parameters ?? []) {
    if (identifier(parameter.name) === "this") {
      continue;
    }
    const names: Node[] = [];
    typeNames(parameter.type, names);
    ties.push(...names.map((node) => ({ relation: "takes" as const, node })));
  }
  const names: Node[] = [];
  typeNames(signature.type, names);
  ties.push(...names.map((node) => ({ relation: "returns" as const, node })));
}

const KEYWORDS: ReadonlyMap<number, Modifier> = new Map<number, Modifier>([
  [K.AsyncKeyword, "async"],
  [K.AbstractKeyword, "abstract"],
  [K.DeclareKeyword, "declare"],
  [K.ConstKeyword, "const"],
  [K.DefaultKeyword, "default"],
]);

/**
 * The modifiers of a statement, from its keywords and from the comments before it. `more`
 * are modifiers that the caller found elsewhere, such as the `async` of an arrow function.
 */
function modifiersOf(statement: Loose, sourceFile: SourceFile, more: readonly Modifier[]): Modifier[] {
  const found = new Set<Modifier>(more);
  for (const modifier of statement.modifiers ?? []) {
    const known = KEYWORDS.get(modifier.kind);
    if (known !== undefined) {
      found.add(known);
    }
  }
  // The text between the end of the node before and the first token of this one holds its
  // comments.
  if (sourceFile.text.slice(statement.pos, statement.getStart(sourceFile)).includes("/**")) {
    found.add("documented");
  }
  return MODIFIERS.filter((modifier) => found.has(modifier));
}

/** The top-level declarations of a file, in the order of the text. */
export function collectDeclarations(sourceFile: SourceFile): Declared[] {
  const found: Declared[] = [];
  sourceFile.forEachChild((child) => {
    const node = child as Loose;
    const modifiers = node.modifiers ?? [];
    const exported = modifiers.some((modifier) => modifier.kind === K.ExportKeyword);
    if (node.kind === K.VariableStatement) {
      for (const declaration of node.declarationList?.declarations ?? []) {
        const name = identifier(declaration.name);
        if (name !== undefined && declaration.name !== undefined) {
          const ties: Tie[] = [];
          const names: Node[] = [];
          typeNames(declaration.type, names);
          ties.push(...names.map((node) => ({ relation: "has-type" as const, node })));
          // `const make = (order: Order): Total => ..` takes and returns as a function does.
          const value = declaration.initializer;
          const more: Modifier[] = [];
          if (value !== undefined && (value.kind === K.ArrowFunction || value.kind === K.FunctionExpression)) {
            signatureTies(value, ties);
            if ((value.modifiers ?? []).some((modifier) => modifier.kind === K.AsyncKeyword)) {
              more.push("async");
            }
          }
          const list = node.declarationList;
          if (list !== undefined && /^const\b/.test(sourceFile.text.slice(list.getStart(sourceFile)))) {
            more.push("const");
          }
          found.push({ name, kind: "variable", at: declaration.name, whole: declaration, exported, ties, modifiers: modifiersOf(node, sourceFile, more) });
        }
      }
      return;
    }
    const kind = KINDS.get(node.kind);
    if (kind === undefined) {
      return;
    }
    const name = identifier(node.name);
    const isDefault = modifiers.some((modifier) => modifier.kind === K.DefaultKeyword);
    if (name === undefined && !(isDefault && kind === "class") && !(isDefault && kind === "function")) {
      return;
    }
    const ties: Tie[] = [];
    for (const modifier of modifiers) {
      const target = modifier.kind === K.Decorator ? named(modifier.expression) : undefined;
      if (target !== undefined) {
        ties.push({ relation: "decorator", node: target });
      }
    }
    for (const clause of node.heritageClauses ?? []) {
      const relation = clause.token === K.ImplementsKeyword ? "implements" : clause.token === K.ExtendsKeyword ? "extends" : undefined;
      for (const type of clause.types ?? []) {
        // TypeScript 7.0 gives each type of a heritage clause its `expression`. TypeScript 7.1
        // gives a type that is only a type, as in `implements` and in the `extends` of an
        // interface, as a type reference, which names its target in `typeName`.
        const target = named(type.expression ?? type.typeName);
        if (relation !== undefined && target !== undefined) {
          ties.push({ relation, node: target });
        }
      }
    }
    if (kind === "function") {
      signatureTies(node, ties);
    }
    found.push({ name: name ?? "default", kind, at: node.name ?? node, whole: node, exported, ties, modifiers: modifiersOf(node, sourceFile, []) });
  });
  return found;
}
