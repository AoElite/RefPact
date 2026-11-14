//! The request `select` (SPEC section 8): a check of a binding in another language. The core
//! applies the subjects, the filters and the exemptions of the rule, and answers with what
//! the rule sees, so that the binding finds the violations itself.

use std::cell::RefCell;
use std::fmt::Write;

use crate::check::{Check, Checker, Scope, Setup};
use crate::error::Role;
use crate::item_check::{ItemCheck, ItemChecker, ItemScope};
use crate::items;
use crate::model::{RawPattern, Targets};
use crate::pattern::Pattern;
use crate::rule::Items;
use crate::violation::Violation;

/// The check of `select`. It finds no violation, and keeps what the rule sees as JSON text.
pub(super) struct Selection {
    /// Lists of targets. The answer marks the first path of each reference that a list
    /// matches.
    pub(super) targets: Vec<Targets>,
    /// Lists of unit patterns. The answer has the units that each list matches.
    pub(super) units: Vec<Vec<RawPattern>>,
    /// The filters of a rule about items.
    pub(super) items: Option<Items>,
    answer: RefCell<String>,
}

/// The valid patterns of a selection.
struct Lists {
    targets: Vec<Vec<Pattern>>,
    units: Vec<Vec<Pattern>>,
}

impl Selection {
    pub(super) fn new(
        targets: Vec<Targets>,
        units: Vec<Vec<RawPattern>>,
        items: Option<Items>,
    ) -> Selection {
        Selection {
            targets,
            units,
            items,
            answer: RefCell::new(String::new()),
        }
    }

    /// The answer, after the evaluation.
    pub(super) fn answer(&self) -> String {
        self.answer.take()
    }

    fn lists(&self, setup: &mut Setup<'_>) -> Lists {
        Lists {
            targets: self
                .targets
                .iter()
                .map(|targets| setup.targets(targets))
                .collect(),
            units: self
                .units
                .iter()
                .map(|units| setup.units(units, Role::Place))
                .collect(),
        }
    }
}

impl Check for Selection {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        if let Some(filters) = &self.items {
            return items::prepare_with(filters, self, setup);
        }
        let lists = self.lists(setup);
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            *self.answer.borrow_mut() = text(scope, &lists, None);
            Vec::new()
        })
    }
}

impl ItemCheck for Selection {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn ItemChecker + 'c> {
        let lists = self.lists(setup);
        Box::new(move |items: &ItemScope<'_>| -> Vec<Violation> {
            *self.answer.borrow_mut() = text(items, &lists, Some(items));
            Vec::new()
        })
    }
}

/// The check of `graph`: a graph of what a rule sees, as JSON text, so that a binding has
/// the edges and the tangles of the core. A binding asks for it only when its check uses
/// the graph.
pub(super) struct GraphAnswer {
    /// The graph of the subjects (SPEC 4.5), and not the unit graph (SPEC 4.8).
    subjects: bool,
    /// The tangles of the graph, and not its edges: a check that does not use the tangles
    /// does not pay for their search.
    tangles: bool,
    answer: RefCell<String>,
}

impl GraphAnswer {
    pub(super) fn new(subjects: bool, tangles: bool) -> GraphAnswer {
        GraphAnswer {
            subjects,
            tangles,
            answer: RefCell::new(String::new()),
        }
    }

    /// The answer, after the evaluation.
    pub(super) fn answer(&self) -> String {
        self.answer.take()
    }
}

impl Check for GraphAnswer {
    fn prepare<'c>(&'c self, _: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            *self.answer.borrow_mut() = graph_text(scope, self.subjects, self.tangles);
            Vec::new()
        })
    }
}

