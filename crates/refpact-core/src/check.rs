//! The interface of every check (SPEC 4.15). The core evaluates all rules in the same way: it
//! selects the subjects, applies the filters and the exemptions, and then asks the check of
//! the constraint for the violations. The built-in constraints are checks of this kind, and a
//! user can write more.
//!
//! A check runs in two steps. [`Check::prepare`] reads the arguments of the check before
//! anything is evaluated, and reports what is wrong with them through [`Setup`]: a pattern
//! that matches nothing, for example. The [`Checker`] that it returns then finds the
//! violations in a [`Scope`]: the facts that the rule sees.
//!
//! ```
//! use refpact_core::{
//!     evaluate, Constraint, Custom, Detail, Fact, FactSet, Include, Rule, Scope, Subjects,
//!     Violation, What,
//! };
//!
//! // Each unit may refer to at most one path.
//! let check = Custom::new(|scope: &Scope<'_>| -> Vec<Violation> {
//!     scope
//!         .subjects()
//!         .filter(|&unit| scope.references().filter(|seen| seen.unit == unit).count() > 1)
//!         .map(|unit| scope.violation_of_unit(unit, Detail::custom("refers to more than one path")))
//!         .collect()
//! });
//! let rule = Rule::new(
//!     "one reference",
//!     Subjects { include: Include::All, except: None },
//!     Constraint::Custom(check),
//! );
//! let facts = FactSet { separator: "::".into(), units: vec![vec!["a".into()]], ..FactSet::default() };
//! assert_eq!(evaluate(&rule, &facts), Ok(Vec::new()));
//! ```

use std::fmt;
use std::sync::Arc;

use crate::checks::{
    error, invalid_pattern, unit_list, unknown_values, use_targets, verified_targets,
};
use crate::error::{ErrorCode, Role, RuleError};
use crate::filters::Filters;
use crate::graph::{self, Edge, Graph, Slices};
use crate::index::{FastMap, Index, NumberSet};
use crate::model::{Declaration, Fact, FactSet, RawPattern, Reference, Targets, What};
use crate::pattern::{join, Path, Pattern};
use crate::violation::{Detail, Violation};

/// What a constraint demands: the arguments of a check, read before the evaluation.
///
/// A closure `Fn(&Scope) -> Vec<Violation>` is a check without arguments.
pub trait Check {
    /// Checks the arguments with `setup`, and returns what finds the violations. An error
    /// that `setup` records stops the evaluation before the checker runs.
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c>;
}

/// What finds the violations of a rule, after [`Check::prepare`].
///
/// A closure `Fn(&Scope) -> Vec<Violation>` is a checker.
pub trait Checker {
    /// Whether the checker reads references. An exemption of a rule whose checker does not
    /// read them lets nothing pass, so it is an error (SPEC 4.13). Default: `true`.
    fn reads_references(&self) -> bool {
        true
    }

    /// The violations, in a stable order.
    fn violations(&self, scope: &Scope<'_>) -> Vec<Violation>;
}

impl<F> Checker for F
where
    F: Fn(&Scope<'_>) -> Vec<Violation>,
{
    fn violations(&self, scope: &Scope<'_>) -> Vec<Violation> {
        self(scope)
    }
}

impl<F> Check for F
where
    F: Fn(&Scope<'_>) -> Vec<Violation>,
{
    fn prepare<'c>(&'c self, _: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        Box::new(self)
    }
}

/// A checker that reads no references, such as one about markers or declarations. An
/// exemption of its rule is an error.
pub struct NoReferences<F>(pub F);

impl<F> Checker for NoReferences<F>
where
    F: Fn(&Scope<'_>) -> Vec<Violation>,
{
    fn reads_references(&self) -> bool {
        false
    }

    fn violations(&self, scope: &Scope<'_>) -> Vec<Violation> {
        (self.0)(scope)
    }
}

/// The check of [`crate::Constraint::Custom`]. Two values are equal when they are clones of
/// one value.
#[derive(Clone)]
pub struct Custom {
    check: Arc<dyn Check + Send + Sync>,
}

impl Custom {
    /// Wraps a check, or a closure `Fn(&Scope) -> Vec<Violation>`.
    pub fn new(check: impl Check + Send + Sync + 'static) -> Custom {
        Custom {
            check: Arc::new(check),
        }
    }
}

impl Check for Custom {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        self.check.prepare(setup)
    }
}

impl fmt::Debug for Custom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Custom(..)")
    }
}

impl PartialEq for Custom {
    fn eq(&self, other: &Custom) -> bool {
        Arc::ptr_eq(&self.check, &other.check)
    }
}

