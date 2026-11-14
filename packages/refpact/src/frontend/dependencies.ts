/**
 * The dependencies that the `package.json` files of a project name: each is a reference
 * from the `package.json` to a package, so that a rule can say which package may depend on
 * which. This is the one place that reads `package.json` files.
 */
import { existsSync, readFileSync, realpathSync } from "node:fs";
import path from "node:path";

import type { Declaration, Fact, Path } from "../core/index.ts";
import { exportSegment, packageSegment } from "./paths.ts";

/**
 * The kinds of the references that a `package.json` gives. Only a project that is loaded
 * with `readDependencies` has them.
 *
 * - `dependency`, `dev-dependency`, `peer-dependency` and `optional-dependency`: a package
 *   that `dependencies`, `devDependencies`, `peerDependencies` or `optionalDependencies` names.
 * - `transitive-dependency`: a package that the file does not name so, and that one of its
 *   `dependencies`, `peerDependencies` or `optionalDependencies` needs.
 * - `transitive-dev-dependency`: a package that only its `devDependencies` need.
 *
 * Each of these references has the properties of its package, when the package is
 * installed: its version and its licence.
 */
export const DEPENDENCY_KINDS = [
  "dependency",
  "dev-dependency",
  "peer-dependency",
  "optional-dependency",
  "transitive-dependency",
  "transitive-dev-dependency",
] as const;

/** One of {@link DEPENDENCY_KINDS}. */
export type DependencyKind = (typeof DEPENDENCY_KINDS)[number];

/** The kinds of the dependencies that a `package.json` names itself. */
export const NAMED_DEPENDENCY_KINDS: readonly string[] = ["dependency", "dev-dependency", "peer-dependency", "optional-dependency"];

/**
 * The properties of a reference to an installed package, from its own `package.json`:
 *
 * - `version`: its version, such as `4.17.21`.
 * - `license`: its licence as the file writes it, such as `MIT` or `(MIT OR Apache-2.0)`. A
 *   package that names no licence has none.
 *
 * A package that is not installed has neither. Two versions of one package are two
 * references.
 */
export const PACKAGE_PROPERTIES = ["version", "license"] as const;

/** The kind of the item that a `package.json` with a name declares: its package. */
export const PACKAGE_ITEM_KIND = "package";

/** The keys of a `package.json` that name dependencies, with the kind of each. */
const SECTIONS: readonly (readonly [string, DependencyKind])[] = [
  ["dependencies", "dependency"],
  ["devDependencies", "dev-dependency"],
  ["peerDependencies", "peer-dependency"],
  ["optionalDependencies", "optional-dependency"],
];

/** The keys whose packages are needed where the package is installed. */
const INSTALLED = ["dependencies", "peerDependencies", "optionalDependencies"];

interface Place {
  readonly line: number;
  readonly column: number;
}

/** What one `package.json` declares. */
interface Manifest {
  /** The absolute path of the file. */
  readonly file: string;
  readonly name: string | undefined;
  /** The number of lines of the file. */
  readonly lines: number;
  /** The place of each key of the objects at the top of the file, by the key of the object. */
  readonly places: ReadonlyMap<string, ReadonlyMap<string, Place>>;
  /** The place of the key `name`. */
  readonly namePlace: Place | undefined;
}

/**
 * The places of the keys of a JSON text: for each key at the top that has an object, the
 * keys of that object. The key `""` has the keys at the top themselves.
 */
function keyPlaces(text: string): Map<string, Map<string, Place>> {
  const found = new Map<string, Map<string, Place>>();
  const record = (section: string, key: string, place: Place): void => {
    const keys = found.get(section) ?? new Map<string, Place>();
    found.set(section, keys);
    if (!keys.has(key)) {
      keys.set(key, place);
    }
  };
  let depth = 0;
  let section: string | undefined;
  let line = 1;
  let lineStart = 0;
  for (let at = 0; at < text.length; at++) {
    const char = text[at];
    if (char === "\n") {
      line += 1;
      lineStart = at + 1;
    } else if (char === "{" || char === "[") {
      depth += 1;
    } else if (char === "}" || char === "]") {
      depth -= 1;
    } else if (char === '"') {
      let end = at + 1;
      while (end < text.length && text[end] !== '"') {
        end += text[end] === "\\" ? 2 : 1;
      }
      let after = end + 1;
      while (after < text.length && /\s/.test(text[after] as string)) {
        after += 1;
      }
      if (text[after] === ":" && depth <= 2) {
        const key = JSON.parse(text.slice(at, end + 1)) as string;
        const place = { line, column: at - lineStart + 1 };
        if (depth === 1) {
          section = key;
          record("", key, place);
        } else if (section !== undefined) {
          record(section, key, place);
        }
      }
      at = end;
    }
  }
  return found;
}