/// `{ ok: true, nodes, edges }`, or with `tangles` `{ ok: true, tangles }`. `nodes` has the
/// unit of each node of the graph of the subjects; a node of the unit graph is its unit, so
/// that graph has no `nodes`. An edge is `[to, fact, path]`, with the place of its path in
/// the paths of the fact.
fn graph_text(scope: &Scope, subjects: bool, tangles: bool) -> String {
    let graph = if subjects {
        scope.graph(None)
    } else {
        scope.unit_graph()
    };
    let mut text = String::from(r#"{"ok":true,"#);
    if tangles {
        text.push_str(r#""tangles":["#);
        for (index, tangle) in graph.tangles().iter().enumerate() {
            if index > 0 {
                text.push(',');
            }
            text.push_str(r#"{"members":["#);
            numbers(&mut text, tangle.members.iter().copied());
            text.push_str(r#"],"cycle":["#);
            for (number, (node, edge)) in tangle.cycle.iter().enumerate() {
                if number > 0 {
                    text.push(',');
                }
                let _ = write!(text, "[{node},{edge}]");
            }
            text.push_str("]}");
        }
        text.push_str("]}");
        return text;
    }
    if subjects {
        text.push_str(r#""nodes":["#);
        let units = (0..graph.len()).map(|node| {
            scope
                .unit(graph.node(node))
                .expect("a node of the graph of the subjects is a unit")
        });
        numbers(&mut text, units);
        text.push_str("],");
    }
    text.push_str(r#""edges":["#);
    for node in 0..graph.len() {
        if node > 0 {
            text.push(',');
        }
        text.push('[');
        for (number, edge) in graph.edges(node).iter().enumerate() {
            if number > 0 {
                text.push(',');
            }
            let crate::What::Reference(reference) = &graph.fact(node, number).what else {
                unreachable!("an edge comes from a reference")
            };
            let path = reference
                .paths()
                .position(|path| std::ptr::eq(path, edge.target))
                .expect("the path of an edge is a path of its reference");
            let _ = write!(text, "[{},{},{path}]", edge.to, edge.fact);
        }
        text.push(']');
    }
    text.push_str("]}");
    text
}

/// Numbers with commas between them.
fn numbers(text: &mut String, items: impl Iterator<Item = usize>) {
    for (index, item) in items.enumerate() {
        if index > 0 {
            text.push(',');
        }
        let _ = write!(text, "{item}");
    }
}

/// `{ ok: true, subjects, units, references, markers, declarations, items }`, by number and
/// position. The text is written directly: a large fact set has many references.
fn text(scope: &Scope, lists: &Lists, items: Option<&ItemScope>) -> String {
    let facts = scope.facts();
    let mut text = String::from(r#"{"ok":true,"subjects":["#);
    numbers(&mut text, scope.subjects());
    text.push_str(r#"],"units":["#);
    for (index, patterns) in lists.units.iter().enumerate() {
        if index > 0 {
            text.push(',');
        }
        text.push('[');
        let matched = (0..facts.units.len()).filter(|&unit| {
            patterns
                .iter()
                .any(|pattern| pattern.matches(&facts.units[unit]))
        });
        numbers(&mut text, matched);
        text.push(']');
    }
    text.push_str(r#"],"references":["#);
    // The references inside the selected items are some of the references, in their order.
    let mut inside = items.map(|items| items.item_references().peekable());
    let mut hits: Vec<Option<usize>> = vec![None; lists.targets.len()];
    for (index, seen) in scope.references().enumerate() {
        if index > 0 {
            text.push(',');
        }
        let _ = write!(
            text,
            r#"{{"fact":{},"unit":{},"owners":["#,
            seen.position, seen.unit
        );
        hits.fill(None);
        for (number, (path, owner)) in scope.owners(&seen).enumerate() {
            if number > 0 {
                text.push(',');
            }
            match owner {
                Some(owner) => {
                    let _ = write!(text, "{owner}");
                }
                None => text.push_str("null"),
            }
            for (hit, targets) in hits.iter_mut().zip(&lists.targets) {
                if hit.is_none() && targets.iter().any(|pattern| pattern.matches_within(path)) {
                    *hit = Some(number);
                }
            }
        }
        text.push(']');
        if hits.iter().any(Option::is_some) {
            text.push_str(r#","hits":["#);
            for (number, hit) in hits.iter().enumerate() {
                if number > 0 {
                    text.push(',');
                }
                match hit {
                    Some(hit) => {
                        let _ = write!(text, "{hit}");
                    }
                    None => text.push_str("null"),
                }
            }
            text.push(']');
        }
        if let Some(inside) = &mut inside {
            if inside
                .next_if(|next| next.position == seen.position)
                .is_some()
            {
                text.push_str(r#","inside":true"#);
            }
        }
        text.push('}');
    }
    text.push_str(r#"],"markers":["#);
    numbers(&mut text, scope.markers().map(|seen| seen.position));
    text.push_str(r#"],"declarations":["#);
    // The declarations that the rule sees are some of the declarations, in their order, and
    // so are the selected items.
    let positions = |text: &mut String, some: &mut dyn Iterator<Item = &crate::Declaration>| {
        let mut all = facts.declarations.iter().enumerate();
        let found = some.filter_map(|wanted| {
            all.find(|(_, declaration)| std::ptr::eq(*declaration, wanted))
                .map(|(position, _)| position)
        });
        numbers(text, found);
    };
    positions(&mut text, &mut scope.declarations());
    text.push(']');
    if let Some(items) = items {
        text.push_str(r#","items":["#);
        positions(&mut text, &mut items.items());
        text.push(']');
    }
    text.push('}');
    text
}
