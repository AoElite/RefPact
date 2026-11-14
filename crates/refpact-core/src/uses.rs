//! The constraints that look at one fact at a time (SPEC 4.2 to 4.4, 4.6 and 4.11).

use crate::check::{Checker, NoReferences, Scope, Seen, Setup};
use crate::cycles;
use crate::error::Role;
use crate::model::{RawPattern, Targets};
use crate::pattern::{Path, Pattern};
use crate::violation::{Detail, Violation};

/// The detail of a use of `hit` by a reference.
fn uses(hit: &Path, seen: &Seen) -> Detail {
    let reference = seen.reference;
    Detail::Uses {
        item: None,
        target: hit.clone(),
        written: (hit != &reference.target).then(|| reference.target.clone()),
        kind: reference.kind.clone(),
        certainty: reference.certainty,
    }
}

/// One violation for each reference of a subject that `breaks` gives a detail for.
fn each_reference(scope: &Scope, breaks: impl Fn(&Seen) -> Option<Detail>) -> Vec<Violation> {
    scope
        .references()
        .filter(|seen| scope.selects(seen.unit))
        .filter_map(|seen| Some(scope.violation_at(seen.fact, breaks(&seen)?)))
        .collect()
}

/// SPEC 4.2: no reference may lead to one of the targets, outside its own unit.
pub(crate) fn not_use<'c>(targets: &'c Targets, setup: &mut Setup) -> Box<dyn Checker + 'c> {
    let targets = setup.targets(targets);
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
        each_reference(scope, |seen| Some(uses(scope.hit(seen, &targets)?, seen)))
    })
}

/// SPEC 4.3: every reference must lead to one of the targets, or stay inside its own unit.
pub(crate) fn only_use<'c>(targets: &'c Targets, setup: &mut Setup) -> Box<dyn Checker + 'c> {
    let targets = setup.targets(targets);
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
        each_reference(scope, |seen| {
            let allowed = scope.owners(seen).any(|(path, owner)| {
                owner == Some(seen.unit)
                    || targets.iter().any(|pattern| pattern.matches_within(path))
            });
            if allowed {
                return None;
            }
            let reference = seen.reference;
            Some(Detail::Uses {
                item: None,
                target: reference.target.clone(),
                written: None,
                kind: reference.kind.clone(),
                certainty: reference.certainty,
            })
        })
    })
}

/// SPEC 4.4: the units must not contain one of the markers.
pub(crate) fn not_contain<'c>(markers: &'c [String], setup: &mut Setup) -> Box<dyn Checker + 'c> {
    setup.markers(markers);
    Box::new(NoReferences(move |scope: &Scope<'_>| -> Vec<Violation> {
        scope
            .markers()
            .filter(|seen| markers.iter().any(|marker| marker == seen.marker))
            .map(|seen| {
                let detail = Detail::Contains {
                    marker: seen.marker.to_owned(),
                };
                scope.violation_at(seen.fact, detail)
            })
            .collect()
    }))
}

/// SPEC 4.6: no reference may lead from one slice to another slice.
pub(crate) fn independent<'c>(
    slices: &'c [RawPattern],
    setup: &mut Setup,
) -> Box<dyn Checker + 'c> {
    let slices = setup.slices(slices);
    cycles::at_least_two_nodes(Some(&slices), setup);
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
        let units = &scope.facts().units;
        each_reference(scope, |seen| {
            let own = slices.of(&seen.fact.unit)?;
            let (hit, _) = scope.owners(seen).find(|(_, owner)| {
                owner
                    .filter(|&owner| scope.selects(owner))
                    .and_then(|owner| slices.of(&units[owner]))
                    .is_some_and(|other| other != own)
            })?;
            Some(uses(hit, seen))
        })
    })
}

/// SPEC 4.11: the references to the protected units from units that are neither protected
/// nor users.
pub(crate) fn only_used_by<'c>(
    users: &'c [RawPattern],
    setup: &mut Setup,
) -> Box<dyn Checker + 'c> {
    let users: Vec<Pattern> = setup.units(users, Role::User);
    Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
        let units = &scope.facts().units;
        scope
            .references()
            .filter(|seen| {
                !scope.selects(seen.unit)
                    && !users.iter().any(|user| user.matches(&units[seen.unit]))
            })
            .filter_map(|seen| {
                let (hit, _) = scope
                    .owners(&seen)
                    .find(|(_, owner)| owner.is_some_and(|owner| scope.selects(owner)))?;
                Some(scope.violation_at(seen.fact, uses(hit, &seen)))
            })
            .collect()
    })
}
