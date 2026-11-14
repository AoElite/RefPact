/**
 * The compiler backend for TypeScript 7.
 *
 * TypeScript 7 has no stable API. It ships a client that starts the native compiler as a
 * child process and asks it for projects, syntax trees and symbols. TypeScript 7.0 has it
 * under `typescript/unstable/*`, TypeScript 7.1 under `typescript/*`. This file loads the
 * client that the installed package has, and hides the few differences.
 *
 * The types come from the 7.0 declarations. The 7.1 client is used through the small
 * `Api71` interfaces below, which nothing checks against the 7.1 declarations.
 */
import type { SyntaxKind as SyntaxKindEnum } from "typescript/unstable/ast";
import type { Node as NativeNode } from "typescript/unstable/ast";
import type { API, Checker, Program, Project, Snapshot, SymbolFlags, Symbol as NativeSymbol } from "typescript/unstable/sync";

import type { Backend, CompilerProject, CompilerSession, Installed, SourceFile } from "./compiler.ts";
import type { ComponentFiles } from "./components.ts";

/** The parts of the 7.1 client that are not as in 7.0. */
interface Api71 {
  createSnapshot(params: { openProjects: string[] }): Snapshot71;
}
interface Snapshot71 {
  getConfiguredProject(configFileName: string): Project71 | undefined;
  dispose(): void;
}
interface Project71 {
  readonly program: Program;
  readonly checker: Checker;
  readonly parsedCommandLine: { readonly options: object };
}

interface Opened {
  readonly program: Program;
  readonly checker: Checker;
  readonly options: object;
}

interface Client {
  readonly is71: boolean;
  readonly API: new (options: { cwd: string; fs?: object }) => API;
  /** In 7.1: the value that a callback of the file system gives to let the compiler read the disk. */
  readonly useOS: unknown;
  readonly SymbolFlags: typeof SymbolFlags;
}

/** Loads the client of the installed TypeScript 7. */
export async function loadNative(installed: Installed): Promise<Backend> {
  const is71 = "./sync" in installed.exports;
  const entry = is71 ? installed.packageName : `${installed.packageName}/unstable`;
  // The numbers of the kinds differ between 7.0 and 7.1, so the value must be the loaded one.
  const ast = (await import(`${entry}/ast`)) as { SyntaxKind: typeof SyntaxKindEnum };
  const sync = (await import(`${entry}/sync`)) as Omit<Client, "is71" | "useOS">;
  const useOS = is71 ? ((await import(`${entry}/fs`)) as { serverFS: { useOS: unknown } }).serverFS.useOS : undefined;
  const client: Client = { is71, API: sync.API, SymbolFlags: sync.SymbolFlags, useOS };
  return {
    SyntaxKind: ast.SyntaxKind as unknown as Readonly<Record<string, number>>,
    jsDocBesideChildren: false,
    openProjects: (root, configFiles, files) => openProjects(client, root, configFiles, files),
  };
}

/** Opens the given tsconfig files in one compiler process. */
function openProjects(client: Client, root: string, configFiles: readonly string[], files: ComponentFiles | undefined): CompilerSession {
  // With a file system, the compiler asks this process for each file that it reads.
  const api = new client.API(files !== undefined ? { cwd: root, fs: fileSystem(client, files) } : { cwd: root });
  let snapshot: { dispose(): void } | undefined;
  try {
    let project: (configFile: string) => Opened | undefined;
    if (client.is71) {
      const opened = (api as unknown as Api71).createSnapshot({ openProjects: [...configFiles] });
      snapshot = opened;
      project = (configFile) => {
        const found = opened.getConfiguredProject(configFile);
        return found && { program: found.program, checker: found.checker, options: found.parsedCommandLine.options };
      };
    } else {
      const opened: Snapshot = api.updateSnapshot({ openProjects: [...configFiles] });
      snapshot = opened;
      project = (configFile) => {
        const found: Project | undefined = opened.getProject(configFile);
        return found && { program: found.program, checker: found.checker, options: found.compilerOptions };
      };
    }
    const projects = configFiles.map((configFile) => {
      const found = project(configFile);
      if (found === undefined) {
        throw new Error(`refpact: the TypeScript compiler did not load ${configFile}`);
      }
      return wrap(client, configFile, found, () => api.clearSourceFileCache());
    });
    const opened = snapshot;
    return {
      projects,
      close() {
        opened.dispose();
        api.close();
      },
    };
  } catch (error) {
    snapshot?.dispose();
    api.close();
    throw error;
  }
}

