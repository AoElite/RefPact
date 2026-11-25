/** Loads TypeScript projects through the compiler and turns their imports into facts. */
import { existsSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from "node:fs";
import { isBuiltin } from "node:module";
import { tmpdir } from "node:os";
import path from "node:path";

import type { Declaration, Fact, FactSet, Path, Relation } from "../core/index.ts";
import { checks } from "./checks.ts";
import { COMPONENT_EXTENSIONS, componentFiles, componentOf } from "./components.ts";
import type { ComponentExtension } from "./components.ts";
import { openProjects } from "./compiler.ts";
import type { CompilerProject, Node, SourceFile, TsSymbol } from "./compiler.ts";
import { DEPENDENCY_KINDS, dependencyFacts, PACKAGE_ITEM_KIND, PACKAGE_PROPERTIES } from "./dependencies.ts";
import { collectDeclarations, ITEM_KINDS, MEMBER_KINDS, MODIFIERS, RELATIONS, VISIBILITIES } from "./declarations.ts";
import { collectAccesses, collectGlobalUses, collectMarkers, collectMemberUses, collectModuleUses, collectNameSites, GLOBAL_KIND, KINDS, MARKERS, MEMBER_KIND } from "./imports.ts";
import type { Kind, ModuleUse } from "./imports.ts";
import { ATTACHED_PREFIXES, exportSegment, globalSegment, HIDDEN_PREFIXES, nodeSegment, packageSegment } from "./paths.ts";

/** Options of {@link loadProject}. */
export interface LoadOptions {
  /**
   * The directory that file paths in rules are relative to. Files outside it are not units.
   * Default: the directory of the first tsconfig file.
   */
  readonly root?: string;
  /**
   * Resolves what `object.name` names when the object is a value: a member of a class, of an
   * interface or of an enum of the project. Each such use in another file is a reference of
   * the kind `member` to `file.ts#Owner#name`, so that `order.total()` is a use of
   * `src/domain/order.ts#Order#total`.
   *
   * The load then reads every function body and asks the compiler about each access, which
   * takes longer. Default: `false`.
   */
  readonly resolveMembers?: boolean;
  /**
   * Lets each use of an imported name count as an import, inside the item that holds it.
   * After `import { query } from "./db"`, each `query(..)` is a reference to
   * `src/db.ts#query`, of the kind of its import, and every reference says which item of
   * the file it is in. A rule about what the code of an item imports needs this:
   * `items(..).mustNotImport(..)` and `mustOnlyImport(..)`.
   *
   * A rule about files then reports the import and each use of its name, as a rule about
   * modules does in Rust. The load reads every function body and asks the compiler about
   * each place, which takes longer. Default: `false`.
   */
  readonly resolveUses?: boolean;
  /**
   * Lets a use of a member of an interface or of a class also count as a use of the same
   * member of each class of the project that implements or extends it. `store.save(order)`,
   * where `store` is a `Store`, is then also a **candidate** use of `PgStore#save`, because
   * the call can run it. A rule about what the code can reach, such as `mustNotReach`, then
   * follows the call into the classes.
   *
   * This turns on {@link LoadOptions.resolveMembers}. It is not the default, because it
   * turns around what an interface is for: with it, the code that calls the interface uses
   * the files that implement it. A rule leaves the candidates out with
   * `ignoringCandidates()`. Default: `false`.
   */
  readonly followImplementations?: boolean;
  /**
   * Reads the code of component files: `.svelte`, `.vue` and `.astro`. `true` reads all
   * three, and a list names the extensions to read.
   *
   * Each component file is a file of the project under its own name, `src/App.svelte`. Its
   * code is what its `<script>` blocks hold, and for Astro also the part between the `---`
   * lines at its start. The imports and the globals of that code are references as in a
   * TypeScript file, at their lines and columns in the component, and an import of
   * `./App.svelte` leads to the component.
   *
   * The markup is not read: a global or a member that only the markup uses is not seen. A
   * component that the markup uses is imported in the script, so no import is missed.
   *
   * The compiler then asks refpact for each file that it reads, which makes the load
   * slower. Default: `false`.
   */
  readonly components?: boolean | readonly ComponentExtension[];
  /**
   * Reads the dependencies that the `package.json` files name, and the packages that those
   * need.
   *
   * The code shows which packages a file imports. A `package.json` shows which packages are
   * installed, also those that no code imports. With this option, each `package.json` that a
   * file of the project belongs to is a file of the project under its path, `package.json`
   * or `packages/app/package.json`, and each dependency is a reference from it to the
   * package, at the line of its key:
   *
   * - `dependency`, `dev-dependency`, `peer-dependency` and `optional-dependency` for what
   *   the file names. A package that another `package.json` of the project declares is also
   *   a reference to that file, so the rules about files, such as `mustBeAcyclic`, see the
   *   dependencies between the packages of a monorepo.
   * - `transitive-dependency` for each other package that a dependency needs, with the
   *   packages on the way in the note of the violation, and `transitive-dev-dependency`
   *   when only the `devDependencies` need it. They are found in the `node_modules`
   *   directories, as Node finds them; a package that is not installed is not followed.
   *
   * A `package.json` with a name declares one item of the kind `package`.
   *
   * ```ts
   * const app = loadProject("tsconfig.json", { readDependencies: true });
   * rule("the app is installed without lodash")
   *   .files(anyOf("package.json"))
   *   .mustNotImport(anyOf(pkg("lodash")))
   *   .onlyKinds("dependency", "transitive-dependency")
   *   .check(app);
   * ```
   *
   * `mustOnlyImportDeclaredPackages` and `mustNotImportDevDependencies` need this option.
   *
   * Each of these references has the version and the licence of the installed package as
   * `properties`, for a check of your own: `seen.reference.properties?.["license"]`. Name
   * them in the rule, with `readingProperties`.
   *
   * It is not the default: a `package.json` is then a file that `all()` selects, so a rule
   * with a list of what the files may import would have to name every package, and a rule
   * such as `mustBeImported` would have to leave the file out. The load reads the
   * `package.json` of each installed package that the dependencies lead to. Default: `false`.
   */
  readonly readDependencies?: boolean;
}

/** One or more analysed TypeScript projects. */
export class Project {
  /** The directory that file paths are relative to. */
  readonly root: string;
  /** The tsconfig files that were loaded, also those found through `references`. */
  readonly configFiles: readonly string[];
  /** The facts, for the core. */
  readonly facts: FactSet;

  /** @internal */
  constructor(root: string, configFiles: readonly string[], facts: FactSet) {
    this.root = root;
    this.configFiles = configFiles;
    this.facts = facts;
  }

  /** The paths of all files, relative to the root, such as `src/app/main.ts`. */
  get files(): string[] {
    return this.facts.units.map((unit) => unit.join("/"));
  }
}

/**
 * Loads the project of a tsconfig file, and the projects it names in `references`. A
 * directory stands for its `tsconfig.json`, or for its `jsconfig.json` when it has none. A
 * directory with neither is read with default options: all its files below it except
 * `node_modules`, JavaScript too, with the resolution of a bundler, which finds `./db` and
 * `./db.js` alike. A JavaScript project is also read through a tsconfig file with `allowJs`.
 *
 * Imports are resolved by the installed TypeScript compiler, with the options of the project
 * that owns the file: `paths`, package `exports` and `imports`, and project references.
 */
export function loadProject(tsconfig: string | readonly string[], options: LoadOptions = {}): Project {
  const inputs = typeof tsconfig === "string" ? [tsconfig] : [...tsconfig];
  // The directories of the configs that refpact writes for directories without one.
  const written: string[] = [];
  try {
    const given = inputs.map((input) => configPath(input, written));
    const first = given[0];
    if (first === undefined) {
      throw new Error("refpact: loadProject needs a tsconfig file");
    }
    const configFiles = withReferences(given.map(({ file }) => file));
    const root = path.resolve(options.root ?? first.directory);

    const extensions = options.components === true ? [...COMPONENT_EXTENSIONS] : options.components === false ? [] : [...(options.components ?? [])];
    const session = openProjects(root, configFiles, extensions.length > 0 ? componentFiles(extensions) : undefined);
    let project: Project;
    try {
      const own = configFiles.filter((file) => !written.includes(path.dirname(file)));
      const members = options.resolveMembers === true || options.followImplementations === true;
      const facts = analyse(root, session.projects, members, options.followImplementations === true, options.resolveUses === true, extensions);
      project = new Project(root, own, options.readDependencies === true ? withDependencies(root, facts) : facts);
    } finally {
      session.close();
    }
    if (written.length > 0 && project.facts.units.length === 0) {
      throw new Error(`refpact: ${root} has no tsconfig.json, no jsconfig.json and no JavaScript or TypeScript files.`);
    }
    return project;
  } finally {
    for (const directory of written) {
      rmSync(directory, { recursive: true, force: true });
    }
  }
}

/** A config file to load, and the directory that file paths are relative to by default. */
interface ConfigPath {
  readonly file: string;
  readonly directory: string;
}

/**
 * The config file of a path: the file itself, or in a directory its `tsconfig.json`, else
 * its `jsconfig.json`, which a JavaScript project has. For a directory with neither, a
 * config with default options is written to a new directory, which is added to `written`.
 * Without `written`, as for a project reference, such a directory is an error.
 */
function configPath(file: string, written?: string[]): ConfigPath {
  const absolute = path.resolve(file);
  if (!existsSync(absolute)) {
    throw new Error(`refpact: ${absolute} does not exist`);
  }
  if (!statSync(absolute).isDirectory()) {
    return { file: absolute, directory: path.dirname(absolute) };
  }
  const found = ["tsconfig.json", "jsconfig.json"].map((name) => path.join(absolute, name)).find((candidate) => existsSync(candidate));
  if (found !== undefined) {
    return { file: found, directory: absolute };
  }
  if (written === undefined) {
    throw new Error(`refpact: ${absolute} has no tsconfig.json and no jsconfig.json.`);
  }
  const directory = mkdtempSync(path.join(tmpdir(), "refpact-config-"));
  written.push(directory);
  const glob = absolute.split(path.sep).join("/");
  const config = {
    compilerOptions: { allowJs: true, module: "preserve", noEmit: true, types: [] },
    // A wildcard of the compiler leaves out `node_modules`.
    include: [`${glob}/**/*`],
  };
  const configFile = path.join(directory, "jsconfig.json");
  writeFileSync(configFile, JSON.stringify(config));
  return { file: configFile, directory: absolute };
}

/** Adds the projects that the given tsconfig files name in `references`, at any depth. */
function withReferences(configFiles: readonly string[]): string[] {
  const all: string[] = [];
  const pending = [...configFiles];
  while (pending.length > 0) {
    const file = pending.shift() as string;
    if (all.includes(file)) {
      continue;
    }
    all.push(file);
    const config = parseJsonWithComments(readFileSync(file, "utf8"), file) as { references?: unknown };
    if (Array.isArray(config.references)) {
      for (const reference of config.references as { path?: unknown }[]) {
        if (typeof reference.path === "string") {
          pending.push(configPath(path.resolve(path.dirname(file), reference.path)).file);
        }
      }
    }
  }
  return all;
}

/** Parses JSON that may have comments and trailing commas, as tsconfig files do. */
export function parseJsonWithComments(text: string, file: string): unknown {
  let plain = "";
  let index = 0;
  while (index < text.length) {
    const char = text[index] as string;
    const next = text[index + 1];
    if (char === '"') {
      const start = index;
      index += 1;
      while (index < text.length && text[index] !== '"') {
        index += text[index] === "\\" ? 2 : 1;
      }
      index += 1;
      plain += text.slice(start, index);
    } else if (char === "/" && next === "/") {
      while (index < text.length && text[index] !== "\n") {
        index += 1;
      }
    } else if (char === "/" && next === "*") {
      const end = text.indexOf("*/", index + 2);
      index = end === -1 ? text.length : end + 2;
    } else {
      plain += char;
      index += 1;
    }
  }
  try {
    return JSON.parse(plain.replace(/,(\s*[}\]])/g, "$1"));
  } catch (error) {
    throw new Error(`refpact: cannot read ${file}: ${(error as Error).message}`);
  }
}

