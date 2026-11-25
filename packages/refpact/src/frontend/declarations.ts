/** The named items that a file declares at its top level, and their members, for the rules about items. */
import { SyntaxKind } from "./compiler.ts";
import type { Node, SourceFile } from "./compiler.ts";

/**
 * A kind of item: `function`, `class`, `interface`, `type`, `enum`, `variable` or
 * `namespace`, and for a member of a class or of an interface `field` or `method`.
 */
export type ItemKind = "function" | "class" | "interface" | "type" | "enum" | "variable" | "namespace" | "field" | "method" | "package";

/** The kinds of the members of items. A rule selects them only when it names them. */
export const MEMBER_KINDS: readonly ItemKind[] = ["field", "method"];

/** The kinds of item a declaration can have. */
export const ITEM_KINDS: readonly ItemKind[] = ["function", "class", "interface", "type", "enum", "variable", "namespace", ...MEMBER_KINDS];

/**
 * For an item of a file, `exported`: the module exports the item, also under another name,
 * and `local`: it does not. For a member, `public`, `protected` or `private`, where a name
 * that starts with `#` is `private`.
 */
export type Visibility = "exported" | "local" | "public" | "protected" | "private";

/** The visibilities a declaration can have. */
export const VISIBILITIES: readonly Visibility[] = ["exported", "local", "public", "protected", "private"];

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
  /**
   * For `import("./port").Port` in a heritage clause: the specifier of the module, and the
   * name of the export. The compiler reads this form as a value, so the node gives no type.
   */
  readonly from?: { readonly specifier: Node; readonly name: string };
}

