/**
 * What the front end needs from a TypeScript compiler.
 *
 * The installed `typescript` package is the compiler, so a project is read by the same
 * compiler that builds it. `native.ts` talks to the native compiler of TypeScript 7 through
 * the client that the package ships. `classic.ts` runs the compiler of TypeScript 6 in this
 * process.
 */
import { createRequire } from "node:module";

import type { ComponentFiles } from "./components.ts";
import { loadClassic } from "./classic.ts";
import { loadNative } from "./native.ts";

/** A node of a syntax tree. */
export interface Node {
  readonly kind: number;
  /** Where the node begins in the text of the file, with the comments before it. */
  readonly pos: number;
  /** The position after the last character of the node in the text of the file. */
  readonly end: number;
  forEachChild(visit: (child: Node) => void): void;
  getStart(sourceFile: SourceFile): number;
}

export interface SourceFile extends Node {
  /** The text of the file. The positions of nodes count in it. */
  readonly text: string;
  getLineAndCharacterOfPosition(position: number): { readonly line: number; readonly character: number };
}

/** A symbol of the compiler. Only its name is read directly; the project answers the rest. */
export interface TsSymbol {
  readonly name: string;
}

/** One opened tsconfig project. */
export interface CompilerProject {
  readonly configFile: string;
  readonly options: Readonly<Record<string, unknown>>;
  /** Absolute names of the files of the program, without default libraries and external packages. */
  readonly ownFiles: readonly string[];
  sourceFile(file: string): SourceFile | undefined;
  /**
   * Lets go of the syntax trees that `sourceFile` gave, so that their memory can be used
   * again. The trees that the caller still holds stay valid.
   */
  forgetSourceFiles(): void;
  /**
   * The symbols at many nodes: `undefined` where the compiler finds none. With the native
   * compiler this is one round trip.
   */
  symbolsAt(nodes: readonly Node[]): (TsSymbol | undefined)[];
  /** The symbol of the value of a shorthand property: `fetch` in `{ fetch }`. One round trip. */
  shorthandValueAt(property: Node): TsSymbol | undefined;
  /** Absolute names of the files that declare the symbol. */
  filesOf(symbol: TsSymbol): string[];
  /** True when the two are the same symbol. */
  same(a: TsSymbol, b: TsSymbol): boolean;
  /**
   * The class, the interface or the enum that declares a member: a property, a method, an
   * accessor or an enum member. `undefined` for other symbols, and for a member of a type
   * without a name.
   */
  ownerOf(symbol: TsSymbol): TsSymbol | undefined;
  /** True when the symbol is a type parameter, as `T` in `function first<T>(items: T[]): T`. */
  isTypeParameter(symbol: TsSymbol): boolean;
  /** One step from an alias to what it stands for. `undefined` when the symbol is not an alias. */
  immediateAlias(symbol: TsSymbol): TsSymbol | undefined;
  /**
   * All that a module exports, also through `export *`, by name. With the native compiler
   * this is one round trip for each module.
   */
  exportsOf(module: TsSymbol): ReadonlyMap<string, TsSymbol>;
}

export interface CompilerSession {
  readonly projects: readonly CompilerProject[];
  close(): void;
}

/** What a backend gives. */
export interface Backend {
  /** The numbers of the kinds of nodes. They differ between versions of the compiler. */
  readonly SyntaxKind: Readonly<Record<string, number>>;
  /**
   * Where the nodes of a JSDoc comment are. `true`: in the field `jsDoc` of the node after
   * the comment, as in TypeScript 6. `false`: among the children, as in TypeScript 7.
   */
  readonly jsDocBesideChildren: boolean;
  openProjects(root: string, configFiles: readonly string[], files?: ComponentFiles): CompilerSession;
}

/** The installed `typescript` package. */
export interface Installed {
  readonly packageName: string;
  readonly version: string;
  readonly exports: Readonly<Record<string, unknown>>;
}