interface Unit {
  readonly path: Path;
  readonly file: string;
  readonly project: CompilerProject;
}

/** The facts with the `package.json` files of the project as units, and what they name as references. */
function withDependencies(root: string, facts: FactSet): FactSet {
  const files = facts.units.map((unit) => path.resolve(root, ...unit));
  const found = dependencyFacts(root, files);
  // The units stay in the order of their paths, and each keeps its number of lines.
  const lines = facts.unitLines ?? [];
  const units = [...facts.units.map((unit, index) => ({ path: unit, lines: lines[index] ?? 0 })), ...found.units];
  units.sort((a, b) => compare(a.path.join("/"), b.path.join("/")));
  return {
    ...facts,
    units: units.map((unit) => unit.path),
    unitLines: units.map((unit) => unit.lines),
    unitFiles: units.map((unit) => unit.path.join("/")),
    items: [...facts.items, ...found.declarations.map((declaration) => declaration.path)],
    kinds: [...facts.kinds, ...DEPENDENCY_KINDS],
    facts: [...facts.facts, ...found.facts],
    declarations: [...(facts.declarations ?? []), ...found.declarations],
    itemKinds: [...(facts.itemKinds ?? []), PACKAGE_ITEM_KIND],
    properties: [...PACKAGE_PROPERTIES],
  };
}

