/**
 * The compiler backend for TypeScript 6.
 *
 * TypeScript 6 is the last version whose compiler is JavaScript. It runs in this process,
 * through the stable API of the `typescript` package, so no answer is a round trip and
 * nothing is kept between calls.
 *
 * The types come from the package `typescript-6`, an alias of TypeScript 6 that only the
 * development of refpact installs. The code that runs is the installed `typescript`.
 */
import type * as TS from "typescript-6";

import type { Backend, CompilerProject, CompilerSession, Installed, Node, SourceFile, TsSymbol } from "./compiler.ts";
import type { ComponentFiles } from "./components.ts";
import { COMPONENT_EXTENSIONS } from "./components.ts";

type Compiler = typeof TS;

/** Loads the compiler of the installed TypeScript 6. */
export async function loadClassic(installed: Installed): Promise<Backend> {
  const ts = ((await import(installed.packageName)) as { default: Compiler }).default;
  return {
    SyntaxKind: ts.SyntaxKind as unknown as Readonly<Record<string, number>>,
    jsDocBesideChildren: true,
    openProjects: (_root, configFiles, files) => ({
      projects: configFiles.map((configFile) => open(ts, configFile, files)),
      // The compiler is in this process, so there is nothing to stop.
      close() {},
    }),
  };
}

/** Opens one tsconfig file as a program. */
function open(ts: Compiler, configFile: string, files: ComponentFiles | undefined): CompilerProject {
  const system = files === undefined ? ts.sys : withComponents(ts.sys, files);
  const parsed = ts.getParsedCommandLineOfConfigFile(configFile, undefined, {
    ...system,
    onUnRecoverableConfigFileDiagnostic(diagnostic) {
      const message = ts.flattenDiagnosticMessageText(diagnostic.messageText, "\n");
      throw new Error(`refpact: the TypeScript compiler did not load ${configFile}: ${message}`);
    },
  });
  if (parsed === undefined) {
    throw new Error(`refpact: the TypeScript compiler did not load ${configFile}`);
  }
  const host = ts.createCompilerHost(parsed.options, true);
  host.readFile = system.readFile;
  host.fileExists = system.fileExists;
  // An import of a referenced project leads to its source, as with TypeScript 7, and not to
  // the declaration files of a build that may not exist.
  (host as { useSourceOfProjectReferenceRedirect?: () => boolean }).useSourceOfProjectReferenceRedirect = () => true;
  const program = ts.createProgram({
    rootNames: parsed.fileNames,
    options: parsed.options,
    host,
    ...(parsed.projectReferences === undefined ? {} : { projectReferences: parsed.projectReferences }),
  });
  const checker = program.getTypeChecker();
  const Flags = ts.SymbolFlags;
  const memberFlags = Flags.Property | Flags.Method | Flags.GetAccessor | Flags.SetAccessor | Flags.EnumMember;
  const ownerFlags = Flags.Class | Flags.Interface | Flags.RegularEnum | Flags.ConstEnum;
  const native = (symbol: TsSymbol): TS.Symbol => symbol as unknown as TS.Symbol;
  return {
    configFile,
    options: parsed.options as Readonly<Record<string, unknown>>,
    ownFiles: program
      .getSourceFiles()
      .filter((file) => !program.isSourceFileDefaultLibrary(file) && !program.isSourceFileFromExternalLibrary(file))
      .map((file) => file.fileName),
    sourceFile: (file) => program.getSourceFile(file) as unknown as SourceFile | undefined,
    // The program holds its syntax trees for as long as it lives.
    forgetSourceFiles() {},
    symbolsAt: (nodes) => nodes.map((node) => known(checker, checker.getSymbolAtLocation(node as unknown as TS.Node))),
    shorthandValueAt: (property: Node) => known(checker, checker.getShorthandAssignmentValueSymbol(property as unknown as TS.Node)),
    filesOf: (symbol) => (native(symbol).declarations ?? []).map((declaration) => declaration.getSourceFile().fileName),
    same: (a, b) => a === b,
    ownerOf(symbol) {
      if ((native(symbol).flags & memberFlags) === 0) {
        return undefined;
      }
      // A member is declared inside its owner, and the name of the owner has its symbol.
      const name = (native(symbol).declarations?.[0]?.parent as { name?: TS.Node } | undefined)?.name;
      const owner = name === undefined ? undefined : checker.getSymbolAtLocation(name);
      return owner !== undefined && (owner.flags & ownerFlags) !== 0 ? owner : undefined;
    },
    isTypeParameter: (symbol) => (native(symbol).flags & Flags.TypeParameter) !== 0,
    immediateAlias: (symbol) =>
      (native(symbol).flags & Flags.Alias) === 0 ? undefined : known(checker, checker.getImmediateAliasedSymbol(native(symbol))),
    exportsOf: (module) => new Map(checker.getExportsOfModule(native(module)).map((symbol) => [symbol.name, symbol])),
  };
}

/** The symbol, when the compiler found what it stands for. */
function known(checker: TS.TypeChecker, symbol: TS.Symbol | undefined): TS.Symbol | undefined {
  return symbol === undefined || checker.isUnknownSymbol(symbol) ? undefined : symbol;
}

/**
 * The file system of the compiler with the component files in it. A component file
 * `App.svelte` is there as `App.svelte.ts`, as `components.ts` describes.
 */
function withComponents(system: TS.System, files: ComponentFiles): TS.System {
  return {
    ...system,
    readFile: (fileName, encoding) => files.readFile(fileName) ?? system.readFile(fileName, encoding),
    fileExists: (fileName) => files.fileExists(fileName) ?? system.fileExists(fileName),
    // The files of a tsconfig. A pattern that names `.ts` files names the component files
    // too, under the names that they are shown with.
    readDirectory(directory, extensions, excludes, includes, depth) {
      const found = system.readDirectory(directory, extensions, excludes, includes, depth);
      const components = system
        .readDirectory(directory, COMPONENT_EXTENSIONS, excludes, includes?.map((pattern) => pattern.replace(/\.ts$/, "")), depth)
        .map((file) => `${file}.ts`)
        .filter((file) => files.fileExists(file) === true);
      return [...found, ...components];
    },
  };
}