impl Eq for Custom {}

// A check reads the facts and returns violations. When it panics, the evaluation stops and
// nothing of the core is left half done, so a rule stays usable after `catch_unwind`.
impl std::panic::UnwindSafe for Custom {}
impl std::panic::RefUnwindSafe for Custom {}

/// The numbers of the units that a rule selects, in order.
fn selected_units(selected: &[bool]) -> impl Iterator<Item = usize> + '_ {
    (0..selected.len()).filter(move |&unit| selected[unit])
}

/// What [`Check::prepare`] gets: the fact set, the subjects, and the errors of the rule so far.
pub struct Setup<'a> {
    facts: &'a FactSet,
    selected: &'a [bool],
    errors: &'a mut Vec<RuleError>,
}

impl<'a> Setup<'a> {
    pub(crate) fn new(
        facts: &'a FactSet,
        selected: &'a [bool],
        errors: &'a mut Vec<RuleError>,
    ) -> Setup<'a> {
        Setup {
            facts,
            selected,
            errors,
        }
    }

    /// The fact set.
    pub fn facts(&self) -> &'a FactSet {
        self.facts
    }

    /// Whether the rule selects the unit, by its number: its position in the units of the
    /// fact set.
    pub fn selects(&self, unit: usize) -> bool {
        self.selected[unit]
    }

    /// The numbers of the units that the rule selects, in order.
    pub fn subjects(&self) -> impl Iterator<Item = usize> + 'a {
        selected_units(self.selected)
    }

    /// Whether the rule has errors so far. A check that cannot fail when the rest of the rule
    /// is valid reports that only when this is `false`.
    pub fn has_errors(&self) -> bool {
        !self.errors.is_empty()
    }

    /// Records an error. The rule is then not evaluated.
    pub fn error(&mut self, code: ErrorCode, role: Option<Role>, value: Option<String>) {
        self.errors.push(error(code, role, value));
    }

    /// Unit patterns, which match units exactly: an empty list is an error, and so is a
    /// pattern that is not valid or that matches no unit. Returns the valid patterns.
    pub fn units(&mut self, patterns: &[RawPattern], role: Role) -> Vec<Pattern> {
        unit_list(patterns, role, self.facts, self.errors)
    }

    /// Targets, which match paths within: the checks of a rule about uses (SPEC section 5,
    /// checks 7 to 9). Returns the valid patterns of both lists.
    pub fn targets(&mut self, targets: &Targets) -> Vec<Pattern> {
        use_targets(targets, self.facts, self.errors)
    }

    /// Patterns of paths that are checked for existence as targets are (SPEC 5.1). An empty
    /// list is no error. Returns the valid patterns.
    pub fn paths(&mut self, patterns: &[RawPattern], role: Role) -> Vec<Pattern> {
        verified_targets(patterns, role, self.facts, self.errors)
    }

    /// Slice patterns (SPEC 4.5): an empty list is an error, and so is a pattern that is
    /// not valid or that no start of the path of a unit matches.
    pub fn slices(&mut self, patterns: &[RawPattern]) -> Slices {
        let facts = self.facts;
        let separator = facts.separator.as_str();
        if patterns.is_empty() {
            self.error(ErrorCode::EmptyList, Some(Role::Slice), None);
        }
        let mut valid = Vec::new();
        for raw in patterns {
            match Pattern::new(raw) {
                None => self
                    .errors
                    .push(invalid_pattern(raw, Role::Slice, separator)),
                Some(pattern) => {
                    let single = std::slice::from_ref(&pattern);
                    if !facts
                        .units
                        .iter()
                        .any(|unit| graph::slice_of(unit, single).is_some())
                    {
                        self.error(
                            ErrorCode::PatternMatchesNothing,
                            Some(Role::Slice),
                            Some(join(raw, separator)),
                        );
                    }
                    valid.push(pattern);
                }
            }
        }
        Slices::new(valid)
    }

    /// The paths of the nodes of [`Scope::subject_graph`]: the subjects, or their slices. A rule
    /// about a graph with fewer than two nodes cannot fail (SPEC section 5, check 6).
    pub fn nodes(&self, slices: Option<&Slices>) -> Vec<Path> {
        graph::nodes(self.facts, self.selected, slices)
    }

    /// Markers: an empty list is an error, and so is a marker that the front end does not
    /// produce.
    pub fn markers(&mut self, markers: &[String]) {
        if markers.is_empty() {
            self.error(ErrorCode::EmptyList, Some(Role::Marker), None);
        }
        let known = &self.facts.markers;
        unknown_values(
            markers,
            known,
            ErrorCode::UnknownMarker,
            Role::Marker,
            self.errors,
        );
    }

    /// Globs for the names of items: `*` matches any run of characters. An empty list is an
    /// error, and so is a glob that is not valid. With `exist`, a glob that matches the name
    /// of no declaration is an error. Match a name with
    /// `glob.matches(std::slice::from_ref(&declaration.name))`.
    pub fn names(&mut self, globs: &[String], role: Role, exist: bool) -> Vec<Pattern> {
        if globs.is_empty() {
            self.error(ErrorCode::EmptyList, Some(role), None);
        }
        let mut valid = Vec::new();
        for text in globs {
            // A glob is one pattern segment without `**`.
            let glob = if text.is_empty() || text.contains("**") {
                None
            } else {
                Pattern::new(std::slice::from_ref(text))
            };
            match glob {
                None => self.error(ErrorCode::InvalidPattern, Some(role), Some(text.clone())),
                Some(pattern) => {
                    let found = self.facts.declarations.iter().any(|declaration| {
                        pattern.matches(std::slice::from_ref(&declaration.name))
                    });
                    if exist && !found {
                        self.error(
                            ErrorCode::PatternMatchesNothing,
                            Some(role),
                            Some(text.clone()),
                        );
                    }
                    valid.push(pattern);
                }
            }
        }
        valid
    }

    /// Kinds of item: an empty list is an error, and so is a kind that the front end does
    /// not produce.
    pub fn item_kinds(&mut self, kinds: &[String]) {
        if kinds.is_empty() {
            self.error(ErrorCode::EmptyList, Some(Role::ItemKind), None);
        }
        let known = &self.facts.item_kinds;
        unknown_values(
            kinds,
            known,
            ErrorCode::UnknownItemKind,
            Role::ItemKind,
            self.errors,
        );
    }

    /// Visibilities: an empty list is an error, and so is a visibility that the front end
    /// does not produce.
    pub fn visibilities(&mut self, visibilities: &[String]) {
        if visibilities.is_empty() {
            self.error(ErrorCode::EmptyList, Some(Role::Visibility), None);
        }
        let known = &self.facts.visibilities;
        unknown_values(
            visibilities,
            known,
            ErrorCode::UnknownVisibility,
            Role::Visibility,
            self.errors,
        );
    }

    /// Modifiers of items: an empty list is an error, and so is a modifier that the front
    /// end does not produce.
    pub fn modifiers(&mut self, modifiers: &[String], role: Role) {
        if modifiers.is_empty() {
            self.error(ErrorCode::EmptyList, Some(role), None);
        }
        let known = &self.facts.modifiers;
        unknown_values(
            modifiers,
            known,
            ErrorCode::UnknownModifier,
            role,
            self.errors,
        );
    }

    /// A relation of items, such as `implements`: one that the front end does not produce
    /// is an error.
    pub fn relation(&mut self, relation: &str) {
        if !self.facts.relations.iter().any(|known| known == relation) {
            self.error(
                ErrorCode::UnknownRelation,
                Some(Role::Relation),
                Some(relation.to_owned()),
            );
        }
    }

    /// The paths of a relation that selects items: the relation must be known, an empty
    /// list is an error, and so is a pattern that is not valid or that matches within no
    /// target of the relation in the declarations.
    pub fn related(&mut self, relation: &str, paths: &[RawPattern]) -> Vec<Pattern> {
        let facts = self.facts;
        self.relation(relation);
        if paths.is_empty() {
            self.error(ErrorCode::EmptyList, Some(Role::Related), None);
        }
        let mut valid = Vec::new();
        for raw in paths {
            match Pattern::new(raw) {
                None => self
                    .errors
                    .push(invalid_pattern(raw, Role::Related, &facts.separator)),
                Some(pattern) => {
                    let found = facts.declarations.iter().any(|declaration| {
                        declaration.relations.iter().any(|tie| {
                            tie.relation == relation && pattern.matches_within(&tie.target)
                        })
                    });
                    if !found {
                        self.error(
                            ErrorCode::PatternMatchesNothing,
                            Some(Role::Related),
                            Some(join(raw, &facts.separator)),
                        );
                    }
                    valid.push(pattern);
                }
            }
        }
        valid
    }
}