function analyse(
  root: string,
  projects: readonly CompilerProject[],
  members: boolean,
  implementations: boolean,
  nameUses: boolean,
  components: readonly ComponentExtension[],
): FactSet {
  // A file belongs to the first project that has it. Its imports are resolved there.
  const units = new Map<string, Unit>();
  for (const project of projects) {
    for (const file of project.ownFiles) {
      // A component file is a unit under its own name, not under the name that the compiler has for it.
      const component = components.length > 0 && !existsSync(file) ? componentOf(file, components) : undefined;
      const relative = path.relative(root, component?.file ?? file).split(path.sep).join("/");
      if (!units.has(file) && !relative.startsWith("../") && !path.isAbsolute(relative)) {
        units.set(file, { path: relative.split("/"), file, project });
      }
    }
  }
  const ordered = [...units.values()].sort((a, b) => compare(a.path.join("/"), b.path.join("/")));
  const lookup = new UnitLookup(root, ordered);

  const facts: Fact[] = [];
  const items: Path[] = [];
  const declarations: Declaration[] = [];
  const unitLines: number[] = [];
  for (const unit of ordered) {
    const sourceFile = unit.project.sourceFile(unit.file);
    if (sourceFile === undefined) {
      unitLines.push(0);
      continue;
    }
    unitLines.push(countLines(sourceFile.text));
    const found = factsOfFile(unit, sourceFile, lookup, members, nameUses);
    for (const name of found.exportNames) {
      items.push([...unit.path, exportSegment(name)]);
    }
    found.facts.sort((a, b) => a.line - b.line || a.column - b.column);
    facts.push(...found.facts);
    declarations.push(...found.declarations);
    // The client of the native compiler keeps each tree that it gave. A large project has
    // more trees than fit in memory, and no file is read twice.
    unit.project.forgetSourceFiles();
  }

  addAssignable(declarations);
  if (implementations) {
    addImplementations(facts, declarations);
  }

  return {
    separator: "/",
    units: ordered.map((unit) => unit.path),
    unitLines,
    unitFiles: ordered.map((unit) => unit.path.join("/")),
    items,
    kinds: members ? [...KINDS, MEMBER_KIND] : [...KINDS],
    markers: [...MARKERS],
    tags: [],
    facts,
    declarations,
    itemKinds: [...ITEM_KINDS],
    memberKinds: [...MEMBER_KINDS],
    visibilities: [...VISIBILITIES],
    relations: [...RELATIONS],
    modifiers: [...MODIFIERS],
    attachedPrefixes: ATTACHED_PREFIXES,
    hiddenPrefixes: HIDDEN_PREFIXES,
  };
}

