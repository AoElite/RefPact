//! Language-neutral core of refpact.
//!
//! The core evaluates a [`Rule`] against a [`FactSet`]. It does not know the syntax of any
//! language. A front end makes the facts, and a binding makes the rules.
//!
//! The constraint of a rule is a [`Check`]. The built-in constraints and the checks of a user,
//! in [`Constraint::Custom`], go through the same steps: see the module of [`Check`].
//!
//! This is the only implementation of the core. The Rust crate `refpact` uses it directly,
//! and a binding in another language calls it through the JSON of the [`wire`] module, for
//! example in the WebAssembly build of the crate `refpact-wasm`.
//!
//! The behaviour is specified in `conformance/SPEC.md`. The cases in `conformance/cases` are
//! run against it from Rust and through the TypeScript package.

#![warn(missing_docs)]

mod baseline;
mod check;
mod checks;
mod cycles;
mod demands;
mod diagram;
mod error;
mod eval;
mod filters;
mod graph;
mod index;
mod item_check;
mod items;
mod lengths;
mod model;
mod pattern;
mod reach;
mod report;
mod rule;
mod uses;
mod violation;
pub mod wire;

pub use baseline::{
    apply_baseline, baseline_entries, baseline_key, format_baseline, parse_baseline, render_stale,
    Applied, BaselineEntry, BaselineError, StaleEntry,
};
pub use check::{Check, Checker, Custom, NoReferences, Scope, Seen, SeenMarker, Setup};
pub use diagram::{render_stale_diagram, DiagramFormat, DiagramLayer, DiagramNodes};
pub use error::{ErrorCode, Role, RuleError};
pub use eval::{evaluate, Prepared};
pub use graph::{Chain, Edge, Graph, Search, Slices, Tangle};
pub use item_check::{CustomItems, ItemCheck, ItemChecker, ItemScope};
pub use model::{
    Candidates, Certainty, Declaration, Fact, FactSet, KindFilter, Property, RawPattern, Reference,
    Relation, Targets, What,
};
pub use pattern::{is_prefix, join, split, Path, Pattern};
pub use report::{render_error, render_errors, render_violation, render_violations, PathFormat};
pub use rule::{Constraint, Demand, Exemption, Include, Items, Related, Rule, Severity, Subjects};
pub use violation::{Detail, End, Step, Violation};