function read(file: string): Manifest | undefined {
  if (!existsSync(file)) {
    return undefined;
  }
  const text = readFileSync(file, "utf8");
  let json: Record<string, unknown>;
  try {
    json = JSON.parse(text) as Record<string, unknown>;
  } catch (error) {
    throw new Error(`refpact: cannot read ${file}: ${(error as Error).message}`);
  }
  const places = keyPlaces(text);
  const name = typeof json["name"] === "string" ? json["name"] : undefined;
  // A `package.json` that has no name and no dependencies, such as one that only sets `"type"`, describes no package.
  if (name === undefined && SECTIONS.every(([section]) => (places.get(section)?.size ?? 0) === 0)) {
    return undefined;
  }
  return { file, name, lines: text.split("\n").length - (text.endsWith("\n") ? 1 : 0), places, namePlace: places.get("")?.get("name") };
}

/** Finds the `package.json` that a file belongs to. */
class Manifests {
  private readonly byDirectory = new Map<string, Manifest | undefined>();

  /** The nearest `package.json` that describes a package, in the directory of the file or above it. */
  of(file: string): Manifest | undefined {
    return this.inDirectory(path.dirname(file));
  }

  private inDirectory(directory: string): Manifest | undefined {
    if (this.byDirectory.has(directory)) {
      return this.byDirectory.get(directory);
    }
    const parent = path.dirname(directory);
    const found = read(path.join(directory, "package.json")) ?? (parent === directory ? undefined : this.inDirectory(parent));
    this.byDirectory.set(directory, found);
    return found;
  }
}

/** The packages that are installed, as `node_modules` directories have them. */
class Installed {
  private readonly found = new Map<string, string | undefined>();
  private readonly read = new Map<string, InstalledPackage>();

  /** The directory of the package that code in `from` gets for a name, without links; `undefined` when it is not installed. */
  resolve(from: string, name: string): string | undefined {
    const key = `${from}\u0000${name}`;
    if (this.found.has(key)) {
      return this.found.get(key);
    }
    let found: string | undefined;
    for (let directory = from; ; directory = path.dirname(directory)) {
      const candidate = path.join(directory, "node_modules", name);
      if (existsSync(path.join(candidate, "package.json"))) {
        found = realpathSync(candidate);
        break;
      }
      if (path.dirname(directory) === directory) {
        break;
      }
    }
    this.found.set(key, found);
    return found;
  }

  /** What the `package.json` of the package in a directory says. */
  package(directory: string): InstalledPackage {
    let found = this.read.get(directory);
    if (found === undefined) {
      let json: Record<string, unknown> = {};
      try {
        json = JSON.parse(readFileSync(path.join(directory, "package.json"), "utf8")) as Record<string, unknown>;
      } catch {
        // A package that cannot be read needs nothing that can be followed, and says nothing.
      }
      const needs = INSTALLED.flatMap((key) => {
        const value = json[key];
        return typeof value === "object" && value !== null ? Object.keys(value) : [];
      });
      // An old form of the licence is an object with the name as its `type`.
      const license = json["license"];
      const named = typeof license === "object" && license !== null ? (license as Record<string, unknown>)["type"] : license;
      const properties: Record<string, string> = {};
      for (const name of PACKAGE_PROPERTIES) {
        const value = name === "license" ? named : json[name];
        if (typeof value === "string") {
          properties[name] = value;
        }
      }
      found = { needs, properties };
      this.read.set(directory, found);
    }
    return found;
  }
}

/** What the `package.json` of an installed package says. */
interface InstalledPackage {
  /** The names of the packages that it needs where it is installed. */
  readonly needs: readonly string[];
  /** Its {@link PACKAGE_PROPERTIES}. */
  readonly properties: Readonly<Record<string, string>>;
}

/** What the `package.json` files of a project give. */
export interface DependencyFacts {
  /** Each `package.json` as a unit, with the number of its lines. */
  readonly units: readonly { readonly path: Path; readonly lines: number }[];
  readonly facts: readonly Fact[];
  /** The package that each `package.json` with a name declares. */
  readonly declarations: readonly Declaration[];
}

/**
 * The dependencies of the `package.json` files that the files of a project belong to. Each
 * `package.json` is a unit under its path from the root, and each dependency is a reference
 * from it to the package, at the line of its key.
 *
 * A package that another `package.json` of the project declares is also a reference to that
 * file. A package that a dependency needs is found in the `node_modules` directories, as
 * Node finds it; one that is not installed is not followed.
 */