/** A top-level declaration of a file or one of its members, before its names are resolved. */
export interface Declared {
  readonly name: string;
  readonly kind: ItemKind;
  /** For a member: the name of the class or the interface that has it, and its visibility. */
  readonly member?: { readonly owner: string; readonly visibility: Visibility };
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
  readonly arguments?: readonly Loose[];
  readonly typeName?: Loose;
  readonly declarationList?: Loose;
  readonly declarations?: readonly Loose[];
  readonly parameters?: readonly Loose[];
  readonly type?: Loose;
  readonly initializer?: Loose;
  readonly exprName?: Loose;
  readonly qualifier?: Loose;
  readonly members?: readonly Loose[];
  readonly body?: Loose;
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

/** The members of a class or of an interface that are items, each with its kind. */
const MEMBERS: ReadonlyMap<number, ItemKind> = new Map<number, ItemKind>([
  [K.PropertyDeclaration, "field"],
  [K.PropertySignature, "field"],
  [K.MethodDeclaration, "method"],
  [K.MethodSignature, "method"],
  [K.Constructor, "method"],
  [K.GetAccessor, "method"],
  [K.SetAccessor, "method"],
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

/** What `import("./port").Port` names, when the node is this form. */
function inlineImport(node: Loose): Pick<Tie, "from"> {
  const call = node.kind === K.PropertyAccessExpression ? node.expression : undefined;
  const specifier = call?.kind === K.CallExpression && call.expression?.kind === K.ImportKeyword ? call.arguments?.[0] : undefined;
  const name = identifier(node.name);
  return specifier?.kind === K.StringLiteral && name !== undefined ? { from: { specifier, name } } : {};
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

/** The keywords that make a parameter of a constructor a field of the class. */
const PROPERTY_KEYWORDS: ReadonlySet<number> = new Set([K.PublicKeyword, K.PrivateKeyword, K.ProtectedKeyword, K.ReadonlyKeyword]);

/** The text of a name that is no identifier: a string, or a private name with its `#`. */
function textOf(node: Loose): string | undefined {
  return node.text ?? node.escapedText;
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

/** The decorators among the modifiers of a node, as ties. */
function decoratorTies(node: Loose, ties: Tie[]): void {
  for (const modifier of node.modifiers ?? []) {
    const target = modifier.kind === K.Decorator ? named(modifier.expression) : undefined;
    if (target !== undefined) {
      ties.push({ relation: "decorator", node: target });
    }
  }
}

/** The ties of the written type of a field or of a variable. */
function typeTies(type: Loose | undefined, ties: Tie[]): void {
  const names: Node[] = [];
  typeNames(type, names);
  ties.push(...names.map((node) => ({ relation: "has-type" as const, node })));
}

/** `private` and `protected` as the modifiers of a member or of a parameter say them, and `public` otherwise. */
function memberVisibility(node: Loose): Visibility {
  const has = (kind: number): boolean => (node.modifiers ?? []).some((modifier) => modifier.kind === kind);
  return has(K.PrivateKeyword) || node.name?.kind === K.PrivateIdentifier ? "private" : has(K.ProtectedKeyword) ? "protected" : "public";
}

/**
 * The fields and the methods of a class or of an interface with the name `owner`, in the
 * order of the text.
 *
 * - A property is a field, and so is a parameter of the constructor that declares one:
 *   `constructor(private readonly db: Db)`.
 * - A method, an accessor and the constructor, with the name `constructor`, are methods.
 *   The signatures of an overloaded method are one method: the one with the body. A getter
 *   and a setter of one name are two methods at one path, each with its own code.
 * - A member whose name the code computes, `[key]`, is no item.
 */
function collectMembers(owner: string, type: Loose, sourceFile: SourceFile, found: Declared[]): void {
  const places = new Map<string, number>();
  const add = (member: Loose, kind: ItemKind, name: string, at: Node, ties: Tie[]): void => {
    const item: Declared = { name, kind, at, whole: member, exported: false, ties, modifiers: modifiersOf(member, sourceFile, []), member: { owner, visibility: memberVisibility(member) } };
    const known = places.get(`${kind} ${name}`);
    if (known === undefined) {
      places.set(`${kind} ${name}`, found.push(item) - 1);
    } else if ((found[known]?.whole as Loose).body === undefined) {
      // The declaration before was a signature of an overloaded method: this one has its place.
      found[known] = item;
    } else {
      places.set(`${kind} ${name}`, found.push(item) - 1);
    }
  };
  for (const member of type.members ?? []) {
    const kind = MEMBERS.get(member.kind);
    const name = member.kind === K.Constructor ? "constructor" : member.name?.kind === K.StringLiteral || member.name?.kind === K.PrivateIdentifier ? textOf(member.name) : identifier(member.name);
    if (kind === undefined || name === undefined) {
      continue;
    }
    const ties: Tie[] = [];
    decoratorTies(member, ties);
    if (kind === "field") {
      typeTies(member.type, ties);
    } else {
      signatureTies(member, ties);
    }
    add(member, kind, name, member.name ?? member, ties);
    for (const parameter of member.kind === K.Constructor ? (member.parameters ?? []) : []) {
      const declares = (parameter.modifiers ?? []).some((modifier) => PROPERTY_KEYWORDS.has(modifier.kind));
      const field = identifier(parameter.name);
      if (declares && field !== undefined && parameter.name !== undefined) {
        const ties: Tie[] = [];
        typeTies(parameter.type, ties);
        add(parameter, "field", field, parameter.name, ties);
      }
    }
  }
}

/** The top-level declarations of a file, each class and interface before its members, in the order of the text. */
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
          typeTies(declaration.type, ties);
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
    decoratorTies(node, ties);
    for (const clause of node.heritageClauses ?? []) {
      const relation = clause.token === K.ImplementsKeyword ? "implements" : clause.token === K.ExtendsKeyword ? "extends" : undefined;
      for (const type of clause.types ?? []) {
        // TypeScript 7.0 gives each type of a heritage clause its `expression`. TypeScript 7.1
        // gives a type that is only a type, as in `implements` and in the `extends` of an
        // interface, as a type reference, which names its target in `typeName`.
        const target = named(type.expression ?? type.typeName);
        if (relation !== undefined && target !== undefined) {
          ties.push({ relation, node: target, ...inlineImport(target) });
        }
      }
    }
    if (kind === "function") {
      signatureTies(node, ties);
    }
    found.push({ name: name ?? "default", kind, at: node.name ?? node, whole: node, exported, ties, modifiers: modifiersOf(node, sourceFile, []) });
    if (kind === "class" || kind === "interface") {
      collectMembers(name ?? "default", node, sourceFile, found);
    }
  });
  return found;
}
