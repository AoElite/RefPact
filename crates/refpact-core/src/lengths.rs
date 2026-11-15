//! The rules about lengths (SPEC 4.16): a unit or an item must not have more lines than a
//! limit.

use crate::check::{Checker, NoReferences, Scope, Setup};
use crate::error::ErrorCode;
use crate::item_check::{ItemChecker, ItemScope};
use crate::violation::{Detail, Violation};

/// One violation for each selected unit with more lines than `limit`. Without a selected
/// unit whose lines are counted, the rule cannot fail.
pub(crate) fn units<'c>(limit: u32, setup: &mut Setup) -> Box<dyn Checker + 'c> {
    let facts = setup.facts();
    if !setup.has_errors() && setup.subjects().all(|unit| facts.lines_of(unit).is_none()) {
        // The front end does not count the lines of these units.
        setup.error(ErrorCode::NoSubjects, None, None);
    }
    Box::new(NoReferences(move |scope: &Scope<'_>| -> Vec<Violation> {
        let facts = scope.facts();
        scope
            .subjects()
            .filter_map(|unit| {
                let lines = facts.lines_of(unit).filter(|lines| *lines > limit)?;
                let detail = Detail::TooLong {
                    item: None,
                    kind: None,
                    lines,
                    limit,
                };
                Some(scope.violation_of_unit(unit, detail))
            })
            .collect()
    }))
}

/// One violation for each selected declaration with more lines than `limit`. Without a
/// declaration whose end is recorded, the rule cannot fail.
pub(crate) fn items<'c>(limit: u32, setup: &mut Setup) -> Box<dyn ItemChecker + 'c> {
    let facts = setup.facts();
    if !setup.has_errors()
        && facts
            .declarations
            .iter()
            .all(|item| item.end_line.is_none())
    {
        // The front end does not say where an item ends.
        setup.error(ErrorCode::NoSubjects, None, None);
    }
    Box::new(NoReferences(
        move |items: &ItemScope<'_>| -> Vec<Violation> {
            items
                .items()
                .filter_map(|declaration| {
                    let lines = declaration.lines().filter(|lines| *lines > limit)?;
                    let detail = Detail::TooLong {
                        item: Some(declaration.path.clone()),
                        kind: Some(declaration.kind.clone()),
                        lines,
                        limit,
                    };
                    Some(items.violation_of_item(declaration, detail))
                })
                .collect()
        },
    ))
}
