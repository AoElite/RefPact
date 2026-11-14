//! Searches in the unit graph (SPEC sections 4.7 to 4.10).

use crate::check::{Checker, NoReferences, Scope, Setup};
use crate::error::{ErrorCode, Role};
use crate::model::{RawPattern, Targets, What};
use crate::pattern::{is_prefix, Path, Pattern};
use crate::violation::{Detail, End, Violation};

/// SPEC 4.7: each unit must be used from outside itself and the units below it.
pub(crate) fn used<'c>(_: &mut Setup) -> Box<dyn Checker + 'c> {
    Box::new(unused)
}

/// SPEC 4.8: no chain of references may lead from the units to one of the targets.
pub(crate) fn not_reach<'c>(targets: &'c Targets, setup: &mut Setup) -> Box<dyn Checker + 'c> {
    let targets = setup.targets(targets);
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> { reaches(scope, &targets) })
}

/// SPEC 4.9: a chain of references must lead from one of the roots to each unit.
pub(crate) fn reachable_from<'c>(
    roots: &'c [RawPattern],
    setup: &mut Setup,
) -> Box<dyn Checker + 'c> {
    let roots = setup.units(roots, Role::Root);
    let units = &setup.facts().units;
    let only_roots = setup
        .subjects()
        .all(|unit| roots.iter().any(|pattern| pattern.matches(&units[unit])));
    if !setup.has_errors() && only_roots {
        setup.error(ErrorCode::NoSubjects, None, None);
    }
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> { unreachable(scope, &roots) })
}

/// The note of a unit that is in no place, below a unit that is the end of a place (SPEC 4.10).
const ABOVE_IS_IN_A_PLACE: &str =
    "a place matches a unit above it: a pattern that ends in `**` also matches the units below";

/// SPEC 4.10: each unit must be matched exactly by one of the places.
pub(crate) fn in_places<'c>(places: &'c [RawPattern], setup: &mut Setup) -> Box<dyn Checker + 'c> {
    let places = setup.units(places, Role::Place);
    Box::new(NoReferences(move |scope: &Scope<'_>| -> Vec<Violation> {
        let units = &scope.facts().units;
        let in_place = |unit: usize| places.iter().any(|place| place.matches(&units[unit]));
        let mut violations = Vec::new();
        // For each unit: whether a unit below is in a place. Then the places divide what is
        // below the unit, and a pattern with `**` at the unit would take all of it. Only a
        // violation needs it, so a rule that passes does not pay for it.
        let mut placed_below: Option<Vec<bool>> = None;
        for position in scope.subjects() {
            let Some(unit) = scope.unit(&units[position]).filter(|&unit| !in_place(unit)) else {
                continue;
            };
            let placed_below = placed_below.get_or_insert_with(|| {
                let mut below = vec![false; units.len()];
                for unit in (0..units.len()).filter(|&unit| in_place(unit)) {
                    for &above in &scope.unit_prefixes(unit)[1..] {
                        below[above] = true;
                    }
                }
                below
            });
            let mut violation = scope.violation_of_unit(position, Detail::Outside);
            // A place that ends at a unit above is most often a pattern without `**`.
            let ends_above = |&above: &usize| in_place(above) && !placed_below[above];
            if scope.unit_prefixes(unit)[1..].iter().any(ends_above) {
                violation.note = Some(ABOVE_IS_IN_A_PLACE.to_owned());
            }
            violations.push(violation);
        }
        violations
    }))
}