/**
 * The callbacks of the file system in the form of the client. In 7.0 a callback that is
 * left out, or that gives `undefined`, lets the compiler read the disk. In 7.1 each callback
 * must be there, and a value of the client says the same.
 */
function fileSystem(client: Client, files: ComponentFiles): object {
  if (!client.is71) {
    return files;
  }
  const useOS = client.useOS;
  return {
    readFile: (fileName: string) => files.readFile(fileName) ?? useOS,
    fileExists: (fileName: string) => files.fileExists(fileName) ?? useOS,
    getAccessibleEntries(directoryName: string) {
      const entries = files.getAccessibleEntries(directoryName);
      return entries === undefined ? useOS : { ...entries, symlinks: undefined };
    },
    // A component file is shown as a plain file.
    stat: (fileName: string) => {
      const text = files.readFile(fileName);
      return text === undefined ? useOS : { mode: 0o100644, size: text.length, mtime: new Date(0) };
    },
    directoryExists: useOS,
    realpath: useOS,
    writeFile: useOS,
    removeFile: useOS,
  };
}

function wrap(client: Client, configFile: string, { program, checker, options }: Opened, forget: () => void): CompilerProject {
  // TypeScript 7 removed `baseUrl` (error TS5102) and does not resolve with it. A project
  // that sets it would be read with those imports seen as packages, and rules on the files
  // would pass without a check.
  if ((options as { baseUrl?: unknown }).baseUrl !== undefined) {
    throw new Error(
      `refpact: ${configFile} sets baseUrl, which TypeScript 7 removed. Use \`paths\` instead.`,
    );
  }
  const ownFiles = program.getSourceFileNames().filter((file) => {
    const metadata = program.getSourceFileMetadata(file);
    return metadata !== undefined && !metadata.isDefaultLibrary && !metadata.isFromExternalLibrary;
  });
  // Each answer of the compiler is a round trip to another process, so each is kept. Many
  // files import the same module, and many names lead through the same re-export.
  const aliases = new Map<NativeSymbol["id"], NativeSymbol | undefined>();
  const exports = new Map<NativeSymbol["id"], Map<string, NativeSymbol>>();
  const owners = new Map<NativeSymbol["id"], NativeSymbol | undefined>();
  const Flags = client.SymbolFlags;
  const memberFlags = Flags.Property | Flags.Method | Flags.GetAccessor | Flags.SetAccessor | Flags.EnumMember;
  const ownerFlags = Flags.Class | Flags.Interface | Flags.RegularEnum | Flags.ConstEnum;
  return {
    configFile,
    options: options as Readonly<Record<string, unknown>>,
    ownFiles,
    sourceFile: (file) => program.getSourceFile(file) as unknown as SourceFile | undefined,
    forgetSourceFiles: forget,
    symbolsAt: (nodes) => (nodes.length === 0 ? [] : checker.getSymbolAtLocation(nodes as readonly unknown[] as readonly NativeNode[])),
    shorthandValueAt: (property) => checker.getShorthandAssignmentValueSymbol(property as unknown as NativeNode),
    filesOf: (symbol) => (symbol as NativeSymbol).declarations.map((declaration) => declaration.path),
    same: (a, b) => (a as NativeSymbol).id === (b as NativeSymbol).id,
    ownerOf(given) {
      const symbol = given as NativeSymbol;
      if ((symbol.flags & memberFlags) === 0) {
        return undefined;
      }
      if (owners.has(symbol.id)) {
        return owners.get(symbol.id);
      }
      const parent = symbol.getParent();
      const found = parent !== undefined && (parent.flags & ownerFlags) !== 0 ? parent : undefined;
      owners.set(symbol.id, found);
      return found;
    },
    isTypeParameter: (symbol) => ((symbol as NativeSymbol).flags & Flags.TypeParameter) !== 0,
    immediateAlias(given) {
      const symbol = given as NativeSymbol;
      // The compiler stops with a panic when it is asked for the target of a symbol that
      // is not an alias, so the flag is checked first.
      if ((symbol.flags & client.SymbolFlags.Alias) === 0) {
        return undefined;
      }
      if (aliases.has(symbol.id)) {
        return aliases.get(symbol.id);
      }
      const target = checker.getImmediateAliasedSymbol(symbol);
      const found = target === undefined || checker.isUnknownSymbol(target) ? undefined : target;
      aliases.set(symbol.id, found);
      return found;
    },
    exportsOf(given) {
      const module = given as NativeSymbol;
      let found = exports.get(module.id);
      if (found === undefined) {
        found = new Map(checker.getExportsOfModule(module).map((symbol) => [symbol.name, symbol]));
        exports.set(module.id, found);
      }
      return found;
    },
  };
}