/**
 * Gives each declaration the relation `assignable-to` to each type that it extends or
 * implements, directly or through other declarations of the project: when `Order extends
 * Entity` and `Entity implements Saved`, `Order` is assignable to both.
 */
function addAssignable(declarations: Declaration[]): void {
  const byPath = new Map(declarations.map((declaration) => [declaration.path.join("\0"), declaration]));
  const supertypes = (declaration: Declaration): Path[] =>
    declaration.relations.filter((tie) => tie.relation === "extends" || tie.relation === "implements").map((tie) => tie.target);
  for (const [index, declaration] of declarations.entries()) {
    const found: Path[] = [];
    const seen = new Set<string>([declaration.path.join("\0")]);
    const pending = supertypes(declaration);
    while (pending.length > 0) {
      const target = pending.shift() as Path;
      const key = target.join("\0");
      if (seen.has(key)) {
        continue;
      }
      seen.add(key);
      found.push(target);
      const next = byPath.get(key);
      if (next !== undefined) {
        pending.push(...supertypes(next));
      }
    }
    if (found.length > 0) {
      declarations[index] = { ...declaration, relations: [...declaration.relations, ...found.map((target) => ({ relation: "assignable-to", target }))] };
    }
  }
}

/**
 * After each use of a member of an interface or of a class: a candidate use of the same
 * member of each class of another file that is assignable to it.
 */
function addImplementations(facts: Fact[], declarations: readonly Declaration[]): void {
  const key = (target: Path): string => target.join("\0");
  // The classes that are assignable to a declaration, by its path.
  const below = new Map<string, Declaration[]>();
  for (const declaration of declarations) {
    if (declaration.kind !== "class") {
      continue;
    }
    for (const tie of declaration.relations) {
      if (tie.relation === "assignable-to") {
        below.set(key(tie.target), [...(below.get(key(tie.target)) ?? []), declaration]);
      }
    }
  }
  if (below.size === 0) {
    return;
  }
  const all = facts.splice(0);
  for (const fact of all) {
    facts.push(fact);
    if (fact.what.type !== "reference" || fact.what.kind !== MEMBER_KIND) {
      continue;
    }
    const target = fact.what.target;
    const member = target[target.length - 1] as string;
    for (const implementer of below.get(key(target.slice(0, -1))) ?? []) {
      if (key(implementer.unit) === key(fact.unit)) {
        continue;
      }
      facts.push({
        ...fact,
        note: `may be called through ${[...target.slice(-2)].map((segment) => segment.replace(/^#/, "")).join(".")}`,
        what: { type: "reference", target: [...implementer.path, member], aliases: [], kind: MEMBER_KIND, certainty: "candidate" },
      });
    }
  }
}

/** The number of lines of a text. A line break at the end of the text starts no line. */
function countLines(text: string): number {
  if (text === "") {
    return 0;
  }
  let lines = 1;
  for (let index = text.indexOf("\n"); index !== -1 && index < text.length - 1; index = text.indexOf("\n", index + 1)) {
    lines += 1;
  }
  return lines;
}

function compare(a: string, b: string): number {
  return a < b ? -1 : a > b ? 1 : 0;
}

class UnitLookup {
  readonly root: string;
  private readonly byFile = new Map<string, Unit>();

  constructor(root: string, units: readonly Unit[]) {
    this.root = root;
    for (const unit of units) {
      this.byFile.set(unit.file, unit);
      // The compiler names files by a canonical path, which is lower case on file systems
      // that ignore case.
      this.byFile.set(unit.file.toLowerCase(), unit);
    }
  }

  unit(file: string | undefined): Unit | undefined {
    return file === undefined ? undefined : (this.byFile.get(file) ?? this.byFile.get(file.toLowerCase()));
  }

  /** The units whose file, as an absolute name with `/`, the test accepts, in the order of their paths. */
  matching(test: RegExp): Unit[] {
    const found = new Set<Unit>();
    for (const unit of this.byFile.values()) {
      if (test.test(unit.file.split(path.sep).join("/"))) {
        found.add(unit);
      }
    }
    return [...found].sort((a, b) => compare(a.path.join("/"), b.path.join("/")));
  }
}

/**
 * The facts of one file, all names that the file exports, also through `export *`, and the
 * items that it declares.
 */