/// A reference that the rule sees: the filters and the exemptions let it pass.
#[derive(Debug, Clone, Copy)]
pub struct Seen<'a> {
    /// The position of the fact in the fact set.
    pub position: usize,
    /// The number of the unit of the fact.
    pub unit: usize,
    /// The fact.
    pub fact: &'a Fact,
    /// The reference of the fact.
    pub reference: &'a Reference,
}

/// A marker that the rule sees: the filters let it pass.
#[derive(Debug, Clone, Copy)]
pub struct SeenMarker<'a> {
    /// The position of the fact in the fact set.
    pub position: usize,
    /// The number of the unit of the fact.
    pub unit: usize,
    /// The fact.
    pub fact: &'a Fact,
    /// The marker.
    pub marker: &'a str,
}

/// What a [`Checker`] gets: the fact set, the subjects, and the facts that the rule sees.
///
/// A unit is given by its number: its position in the units of the fact set. When two units
/// have the same path, the first one stands for both.
#[derive(Clone, Copy)]
pub struct Scope<'a> {
    filters: Filters<'a>,
    index: &'a Index<'a>,
    selected: &'a [bool],
}

impl<'a> Scope<'a> {
    pub(crate) fn new(filters: Filters<'a>, index: &'a Index<'a>, selected: &'a [bool]) -> Self {
        Scope {
            filters,
            index,
            selected,
        }
    }

