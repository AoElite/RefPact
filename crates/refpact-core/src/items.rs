//! Rules about declarations (SPEC 4.12): the filters that select the declarations, and the
//! check of the demand, which finds the violations among them.

use crate::check::{Checker, Scope, Setup};
use crate::demands;
use crate::error::Role;
use crate::item_check::{ItemCheck, ItemChecker, ItemScope};
use crate::lengths;
use crate::model::Declaration;
use crate::pattern::Pattern;
use crate::rule::{Demand, Items};
use crate::violation::Violation;

/// The built-in demands, each a check of items like a check of the user.
impl ItemCheck for Demand {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn ItemChecker + 'c> {
        match self {
            Demand::NotExist => demands::not_exist(setup),
            Demand::ResideIn(places) => demands::reside_in(places, setup),
            Demand::BeNamed(names) => demands::be_named(names, setup),
            Demand::Have(related) => demands::have(&related.relation, &related.any_of, setup),
            Demand::HaveModifier(modifiers) => demands::have_modifier(modifiers, setup),
            Demand::NotUse(targets) => demands::not_use(targets, setup),
            Demand::OnlyUse(targets) => demands::only_use(targets, setup),
            Demand::NotExceedLines(limit) => lengths::items(*limit, setup),
            Demand::Custom(custom) => custom.prepare(setup),
        }
    }
}

/// The filters of a rule about items, after the checks of SPEC section 5.
struct Filter<'c> {
    named: Option<Vec<Pattern>>,
    kinds: Option<&'c [String]>,
    visibilities: Option<&'c [String]>,
    related: Vec<(&'c str, Vec<Pattern>)>,
    modifiers: Option<&'c [String]>,
}

impl Filter<'_> {
    /// Whether the filters select a declaration.
    fn selects(&self, declaration: &Declaration) -> bool {
        self.named.as_ref().map_or(true, |globs| {
            globs
                .iter()
                .any(|glob| glob.matches(std::slice::from_ref(&declaration.name)))
        }) && self
            .kinds
            .map_or(true, |kinds| kinds.contains(&declaration.kind))
            && self.visibilities.map_or(true, |visibilities| {
                visibilities.contains(&declaration.visibility)
            })
            && self.modifiers.map_or(true, |modifiers| {
                declaration
                    .modifiers
                    .iter()
                    .any(|modifier| modifiers.contains(modifier))
            })
            && self.related.iter().all(|(relation, targets)| {
                declaration.relations.iter().any(|tie| {
                    tie.relation == *relation
                        && targets
                            .iter()
                            .any(|pattern| pattern.matches_within(&tie.target))
                })
            })
    }
}

/// The filters with the checker of the demand: a checker of the rule.
struct Selected<'c> {
    filter: Filter<'c>,
    checker: Box<dyn ItemChecker + 'c>,
}

impl Checker for Selected<'_> {
    fn reads_references(&self) -> bool {
        self.checker.reads_references()
    }

    fn violations(&self, scope: &Scope<'_>) -> Vec<Violation> {
        let items = ItemScope::new(scope, |declaration| self.filter.selects(declaration));
        self.checker.violations(&items)
    }
}

/// Checks the filters and the demand of a rule about items (SPEC section 5, checks 7 to 11).
pub(crate) fn prepare<'c>(items: &'c Items, setup: &mut Setup) -> Box<dyn Checker + 'c> {
    prepare_with(items, &items.must, setup)
}

/// Like [`prepare`], with another check than the demand of the rule.
pub(crate) fn prepare_with<'c>(
    items: &'c Items,
    check: &'c dyn ItemCheck,
    setup: &mut Setup,
) -> Box<dyn Checker + 'c> {
    let named = items
        .named
        .as_ref()
        .map(|globs| setup.names(globs, Role::Name, true));
    let kinds = items.kinds.as_deref();
    if let Some(kinds) = kinds {
        setup.item_kinds(kinds);
    }
    let visibilities = items.visibilities.as_deref();
    if let Some(visibilities) = visibilities {
        setup.visibilities(visibilities);
    }
    let related: Vec<_> = items
        .related
        .iter()
        .map(|related| {
            let paths = setup.related(&related.relation, &related.any_of);
            (related.relation.as_str(), paths)
        })
        .collect();
    let modifiers = items.modifiers.as_deref();
    if let Some(modifiers) = modifiers {
        setup.modifiers(modifiers, Role::Modifier);
    }
    let filter = Filter {
        named,
        kinds,
        visibilities,
        related,
        modifiers,
    };
    let checker = check.prepare(setup);
    Box::new(Selected { filter, checker })
}