function factsOfFile(
  unit: Unit,
  sourceFile: SourceFile,
  lookup: UnitLookup,
  resolveMembers: boolean,
  resolveUses: boolean,
): { facts: Fact[]; exportNames: string[]; declarations: Declaration[] } {
  const project = unit.project;
  // A TypeScript file is read without a visit of the nodes that cannot name a module.
  const prune = /\.[cm]?tsx?$/.test(unit.file);
  const uses = collectModuleUses(sourceFile, prune);
  const bindings = uses.flatMap((use) => (use.namespace !== undefined ? [{ use, binding: use.namespace }] : []));
  const members = collectMemberUses(sourceFile, new Set(bindings.map(({ binding }) => binding.local)), prune);

  // One round trip to the compiler for every node of this file that needs a symbol.
  const nodes: Node[] = [];
  const ask = (node: Node): number => nodes.push(node) - 1;
  const own = ask(sourceFile);
  const asked = uses.map((use) => ({
    specifier: ask(use.specifierNode),
    require: use.requireNode !== undefined ? ask(use.requireNode) : undefined,
    names: use.names.map((name) => ask(name.node)),
    namespace: use.namespace !== undefined ? ask(use.namespace.node) : undefined,
    constants: (use.constants ?? []).map((constant) => ({ use: ask(constant.use), declaration: ask(constant.declaration) })),
  }));
  const askedMembers = members.map((member) => ({
    object: ask(member.object),
    member: ask(member.member),
    alias: member.alias !== undefined ? { declaration: ask(member.alias.declaration), of: ask(member.alias.of) } : undefined,
  }));
  const accesses = resolveMembers ? collectAccesses(sourceFile) : [];
  const askedAccesses = accesses.map(ask);
  const globals = collectGlobalUses(sourceFile);
  const askedGlobals = globals.map((use) => ({ name: use.shorthand !== undefined ? undefined : ask(use.node), through: use.through !== undefined ? ask(use.through) : undefined }));
  const markers = collectMarkers(sourceFile, prune);
  const askedMarkers = markers.map((use) => (use.object !== undefined ? ask(use.object) : undefined));
  const declared = collectDeclarations(sourceFile);
  // The compiler says whether the file exports an item. A member has its visibility in its syntax.
  const askedDeclared = declared.map((item) => ({ at: item.member === undefined ? ask(item.at) : undefined, ties: item.ties.map((tie) => ask(tie.from?.specifier ?? tie.node)) }));
  const pathOf = (item: (typeof declared)[number]): Path => [...unit.path, ...(item.member !== undefined ? [exportSegment(item.member.owner)] : []), exportSegment(item.name)];
  const sites = resolveUses ? collectNameSites(sourceFile, uses) : [];
  const askedSites = sites.map((site) => (site.shorthand === undefined ? ask(site.node) : undefined));
  // The innermost item around a position: a member comes after its class, and is inside it.
  const itemAt = (position: number): Path | undefined => {
    const around = declared.findLast((item) => item.whole.getStart(sourceFile) <= position && position < item.whole.end);
    return around === undefined ? undefined : pathOf(around);
  };
  const symbols = project.symbolsAt(nodes);
  const symbol = (index: number | undefined): TsSymbol | undefined => (index === undefined ? undefined : symbols[index]);

  const facts: Fact[] = [];
  // What a fact at a node has, except what it is.
  const placed = (node: Node, note: string | undefined): Omit<Fact, "what"> => {
    const start = node.getStart(sourceFile);
    const position = sourceFile.getLineAndCharacterOfPosition(start);
    const item = resolveUses ? itemAt(start) : undefined;
    return {
      unit: unit.path,
      ...(item !== undefined ? { item } : {}),
      file: unit.path.join("/"),
      line: position.line + 1,
      column: position.character + 1,
      tags: [],
      ...(note !== undefined ? { note } : {}),
    };
  };
  const add = (node: Node, kind: Kind, target: Path, aliases: Path[], note: string | undefined, certainty: "exact" | "candidate" = "exact"): void => {
    facts.push({
      ...placed(node, note),
      what: {
        type: "reference",
        target,
        aliases: aliases.filter((alias, index) => !samePath(alias, target) && aliases.findIndex((other) => samePath(other, alias)) === index),
        kind,
        certainty,
      },
    });
  };
  const mark = (node: Node, marker: (typeof MARKERS)[number], note: string | undefined): void => {
    facts.push({ ...placed(node, note), what: { type: "marker", marker } });
  };

  // A global of the host: declared, but in no file of the project and not in the ECMAScript library.
  globals.forEach((use, index) => {
    const indexes = askedGlobals[index] as (typeof askedGlobals)[number];
    if (indexes.through !== undefined && isDeclaredInProject(symbol(indexes.through), project, lookup)) {
      return;
    }
    const found = use.shorthand !== undefined ? project.shorthandValueAt(use.shorthand) : symbol(indexes.name);
    const files = found === undefined ? [] : project.filesOf(found);
    if (files.length === 0 || files.some((file) => lookup.unit(file) !== undefined || isLanguageLibrary(file))) {
      return;
    }
    const target = [globalSegment(use.name), ...(use.member !== undefined ? [exportSegment(use.member)] : [])];
    add(use.at, GLOBAL_KIND, target, [], undefined);
  });

  // `console` is a marker when the compiler finds no declaration of it in the project.
  markers.forEach((use, index) => {
    const asked = askedMarkers[index];
    if (asked !== undefined && isDeclaredInProject(symbol(asked), project, lookup)) {
      return;
    }
    mark(use.at, use.marker, undefined);
  });

  const resolved = new Map<ModuleUse, ResolvedModule>();
  // The paths of each imported name, by its symbol, for the relations of the declarations.
  const imported: { readonly symbol: TsSymbol; readonly paths: Path[]; readonly kind: Kind; readonly note: string | undefined }[] = [];
  uses.forEach((use, index) => {
    const indexes = asked[index] as (typeof asked)[number];
    // A `require` that is declared in the project is a local function, not Node's.
    if (use.requireNode !== undefined && isDeclaredInProject(symbol(indexes.require), project, lookup)) {
      return;
    }
    // A constant has the value that was read only when its name is the variable that was read.
    const proved = indexes.constants.every((constant) => {
      const [used, declared] = [symbol(constant.use), symbol(constant.declaration)];
      return used !== undefined && declared !== undefined && project.same(used, declared);
    });
    const computed = proved ? use.computed : "unknown";
    if (computed === "unknown" || computed === "pattern") {
      // The code computes the specifier. What is known of it names the files that it can be.
      mark(use.at, "computed-import", computed === "pattern" ? use.specifier : undefined);
      if (computed === "pattern") {
        const note = `the specifier is computed: ${use.specifier}`;
        for (const target of patternTargets(use.specifier, unit, lookup, false)) {
          add(use.at, use.kind, target, [], note, "candidate");
        }
      }
      return;
    }
    if (computed === "glob") {
      for (const target of patternTargets(use.specifier, unit, lookup, true)) {
        add(use.at, use.kind, target, [], `matches ${use.specifier}`);
      }
      return;
    }
    // The compiler knows the module of a specifier that is one string in the place.
    const known = computed === undefined ? declaredFile(symbol(indexes.specifier), project) : undefined;
    const module = resolveModule(use.specifier, unit, known, lookup, use.asset === true);
    resolved.set(use, module);
    // The compiler resolves a specifier that is one string. For another one, and for the file
    // of `new URL(..)`, which need not be a module, nothing says that it leads nowhere.
    if (module.note === NOT_RESOLVED && computed === undefined && use.asset !== true) {
      mark(use.specifierNode, "unresolved-import", use.specifier);
    }
    if (use.whole) {
      const note = computed === "constant" && module.note === NOT_RESOLVED ? undefined : module.note;
      add(use.at, use.kind, module.base, module.fileBase !== undefined ? [module.fileBase] : [], note);
    }
    use.names.forEach((name, nameIndex) => {
      const target = [...module.base, exportSegment(name.exported)];
      const aliases: Path[] = module.fileBase !== undefined ? [[...module.fileBase, exportSegment(name.exported)]] : [];
      const from = name.specifier === true ? { module: symbol(indexes.specifier), name: name.exported } : undefined;
      const nameSymbol = symbol(indexes.names[nameIndex]);
      aliases.push(...declarations(nameSymbol, project, lookup, false, from));
      add(name.node, name.kind, target, aliases, module.note);
      if (nameSymbol !== undefined) {
        imported.push({ symbol: nameSymbol, paths: [target, ...aliases], kind: name.kind, note: module.note });
      }
    });
  });

  // A use of a name that an import binds is an import of what the name stands for.
  sites.forEach((site, index) => {
    const used = site.shorthand !== undefined ? project.shorthandValueAt(site.shorthand) : symbol(askedSites[index]);
    const entry = used === undefined ? undefined : imported.find((name) => project.same(name.symbol, used));
    const [target, ...aliases] = entry?.paths ?? [];
    if (entry !== undefined && target !== undefined) {
      add(site.node, entry.kind, target, aliases, entry.note);
    }
  });

  // `object.name`, where the name is a member of a class, an interface or an enum that
  // another file of the project declares.
  accesses.forEach((name, index) => {
    const member = symbol(askedAccesses[index]);
    const owner = member === undefined ? undefined : project.ownerOf(member);
    const declaredIn = owner === undefined ? undefined : lookup.unit(declaredFile(owner, project));
    if (member === undefined || owner === undefined || declaredIn === undefined || declaredIn === unit) {
      return;
    }
    add(name, MEMBER_KIND, [...declaredIn.path, exportSegment(owner.name), exportSegment(member.name)], [], undefined);
  });

  // `ns.member`, where `ns` is bound to a module by `import * as ns` or by `require`.
  members.forEach((member, index) => {
    const indexes = askedMembers[index] as (typeof askedMembers)[number];
    let objectSymbol = symbol(indexes.object);
    if (indexes.alias !== undefined) {
      // The object is a `const` whose value is the binding: it must be that `const`.
      const declared = symbol(indexes.alias.declaration);
      const same = objectSymbol !== undefined && declared !== undefined && project.same(objectSymbol, declared);
      objectSymbol = same ? symbol(indexes.alias.of) : undefined;
    }
    const owner = bindings.find(({ use, binding }) => {
      const bindingSymbol = symbol(asked[uses.indexOf(use)]?.namespace);
      return binding.local === member.objectName && objectSymbol !== undefined && bindingSymbol !== undefined && project.same(bindingSymbol, objectSymbol);
    });
    const module = owner !== undefined ? resolved.get(owner.use) : undefined;
    if (owner === undefined || module === undefined) {
      return;
    }
    const target = [...module.base, exportSegment(member.memberName)];
    const aliases: Path[] = module.fileBase !== undefined ? [[...module.fileBase, exportSegment(member.memberName)]] : [];
    const memberSymbol = symbol(indexes.member);
    aliases.push(...declarations(memberSymbol, project, lookup, memberSymbol !== undefined));
    add(member.member, owner.binding.kind, target, aliases, module.note);
  });

  const module = symbol(own);
  const exports = module === undefined ? new Map<string, TsSymbol>() : project.exportsOf(module);
  // What the exports of the file stand for, asked only when a declaration has no `export`.
  let exported: TsSymbol[] | undefined;
  const isExported = (own: TsSymbol | undefined): boolean => {
    if (own === undefined || exports.size === 0) {
      return false;
    }
    exported ??= [...exports.values()].flatMap((entry) => [entry, project.immediateAlias(entry) ?? entry]);
    return exported.some((entry) => project.same(entry, own));
  };
  const items = declared.map((item, index): Declaration => {
    const indexes = askedDeclared[index] as (typeof askedDeclared)[number];
    const relations: Relation[] = [];
    item.ties.forEach((tie, tieIndex) => {
      const asked = symbol(indexes.ties[tieIndex]);
      // `import("./port").Port`: the symbol is that of the module, which has the export.
      const tied = tie.from === undefined || asked === undefined ? asked : project.exportsOf(asked).get(tie.from.name);
      // A type parameter is no type of its own: `T` in `first<T>(items: T[]): T`.
      if (tied !== undefined && project.isTypeParameter(tied)) {
        return;
      }
      const entry = tied === undefined ? undefined : imported.find((name) => project.same(name.symbol, tied));
      const paths = entry !== undefined ? entry.paths : declarations(tied, project, lookup, true);
      for (const target of paths) {
        if (!relations.some((known) => known.relation === tie.relation && samePath(known.target, target))) {
          relations.push({ relation: tie.relation, target });
        }
      }
    });
    const position = sourceFile.getLineAndCharacterOfPosition(item.at.getStart(sourceFile));
    return {
      path: pathOf(item),
      name: item.name,
      unit: unit.path,
      file: unit.path.join("/"),
      line: position.line + 1,
      column: position.character + 1,
      endLine: sourceFile.getLineAndCharacterOfPosition(item.whole.end).line + 1,
      tags: [],
      kind: item.kind,
      visibility: item.member?.visibility ?? (item.exported || isExported(symbol(indexes.at)) ? "exported" : "local"),
      relations,
      modifiers: [...item.modifiers],
    };
  });
  return { facts, exportNames: [...exports.keys()], declarations: items };
}