    /// The name of the rule.
    pub fn rule_name(&self) -> &'a str {
        self.filters.name
    }

    /// The fact set.
    pub fn facts(&self) -> &'a FactSet {
        self.index.facts
    }

    /// Whether the rule selects the unit.
    pub fn selects(&self, unit: usize) -> bool {
        self.selected[unit]
    }

    /// The numbers of the units that the rule selects, in order.
    pub fn subjects(&self) -> impl Iterator<Item = usize> + 'a {
        selected_units(self.selected)
    }

    /// The number of the unit with this path.
    pub fn unit(&self, path: &[String]) -> Option<usize> {
        self.index.unit(path)
    }

    /// The number of the unit of a fact, by the position of the fact.
    pub fn unit_of_fact(&self, position: usize) -> Option<usize> {
        self.index.unit_of_fact(position)
    }

    /// The numbers of the units that are a prefix of a unit, the unit itself first.
    pub fn unit_prefixes(&self, unit: usize) -> &'a [usize] {
        self.index.unit_prefixes(unit)
    }

    /// Whether the rule ignores something with these tags (check 2 of SPEC 4.2).
    pub fn ignores(&self, tags: &[String]) -> bool {
        self.filters.ignores(tags)
    }

    /// Whether the kind and the certainty of the reference pass (checks 3 and 4 of SPEC 4.2).
    pub fn admits(&self, reference: &Reference) -> bool {
        self.filters.admits(reference)
    }

    /// Whether the rule sees the fact, by its position: no ignored tag, and no exemption
    /// lets it pass. The kind and the certainty of a reference are [`Scope::admits`].
    pub fn sees(&self, position: usize) -> bool {
        !self.filters.ignores(&self.facts().facts[position].tags) && !self.index.exempt(position)
    }

    /// The references that the rule sees, of all units, in the order of the facts. Filter
    /// them with [`Scope::selects`] for the references of the subjects.
    pub fn references(&self) -> impl Iterator<Item = Seen<'a>> + 'a {
        let (filters, index) = (self.filters, self.index);
        index
            .facts
            .facts
            .iter()
            .enumerate()
            .filter_map(move |(position, fact)| {
                let What::Reference(reference) = &fact.what else {
                    return None;
                };
                if filters.ignores(&fact.tags)
                    || index.exempt(position)
                    || !filters.admits(reference)
                {
                    return None;
                }
                Some(Seen {
                    position,
                    unit: index.unit_of_fact(position)?,
                    fact,
                    reference,
                })
            })
    }

    /// The markers of the subjects that the rule sees, in the order of the facts.
    pub fn markers(&self) -> impl Iterator<Item = SeenMarker<'a>> + 'a {
        let (filters, index, selected) = (self.filters, self.index, self.selected);
        index
            .facts
            .facts
            .iter()
            .enumerate()
            .filter_map(move |(position, fact)| {
                let What::Marker(marker) = &fact.what else {
                    return None;
                };
                let unit = index.unit_of_fact(position)?;
                if filters.ignores(&fact.tags) || !selected[unit] {
                    return None;
                }
                Some(SeenMarker {
                    position,
                    unit,
                    fact,
                    marker,
                })
            })
    }

    /// The declarations of the subjects that the rule sees, in order.
    pub fn declarations(&self) -> impl Iterator<Item = &'a Declaration> + 'a {
        let (filters, index, selected) = (self.filters, self.index, self.selected);
        index.facts.declarations.iter().filter(move |declaration| {
            index
                .unit(&declaration.unit)
                .is_some_and(|unit| selected[unit])
                && !filters.ignores(&declaration.tags)
        })
    }

    /// The paths of a reference, `[target, aliases..]`, each with its owner: the number of
    /// the longest unit that is a prefix of it.
    pub fn owners(&self, seen: &Seen<'a>) -> impl Iterator<Item = (&'a Path, Option<usize>)> + 'a {
        self.index.owned_paths(seen.position, seen.reference)
    }

    /// Check 5 of SPEC 4.2: the first path of a reference that one of the patterns matches
    /// within, when the unit of the reference is not the owner of the path.
    pub fn hit(&self, seen: &Seen<'a>, patterns: &[Pattern]) -> Option<&'a Path> {
        self.index
            .forbidden(seen.position, Some(seen.unit), seen.reference, patterns)
    }

    /// The edges of a graph: for each node, the nodes that its references lead to, each once,
    /// with the first reference. `node_of` gives the node of a unit, if it has one. A
    /// reference leads to the node of the owner of each of its paths, so a use through a
    /// re-export leads to the unit that re-exports and to the unit of the definition. A
    /// path inside the node of the reference is no edge.
    pub fn edges(
        &self,
        count: usize,
        node_of: impl Fn(usize) -> Option<usize>,
    ) -> Vec<Vec<Edge<'a>>> {
        let mut edges: Vec<Vec<Edge<'a>>> = (0..count).map(|_| Vec::new()).collect();
        let mut known: NumberSet<(usize, usize)> = NumberSet::default();
        for seen in self.references() {
            let Some(from) = node_of(seen.unit) else {
                continue;
            };
            for (target, owner) in self.owners(&seen) {
                let Some(to) = owner.and_then(&node_of) else {
                    continue;
                };
                if to != from && known.insert((from, to)) {
                    edges[from].push(Edge {
                        to,
                        fact: seen.position,
                        target,
                    });
                }
            }
        }
        edges
    }

    /// The edges of the graph with one node for each unit, selected or not. The node of a
    /// unit is its number.
    pub fn unit_edges(&self) -> Vec<Vec<Edge<'a>>> {
        self.edges(self.facts().units.len(), Some)
    }

    /// The graph of all units, selected or not: the node of a unit is its number, and the
    /// edges are [`Scope::unit_edges`].
    pub fn unit_graph(&self) -> Graph<'a> {
        let facts = self.facts();
        Graph::new(facts, facts.units.as_slice(), self.unit_edges())
    }

    /// The graph of the subjects (SPEC 4.5): one node for each subject, or, with slices,
    /// one node for each slice of a subject. A subject in no slice is in no node, and a
    /// reference inside one node is no edge. The nodes are those of [`Setup::nodes`].
    pub fn subject_graph(&self, slices: Option<&Slices>) -> Graph<'a> {
        let facts = self.facts();
        let nodes = graph::nodes(facts, self.selected, slices);
        let numbers: FastMap<&[String], usize> = nodes
            .iter()
            .enumerate()
            .map(|(number, node)| (node.as_slice(), number))
            .collect();
        // The node of each unit, by the number of the unit.
        let node_of_unit: Vec<Option<usize>> = facts
            .units
            .iter()
            .zip(self.selected)
            .map(|(unit, &selected)| {
                let path = graph::node_path(unit, slices).filter(|_| selected)?;
                numbers.get(path).copied()
            })
            .collect();
        let edges = self.edges(nodes.len(), |unit| node_of_unit[unit]);
        drop(numbers);
        Graph::new(facts, nodes, edges)
    }

    /// A violation at the place of a fact, with its note.
    pub fn violation_at(&self, fact: &Fact, detail: Detail) -> Violation {
        Violation {
            rule: self.filters.name.to_owned(),
            unit: fact.unit.clone(),
            file: fact.file.clone(),
            line: fact.line,
            column: fact.column,
            note: fact.note.clone(),
            detail,
        }
    }

    /// A violation at the place of a declaration.
    pub fn violation_of_item(&self, declaration: &Declaration, detail: Detail) -> Violation {
        Violation {
            rule: self.filters.name.to_owned(),
            unit: declaration.unit.clone(),
            file: declaration.file.clone(),
            line: declaration.line,
            column: declaration.column,
            note: None,
            detail,
        }
    }

    /// A violation about a unit as a whole: the file is that of the unit, and the line and
    /// column are 0.
    pub fn violation_of_unit(&self, unit: usize, detail: Detail) -> Violation {
        let path = &self.facts().units[unit];
        Violation::of_unit(self.filters.name, path, self.index, detail)
    }
}
