/**
 * Architecture tests for TypeScript 6 and 7 projects: rules about what files may import, checked
 * as ordinary tests.
 *
 * ```ts
 * import { allExcept, anyOf, loadProject, pkg, rule } from "@aoelite/refpact";
 *
 * const app = loadProject("tsconfig.json");
 *
 * rule("the user interface does not import the database")
 *   .files(anyOf("src/ui/**"))
 *   .mustNotImport(anyOf("src/db/**", pkg("pg")))
 *   .check(app);
 * ```
 */
export { Baseline, baseline } from "./baseline.ts";
export { Graph, ItemScope, Scope } from "./custom.ts";
export type { ImportEdge, SeenImport, SeenMarker } from "./custom.ts";
export { compilerVersion } from "./frontend/compiler.ts";
export type { Filters } from "./filters.ts";
export { Rules, RuleSet, rules } from "./group.ts";
export { ItemSelection } from "./items.ts";
export type { ItemKind, Modifier, Visibility } from "./items.ts";
export { Layers, layers } from "./layers.ts";
export { loadProject, Project } from "./frontend/project.ts";
export { Onion, onionArchitecture } from "./onion.ts";
export type { LoadOptions } from "./frontend/project.ts";
export type { ComponentExtension } from "./frontend/components.ts";
export { DEPENDENCY_KINDS } from "./frontend/dependencies.ts";
export type { DependencyKind } from "./frontend/dependencies.ts";
export {
  all,
  allExcept,
  AnyOf,
  anyOf,
  ArchitectureError,
  Files,
  globalApi,
  ImportRule,
  KINDS,
  MARKERS,
  NamedRule,
  PackageTarget,
  pkg,
  rule,
  ScopedRule,
  Targets,
  unverified,
} from "./rules.ts";
export type { CheckPatterns, Failure, Kind, Marker, Target } from "./rules.ts";
export type { Detail, DiagramFormat, DiagramLayer, DiagramNodes, End, ErrorCode, FactSet, Role, RuleError, StaleEntry, Step, Violation } from "./core/index.ts";

/** The language-neutral core: the rule model and its evaluation. */
export * as core from "./core/index.ts";