/// SPEC 4.7: one violation for each selected unit that nothing outside its subtree uses.
fn unused(scope: &Scope) -> Vec<Violation> {
    let facts = scope.facts();
    // A unit is used when a reference from outside its subtree has a path whose owner is in
    // the subtree. The units with an owner in their subtree are the owner and the units that
    // are a prefix of it.
    let mut used = vec![false; facts.units.len()];
    // For each owner: the unit of the last reference that was counted for it. The
    // references of one unit follow each other, so this skips most repeats.
    let mut last = vec![usize::MAX; facts.units.len()];
    for reference in scope.references() {
        let from = reference.unit;
        for (_, owner) in scope.owners(&reference) {
            let Some(owner) = owner else {
                continue;
            };
            if std::mem::replace(&mut last[owner], from) == from {
                continue;
            }
            for &unit in scope.unit_prefixes(owner) {
                if !is_prefix(&facts.units[unit], &reference.fact.unit) {
                    used[unit] = true;
                }
            }
        }
    }
    of_units(scope, Detail::Unused, |unit| !used[unit])
}

/// One violation for each selected unit that `fails`, in the order of the units. A unit is
/// given by the number that stands for its path.
fn of_units(scope: &Scope, detail: Detail, fails: impl Fn(usize) -> bool) -> Vec<Violation> {
    let units = &scope.facts().units;
    scope
        .subjects()
        .filter(|&position| scope.unit(&units[position]).is_some_and(&fails))
        .map(|position| scope.violation_of_unit(position, detail.clone()))
        .collect()
}

/// SPEC 4.8: one violation for each selected unit from which a chain leads to a target.
fn reaches(scope: &Scope, targets: &[Pattern]) -> Vec<Violation> {
    let facts = scope.facts();
    let graph = scope.unit_graph();
    // For each unit that touches a target: its end, as the index of the fact and the end path.
    let mut ends: Vec<Option<(usize, &Path)>> = vec![None; facts.units.len()];
    for seen in scope.references() {
        let unit = seen.unit;
        if ends[unit].is_none() {
            ends[unit] = scope.hit(&seen, targets).map(|path| (seen.position, path));
        }
    }
    let mut search = graph.search(|node| ends[node].is_some());

    let mut violations = Vec::new();
    for (position, unit) in facts.units.iter().enumerate() {
        let Some(start) = scope.unit(unit) else {
            continue;
        };
        if !scope.selects(position) {
            continue;
        }
        let Some(chain) = search.chain(start) else {
            continue;
        };
        let (end_fact, path) = ends[chain.end].expect("the chain leads to an end");
        let end = &facts.facts[end_fact];
        let What::Reference(reference) = &end.what else {
            unreachable!("an end is a reference")
        };
        // The place of the violation is where the chain starts.
        let place = chain
            .steps
            .first()
            .map_or(end, |&(from, edge)| graph.fact(from, edge));
        let detail = Detail::Reaches {
            steps: chain
                .steps
                .iter()
                .map(|&(from, edge)| graph.step(from, edge))
                .collect(),
            end: End {
                unit: end.unit.clone(),
                file: end.file.clone(),
                line: end.line,
                column: end.column,
                target: path.clone(),
                kind: reference.kind.clone(),
            },
        };
        violations.push(Violation {
            unit: unit.clone(),
            ..scope.violation_at(place, detail)
        });
    }
    violations
}