function samePath(a: Path, b: Path): boolean {
  return a.length === b.length && a.every((segment, index) => segment === b[index]);
}

function declaredFile(symbol: TsSymbol | undefined, project: CompilerProject): string | undefined {
  return symbol === undefined ? undefined : project.filesOf(symbol)[0];
}

/** Whether a file is a part of the ECMAScript library of the compiler: `lib.es5.d.ts`, `lib.es2015.promise.d.ts`. */
function isLanguageLibrary(file: string): boolean {
  return /^lib\.(es\d|esnext|decorators)/.test(path.basename(file));
}

function isDeclaredInProject(symbol: TsSymbol | undefined, project: CompilerProject, lookup: UnitLookup): boolean {
  return symbol !== undefined && project.filesOf(symbol).some((file) => lookup.unit(file) !== undefined);
}

/**
 * The places that a name leads to through re-exports, nearest first. Each place is a file
 * of the project and the name it has there.
 */
function declarations(
  symbol: TsSymbol | undefined,
  project: CompilerProject,
  lookup: UnitLookup,
  includeFirst = false,
  from?: ExportOfModule,
): Path[] {
  const places: Path[] = [];
  let current = symbol;
  for (let hop = 0; hop < 16 && current !== undefined; hop += 1) {
    if (hop > 0 || includeFirst) {
      const unit = lookup.unit(declaredFile(current, project));
      if (unit !== undefined) {
        places.push([...unit.path, exportSegment(current.name)]);
      }
    }
    current = hop === 0 && from !== undefined ? firstStep(current, from, project) : project.immediateAlias(current);
  }
  return places;
}

