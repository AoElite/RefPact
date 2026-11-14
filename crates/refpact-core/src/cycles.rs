//! Cycles between units or slices (SPEC 4.5).

use crate::check::{Checker, Scope, Setup};
use crate::error::ErrorCode;
use crate::graph::Slices;
use crate::model::RawPattern;
use crate::violation::{Detail, Violation};

/// SPEC 4.5: the units, or the slices, must not form a cycle.
pub(crate) fn acyclic<'c>(
    slices: Option<&'c [RawPattern]>,
    setup: &mut Setup,
) -> Box<dyn Checker + 'c> {
    let slices = slices.map(|raw| setup.slices(raw));
    at_least_two_nodes(slices.as_ref(), setup);
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> { tangles(scope, slices.as_ref()) })
}

/// Check 6 of SPEC section 5: a rule about a graph with fewer than two nodes cannot fail.
pub(crate) fn at_least_two_nodes(slices: Option<&Slices>, setup: &mut Setup) {
    if !setup.has_errors() && setup.nodes(slices).len() < 2 {
        setup.error(ErrorCode::TooFewNodes, None, None);
    }
}

/// One violation for each tangle, at its first node, with a shortest cycle through it.
fn tangles(scope: &Scope, slices: Option<&Slices>) -> Vec<Violation> {
    let graph = scope.graph(slices);
    graph
        .tangles()
        .into_iter()
        .map(|tangle| {
            let steps = tangle
                .cycle
                .iter()
                .map(|&(from, edge)| graph.step(from, edge))
                .collect();
            let witness = graph.fact(tangle.members[0], tangle.cycle[0].1);
            let detail = Detail::Cycle {
                steps,
                size: tangle.members.len(),
            };
            Violation {
                unit: graph.node(tangle.members[0]).clone(),
                // The note of the witness is about one reference, not about the tangle.
                note: None,
                ..scope.violation_at(witness, detail)
            }
        })
        .collect()
}
