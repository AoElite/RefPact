//! The demands of the rules about items (SPEC 4.12), each a check of items.

use crate::check::{NoReferences, Setup};
use crate::error::{ErrorCode, Role};
use crate::item_check::{ItemChecker, ItemScope};
use crate::model::{Declaration, RawPattern, Targets};
use crate::pattern::Pattern;
use crate::violation::{Detail, Violation};

/// One violation for each selected declaration that does not meet the demand.
fn each_item(items: &ItemScope, meets: impl Fn(&Declaration) -> bool) -> Vec<Violation> {
    items
        .items()
        .filter(|declaration| !meets(declaration))
        .map(|declaration| {
            let detail = Detail::Item {
                item: declaration.path.clone(),
                kind: declaration.kind.clone(),
            };
            items.violation_of_item(declaration, detail)
        })
        .collect()
}

/// A checker of declarations, which reads no references.
fn of_items<'c>(meets: impl Fn(&Declaration) -> bool + 'c) -> Box<dyn ItemChecker + 'c> {
    Box::new(NoReferences(
        move |items: &ItemScope<'_>| -> Vec<Violation> { each_item(items, &meets) },
    ))
}

/// No declaration may be selected.
pub(crate) fn not_exist<'c>(_: &mut Setup) -> Box<dyn ItemChecker + 'c> {
    of_items(|_| false)
}

/// The unit of each declaration must be matched exactly by one of the places.
pub(crate) fn reside_in<'c>(places: &[RawPattern], setup: &mut Setup) -> Box<dyn ItemChecker + 'c> {
    let places = setup.units(places, Role::Place);
    of_items(move |declaration| places.iter().any(|place| place.matches(&declaration.unit)))
}

/// A glob must match the name of each declaration.
pub(crate) fn be_named<'c>(names: &[String], setup: &mut Setup) -> Box<dyn ItemChecker + 'c> {
    let globs = setup.names(names, Role::RequiredName, false);
    of_items(move |declaration| {
        globs
            .iter()
            .any(|glob| glob.matches(std::slice::from_ref(&declaration.name)))
    })
}

/// Each declaration must have one of the modifiers.
pub(crate) fn have_modifier<'c>(
    modifiers: &'c [String],
    setup: &mut Setup,
) -> Box<dyn ItemChecker + 'c> {
    setup.modifiers(modifiers, Role::RequiredModifier);
    of_items(move |declaration| {
        declaration
            .modifiers
            .iter()
            .any(|modifier| modifiers.contains(modifier))
    })
}

/// Each declaration must have the relation to one of the paths.
pub(crate) fn have<'c>(
    relation: &'c str,
    paths: &[RawPattern],
    setup: &mut Setup,
) -> Box<dyn ItemChecker + 'c> {
    setup.relation(relation);
    if paths.is_empty() {
        setup.error(ErrorCode::EmptyList, Some(Role::Required), None);
    }
    let targets = setup.paths(paths, Role::Required);
    of_items(move |declaration| {
        declaration.relations.iter().any(|tie| {
            tie.relation == relation
                && targets
                    .iter()
                    .any(|pattern| pattern.matches_within(&tie.target))
        })
    })
}

/// The targets of a demand about uses. Without facts that say which item holds them, the
/// rule cannot fail.
fn use_targets(targets: &Targets, setup: &mut Setup) -> Vec<Pattern> {
    let targets = setup.targets(targets);
    let facts = setup.facts();
    if !setup.has_errors() && facts.facts.iter().all(|fact| fact.item.is_none()) {
        // The front end does not say which item holds a fact.
        setup.error(ErrorCode::NoSubjects, None, None);
    }
    targets
}

/// The detail of a use of `hit` by a reference inside an item.
fn uses(hit: &[String], seen: &crate::check::Seen) -> Detail {
    let reference = seen.reference;
    Detail::Uses {
        item: seen.fact.item.clone(),
        target: hit.to_vec(),
        written: (hit != reference.target.as_slice()).then(|| reference.target.clone()),
        kind: reference.kind.clone(),
        certainty: reference.certainty,
    }
}

/// No reference inside a declaration may lead to one of the targets. Unlike a rule about
/// units, a path of the own unit can break the rule.
pub(crate) fn not_use<'c>(targets: &Targets, setup: &mut Setup) -> Box<dyn ItemChecker + 'c> {
    let targets = use_targets(targets, setup);
    Box::new(move |items: &ItemScope<'_>| -> Vec<Violation> {
        items
            .item_references()
            .filter_map(|seen| {
                let (hit, _) = items
                    .owners(&seen)
                    .find(|(path, _)| targets.iter().any(|pattern| pattern.matches_within(path)))?;
                Some(items.violation_at(seen.fact, uses(hit, &seen)))
            })
            .collect()
    })
}

/// Every reference inside a declaration must lead to one of the targets, or stay inside its
/// own unit.
pub(crate) fn only_use<'c>(targets: &Targets, setup: &mut Setup) -> Box<dyn ItemChecker + 'c> {
    let targets = use_targets(targets, setup);
    Box::new(move |items: &ItemScope<'_>| -> Vec<Violation> {
        items
            .item_references()
            .filter(|seen| {
                !items.owners(seen).any(|(path, owner)| {
                    owner == Some(seen.unit)
                        || targets.iter().any(|pattern| pattern.matches_within(path))
                })
            })
            .map(|seen| items.violation_at(seen.fact, uses(&seen.reference.target, &seen)))
            .collect()
    })
}