/** The module that a name of an import or a re-export is taken from, and the name it has there. */
interface ExportOfModule {
  readonly module: TsSymbol | undefined;
  readonly name: string;
}

/**
 * One step from the name of an import or a re-export to the export that it stands for.
 *
 * The compiler can answer this for each name, but with the native compiler each answer is a
 * round trip to another process, and a project has many more imported names than modules.
 * So the answer is read from the exports of the module, which one round trip gives for all
 * names. The compiler finds the export in the same table. The name `default` is the
 * exception and is left to the compiler: it can stand for the module as a whole.
 */
function firstStep(symbol: TsSymbol, from: ExportOfModule, project: CompilerProject): TsSymbol | undefined {
  const found = from.module === undefined || from.name === "default" ? undefined : project.exportsOf(from.module).get(from.name);
  if (found === undefined) {
    return project.immediateAlias(symbol);
  }
  if (checks.aliases) {
    const asked = project.immediateAlias(symbol);
    if (asked === undefined || !project.same(asked, found)) {
      throw new Error(`refpact: the export \`${from.name}\` of a module is not what the compiler gives for an import of it`);
    }
  }
  return found;
}

/** The note of an import that leads to no file. */
const NOT_RESOLVED = "not resolved";

interface ResolvedModule {
  /** The path of the module as the rules see it: a file of the project, or a package. */
  readonly base: Path;
  /** For a package whose source is a file of the project: that file. */
  readonly fileBase?: Path;
  readonly note?: string;
}

function isRelative(specifier: string): boolean {
  return specifier.startsWith("./") || specifier.startsWith("../") || specifier === "." || specifier === "..";
}

/**
 * The paths that a specifier with parts that are not known can be: the files of the project
 * that it matches, or the package that it names. With `glob`, the pattern is a glob of
 * `import.meta.glob`: `*` stays in one directory and `**` does not. Otherwise each `*` is a
 * part that the code computes, of any text.
 */