// REFPACT_TYPESCRIPT names the package to load instead of `typescript`. It is a switch for
// tests and development only, to run the tests against a second compiler that is installed
// under an alias, for example `REFPACT_TYPESCRIPT=typescript-6`.
const packageName = process.env["REFPACT_TYPESCRIPT"] || "typescript";
const manifest = createRequire(import.meta.url)(`${packageName}/package.json`) as {
  version: string;
  exports?: Record<string, unknown>;
};
const installed: Installed = { packageName, version: manifest.version, exports: manifest.exports ?? {} };

/** The version of the compiler that reads the projects, such as `7.0.2`. */
export const compilerVersion: string = installed.version;

const major = Number(installed.version.split(".")[0]);
if (major !== 6 && major !== 7) {
  throw new Error(`refpact: TypeScript ${installed.version} is not supported. Install TypeScript 6 or 7.`);
}
const backend: Backend = major === 6 ? await loadClassic(installed) : await loadNative(installed);

// The kinds that the front end reads. A compiler that does not have one of them is not one
// that this code was written for, so that is an error here and not a silent miss later.
const KIND_NAMES = [
  "CallExpression",
  "CatchClause",
  "AsExpression",
  "BinaryExpression",
  "ConditionalExpression",
  "NewExpression",
  "NonNullExpression",
  "ParenthesizedExpression",
  "PlusToken",
  "SatisfiesExpression",
  "TemplateExpression",
  "ArrayLiteralExpression",
  "AbstractKeyword",
  "ArrayBindingPattern",
  "ArrowFunction",
  "AsyncKeyword",
  "Block",
  "BindingElement",
  "ClassDeclaration",
  "ConstKeyword",
  "Constructor",
  "DebuggerStatement",
  "DeclareKeyword",
  "Decorator",
  "DefaultKeyword",
  "EnumMember",
  "EnumDeclaration",
  "ExportKeyword",
  "ExtendsKeyword",
  "ForInStatement",
  "ForOfStatement",
  "ForStatement",
  "FunctionDeclaration",
  "FunctionExpression",
  "GetAccessor",
  "HeritageClause",
  "ImplementsKeyword",
  "InterfaceDeclaration",
  "ModuleDeclaration",
  "TypeAliasDeclaration",
  "VariableStatement",
  "ExportDeclaration",
  "ExternalModuleReference",
  "Identifier",
  "ImportClause",
  "ImportDeclaration",
  "ImportEqualsDeclaration",
  "ImportKeyword",
  "ImportSpecifier",
  "ImportType",
  "MethodDeclaration",
  "JsxAttribute",
  "JsxClosingElement",
  "JsxOpeningElement",
  "JsxSelfClosingElement",
  "MetaProperty",
  "NamedExports",
  "NamedImports",
  "NamespaceImport",
  "NoSubstitutionTemplateLiteral",
  "ObjectBindingPattern",
  "Parameter",
  "PropertyAssignment",
  "PropertyDeclaration",
  "PropertySignature",
  "PropertyAccessExpression",
  "QualifiedName",
  "SetAccessor",
  "ShorthandPropertyAssignment",
  "StringLiteral",
  "TypeKeyword",
  "TypeQuery",
  "TypeReference",
  "VariableDeclaration",
  "VariableDeclarationList",
] as const;

/** See {@link Backend.jsDocBesideChildren}. */
export const jsDocBesideChildren: boolean = backend.jsDocBesideChildren;

export type SyntaxKind = number;
export const SyntaxKind = Object.fromEntries(
  KIND_NAMES.map((name) => {
    const value = backend.SyntaxKind[name];
    if (typeof value !== "number") {
      throw new Error(`refpact: TypeScript ${installed.version} has no SyntaxKind.${name}`);
    }
    return [name, value];
  }),
) as Readonly<Record<(typeof KIND_NAMES)[number], number>>;

/** Opens the given tsconfig files with the installed compiler. */
export function openProjects(root: string, configFiles: readonly string[], files?: ComponentFiles): CompilerSession {
  return backend.openProjects(root, configFiles, files);
}