/// SPEC 4.9: one violation for each selected unit that no root reaches.
fn unreachable(scope: &Scope, roots: &[Pattern]) -> Vec<Violation> {
    let facts = scope.facts();
    let is_root = |unit: &Path| roots.iter().any(|pattern| pattern.matches(unit));
    let reached = scope
        .unit_graph()
        .reached_from(|node| is_root(&facts.units[node]));
    // A unit is reachable when a unit of its subtree is reached: each reached unit makes
    // itself and the units that are a prefix of it reachable.
    let mut reachable = vec![false; facts.units.len()];
    for (node, _) in reached.iter().enumerate().filter(|(_, &reached)| reached) {
        for &unit in scope.unit_prefixes(node) {
            reachable[unit] = true;
        }
    }
    of_units(scope, Detail::Unreachable, |unit| {
        !is_root(&facts.units[unit]) && !reachable[unit]
    })
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;

    use crate::model::{Certainty, Fact, FactSet, Reference, Targets, What};
    use crate::rule::{Constraint, Include, Rule, Subjects};
    use crate::violation::Detail;

    /// The units of the chain that a search without any short cut gives: breadth-first from
    /// `start` over all edges in their order, to the first unit that is an end.
    fn plain_chain(start: usize, edges: &[Vec<usize>], ends: &[bool]) -> Option<Vec<usize>> {
        let mut found_by: Vec<Option<usize>> = vec![None; edges.len()];
        let mut queue = VecDeque::from([start]);
        while let Some(node) = queue.pop_front() {
            if ends[node] {
                let mut chain = vec![node];
                while let Some(from) = found_by[*chain.last().expect("not empty")] {
                    chain.push(from);
                }
                chain.reverse();
                return Some(chain);
            }
            for &to in &edges[node] {
                if to != start && found_by[to].is_none() {
                    found_by[to] = Some(node);
                    queue.push_back(to);
                }
            }
        }
        None
    }

    #[test]
    fn the_search_gives_the_chain_of_a_plain_search_on_many_graphs() {
        let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
        let mut next = |below: usize| {
            state = state
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            (state >> 33) as usize % below
        };
        let mut chains = 0;
        for round in 0..300 {
            let count = 2 + next(14);
            let name = |unit: usize| vec![format!("u{unit}")];
            let mut facts = Vec::new();
            let mut edges: Vec<Vec<usize>> = vec![Vec::new(); count];
            let mut ends = vec![false; count];
            let reference = |from: usize, line: usize, target: Vec<String>| Fact {
                unit: name(from),
                file: format!("u{from}.rs"),
                line: line as u32,
                column: 1,
                tags: Vec::new(),
                note: None,
                item: None,
                what: What::Reference(Reference {
                    target,
                    aliases: Vec::new(),
                    kind: "use".into(),
                    certainty: Certainty::Exact,
                    properties: Vec::new(),
                }),
            };
            for line in 0..next(count * 3 + 1) {
                let from = next(count);
                if next(8) == 0 {
                    ends[from] = true;
                    facts.push(reference(from, line, vec!["std".into(), "fs".into()]));
                } else {
                    let to = next(count);
                    if to != from && !edges[from].contains(&to) {
                        edges[from].push(to);
                    }
                    facts.push(reference(from, line, vec![format!("u{to}"), "item".into()]));
                }
            }
            let set = FactSet {
                separator: "::".into(),
                units: (0..count).map(name).collect(),
                kinds: vec!["use".into()],
                facts,
                ..FactSet::default()
            };
            let rule = Rule::new(
                "r",
                Subjects {
                    include: Include::All,
                    except: None,
                },
                Constraint::MustNotReach(Targets {
                    any_of: vec![vec!["std".into(), "fs".into()]],
                    unverified: Vec::new(),
                }),
            );
            let violations = crate::evaluate(&rule, &set).expect("the rule is valid");
            let expected: Vec<Vec<usize>> = (0..count)
                .filter_map(|start| plain_chain(start, &edges, &ends))
                .collect();
            let found: Vec<Vec<usize>> = violations
                .iter()
                .map(|violation| {
                    let Detail::Reaches { steps, end } = &violation.detail else {
                        panic!("not a chain")
                    };
                    let number =
                        |unit: &[String]| unit[0][1..].parse::<usize>().expect("a unit name");
                    let mut chain: Vec<usize> =
                        steps.iter().map(|step| number(&step.from)).collect();
                    chain.push(number(&end.unit));
                    assert_eq!(number(&violation.unit), chain[0], "round {round}");
                    chain
                })
                .collect();
            assert_eq!(found, expected, "round {round}");
            chains += found.iter().filter(|chain| chain.len() > 2).count();
        }
        assert!(
            chains > 100,
            "the graphs gave only {chains} chains of two steps or more"
        );
    }
}