export function dependencyFacts(root: string, files: readonly string[]): DependencyFacts {
  const manifests = new Manifests();
  const found = new Map<string, Manifest>();
  for (const file of files) {
    const manifest = manifests.of(file);
    if (manifest !== undefined) {
      found.set(manifest.file, manifest);
    }
  }
  const unitOf = (manifest: Manifest): Path => path.relative(root, manifest.file).split(path.sep);
  const ordered = [...found.values()].sort((a, b) => {
    const [left, right] = [unitOf(a).join("/"), unitOf(b).join("/")];
    return left < right ? -1 : left > right ? 1 : 0;
  });
  const byName = new Map<string, Path>();
  for (const manifest of ordered) {
    if (manifest.name !== undefined && !byName.has(manifest.name)) {
      byName.set(manifest.name, unitOf(manifest));
    }
  }

  const installed = new Installed();
  const facts: Fact[] = [];
  const declarations: Declaration[] = [];
  for (const manifest of ordered) {
    const unit = unitOf(manifest);
    const file = unit.join("/");
    // `at` is the directory of the package, when it is installed.
    const add = (kind: DependencyKind, name: string, place: Place, at: string | undefined, note?: string): void => {
      const sibling = byName.get(name);
      const properties = at !== undefined ? installed.package(at).properties : {};
      facts.push({
        unit,
        file,
        line: place.line,
        column: place.column,
        tags: [],
        ...(note !== undefined ? { note } : {}),
        what: {
          type: "reference",
          target: [packageSegment(name)],
          aliases: sibling !== undefined && sibling !== unit ? [sibling] : [],
          kind,
          certainty: "exact",
          ...(Object.keys(properties).length > 0 ? { properties } : {}),
        },
      });
    };
    if (manifest.name !== undefined && manifest.namePlace !== undefined) {
      declarations.push({
        path: [...unit, exportSegment(manifest.name)],
        name: manifest.name,
        unit,
        file,
        line: manifest.namePlace.line,
        column: manifest.namePlace.column,
        tags: [],
        kind: PACKAGE_ITEM_KIND,
        visibility: "exported",
        relations: [],
        modifiers: [],
      });
    }

    const named: { readonly kind: DependencyKind; readonly name: string; readonly place: Place }[] = [];
    for (const [section, kind] of SECTIONS) {
      for (const [name, place] of manifest.places.get(section) ?? []) {
        named.push({ kind, name, place });
      }
    }
    named.sort((a, b) => a.place.line - b.place.line || a.place.column - b.place.column);
    const directory = path.dirname(manifest.file);
    for (const { kind, name, place } of named) {
      add(kind, name, place, installed.resolve(directory, name));
    }

    // What the dependencies need, on the shortest way first. A package is found on the way
    // that says most: first through what is installed with the package, then through what
    // only its development needs.
    // A package is known by its name and its version: another version of a package that
    // the file names is another package. One that is not installed has no version, and
    // then the name stands for all of them, as the name of the own package does.
    const known = new Set<string>();
    const know = (name: string, at: string | undefined): void => {
      known.add(at !== undefined ? `${name}@${installed.package(at).properties["version"] ?? ""}` : name);
    };
    const isKnown = (name: string, at: string): boolean => known.has(name) || known.has(`${name}@${installed.package(at).properties["version"] ?? ""}`);
    for (const { kind, name } of named) {
      if (kind !== "dev-dependency") {
        know(name, installed.resolve(directory, name));
      }
    }
    if (manifest.name !== undefined) {
      known.add(manifest.name);
    }
    const walk = (development: boolean): void => {
      const kind = development ? "transitive-dev-dependency" : "transitive-dependency";
      const seen = new Set<string>([realpathSync(directory)]);
      const queue: { readonly at: string; readonly place: Place; readonly through: readonly string[] }[] = [];
      for (const { kind: of, name, place } of named) {
        const at = (of === "dev-dependency") === development ? installed.resolve(directory, name) : undefined;
        if (at !== undefined && !seen.has(at)) {
          seen.add(at);
          queue.push({ at, place, through: [name] });
        }
      }
      for (let next = 0; next < queue.length; next++) {
        const { at, place, through } = queue[next] as (typeof queue)[number];
        for (const name of installed.package(at).needs) {
          const needed = installed.resolve(at, name);
          if (needed === undefined || seen.has(needed)) {
            continue;
          }
          seen.add(needed);
          if (!isKnown(name, needed)) {
            know(name, needed);
            add(kind, name, place, needed, `through ${through.map((one) => `\`${one}\``).join(", ")}`);
          }
          queue.push({ at: needed, place, through: [...through, name] });
        }
      }
    };
    walk(false);
    for (const { kind, name } of named) {
      if (kind === "dev-dependency") {
        know(name, installed.resolve(directory, name));
      }
    }
    walk(true);
  }
  return { units: ordered.map((manifest) => ({ path: unitOf(manifest), lines: manifest.lines })), facts, declarations };
}