function patternTargets(pattern: string, from: Unit, lookup: UnitLookup, glob: boolean): Path[] {
  if (isRelative(pattern) || path.isAbsolute(pattern)) {
    const start = path.resolve(path.dirname(from.file), pattern).split(path.sep).join("/");
    const literal = (text: string): string => text.replace(/[.+?^${}()|[\]\\]/g, "\\$&");
    const body = glob
      ? start
          .split(/(\*\*\/|\*\*|\*)/)
          .map((part) => (part === "**/" ? "(.*/)?" : part === "**" ? ".*" : part === "*" ? "[^/]*" : literal(part)))
          .join("")
      : start.split("*").map(literal).join(".*");
    // A computed specifier can leave out the extension, or name a directory with an index file.
    const endings = glob ? "" : `(/index)?(${EXTENSIONS.map(literal).join("|")})?`;
    return lookup
      .matching(new RegExp(`^${body}${endings}$`))
      .filter((unit) => unit !== from)
      .map((unit) => unit.path);
  }
  // Of a package, the name must be known: `pkg/${name}`, not `${name}/sub`.
  const parts = pattern.split("/");
  const nameLength = pattern.startsWith("@") ? 2 : 1;
  const name = parts.slice(0, nameLength).join("/");
  const baseUrl = from.project.options["baseUrl"];
  const aliased =
    pattern.startsWith("#") ||
    matchesPaths(name, from.project.options["paths"]) ||
    matchesPaths(pattern, from.project.options["paths"]) ||
    // With `baseUrl`, the name of a directory below it is no package.
    (typeof baseUrl === "string" && existsSync(path.resolve(baseUrl, name)));
  if (glob || aliased || parts.length <= nameLength || name.includes("*") || name === "") {
    return [];
  }
  return [isBuiltin(name) ? [nodeSegment(name.replace(/^node:/, ""))] : [packageSegment(name)]];
}

function resolveModule(specifier: string, from: Unit, resolvedFile: string | undefined, lookup: UnitLookup, asset = false): ResolvedModule {
  let target = lookup.unit(resolvedFile);
  const relative = isRelative(specifier);

  if (relative || path.isAbsolute(specifier)) {
    if (target !== undefined) {
      return { base: target.path };
    }
    // The compiler does not resolve `require("./x")` in a TypeScript file. The file system
    // answers for relative paths.
    const start = path.resolve(path.dirname(from.file), specifier);
    // The file of `new URL(..)` is named in full.
    target = lookup.unit(asset ? start : probe(start)) ?? lookup.unit(probe(start));
    if (target !== undefined) {
      return { base: target.path };
    }
    const outside = path.relative(lookup.root, resolvedFile ?? start).split(path.sep);
    // A file of `new URL(..)` that is there is found, also when it is no module.
    const found = resolvedFile !== undefined || (asset && existsSync(start));
    return found ? { base: outside } : { base: outside, note: NOT_RESOLVED };
  }

  if (isBuiltin(specifier)) {
    const [name, ...rest] = specifier.replace(/^node:/, "").split("/") as [string, ...string[]];
    return { base: [nodeSegment(name), ...rest] };
  }

  // `#name` is an entry of `imports` in package.json; a `paths` pattern is an alias of the
  // project; a name that TypeScript 6 finds below `baseUrl` is a path. All lead to files,
  // not packages.
  if (specifier.startsWith("#") || matchesPaths(specifier, from.project.options["paths"]) || isBelowBaseUrl(specifier, resolvedFile, from)) {
    if (target !== undefined) {
      return { base: target.path };
    }
    return resolvedFile !== undefined
      ? { base: path.relative(lookup.root, resolvedFile).split(path.sep) }
      : { base: specifier.split("/"), note: NOT_RESOLVED };
  }

  const parts = specifier.split("/");
  const nameLength = specifier.startsWith("@") ? 2 : 1;
  const base = [packageSegment(parts.slice(0, nameLength).join("/")), ...parts.slice(nameLength)];
  // A package of the same workspace can resolve to a source file of the project.
  return target !== undefined ? { base, fileBase: target.path } : { base };
}

/** True when the compiler found the specifier as a path from the `baseUrl` of the project. */
function isBelowBaseUrl(specifier: string, resolvedFile: string | undefined, from: Unit): boolean {
  const baseUrl = from.project.options["baseUrl"];
  if (typeof baseUrl !== "string" || resolvedFile === undefined) {
    return false;
  }
  const named = path.resolve(baseUrl, specifier);
  return resolvedFile === named || resolvedFile.startsWith(`${named}.`) || resolvedFile.startsWith(named + path.sep);
}

const EXTENSIONS = [".ts", ".tsx", ".mts", ".cts", ".d.ts", ".js", ".jsx", ".mjs", ".cjs", ".json"];
const SOURCE_OF_OUTPUT: Readonly<Record<string, readonly string[]>> = {
  ".js": [".ts", ".tsx"],
  ".jsx": [".tsx"],
  ".mjs": [".mts"],
  ".cjs": [".cts"],
};

/** Finds the file of a relative specifier the way TypeScript names files. */
function probe(start: string): string | undefined {
  const candidates = [start];
  const extension = path.extname(start);
  for (const source of SOURCE_OF_OUTPUT[extension] ?? []) {
    candidates.push(start.slice(0, -extension.length) + source);
  }
  for (const added of EXTENSIONS) {
    candidates.push(start + added);
  }
  for (const added of EXTENSIONS) {
    candidates.push(path.join(start, `index${added}`));
  }
  return candidates.find((candidate) => existsSync(candidate) && statSync(candidate).isFile());
}

function matchesPaths(specifier: string, paths: unknown): boolean {
  if (typeof paths !== "object" || paths === null) {
    return false;
  }
  return Object.keys(paths).some((pattern) => {
    const star = pattern.indexOf("*");
    if (star === -1) {
      return pattern === specifier;
    }
    const prefix = pattern.slice(0, star);
    const suffix = pattern.slice(star + 1);
    return specifier.length >= prefix.length + suffix.length && specifier.startsWith(prefix) && specifier.endsWith(suffix);
  });
}
