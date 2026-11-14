//! The checks of patterns and of lists that more than one kind of rule makes (SPEC section
//! 5). A check reaches them through [`crate::Setup`].

use crate::error::{ErrorCode, Role, RuleError};
use crate::model::{FactSet, RawPattern, Targets};
use crate::pattern::{join, Pattern};

pub(crate) fn error(code: ErrorCode, role: Option<Role>, value: Option<String>) -> RuleError {
    RuleError { code, role, value }
}

/// Target patterns that are checked for existence (SPEC 5.1), with their errors.
pub(crate) fn verified_targets(
    patterns: &[RawPattern],
    role: Role,
    facts: &FactSet,
    errors: &mut Vec<RuleError>,
) -> Vec<Pattern> {
    let separator = facts.separator.as_str();
    let mut valid = Vec::new();
    for raw in patterns {
        match Pattern::new(raw) {
            None => errors.push(invalid_pattern(raw, role, separator)),
            Some(pattern) => {
                if is_internal(&pattern, facts) && !is_grounded(&pattern, facts) {
                    errors.push(error(
                        ErrorCode::PatternMatchesNothing,
                        Some(role),
                        Some(join(raw, separator)),
                    ));
                }
                valid.push(pattern);
            }
        }
    }
    valid
}

pub(crate) fn is_internal(pattern: &Pattern, facts: &FactSet) -> bool {
    match pattern.literal_head() {
        Some(head) => facts
            .units
            .iter()
            .any(|unit| unit.first().map(String::as_str) == Some(head)),
        None => false,
    }
}

fn is_grounded(pattern: &Pattern, facts: &FactSet) -> bool {
    facts
        .units
        .iter()
        .chain(&facts.items)
        .any(|path| pattern.matches_within(path))
        || facts
            .items
            .iter()
            .any(|item| item.len() < pattern.len() && pattern.literal_prefix_is(item))
}

pub(crate) fn unit_patterns(
    patterns: &[RawPattern],
    role: Role,
    facts: &FactSet,
    errors: &mut Vec<RuleError>,
) -> Vec<Pattern> {
    let mut valid = Vec::new();
    for raw in patterns {
        match Pattern::new(raw) {
            None => errors.push(invalid_pattern(raw, role, &facts.separator)),
            Some(pattern) => {
                if !facts.units.iter().any(|unit| pattern.matches(unit)) {
                    errors.push(error(
                        ErrorCode::PatternMatchesNothing,
                        Some(role),
                        Some(join(raw, &facts.separator)),
                    ));
                }
                valid.push(pattern);
            }
        }
    }
    valid
}

/// Unit patterns of a list that must not be empty, with their errors.
pub(crate) fn unit_list(
    patterns: &[RawPattern],
    role: Role,
    facts: &FactSet,
    errors: &mut Vec<RuleError>,
) -> Vec<Pattern> {
    if patterns.is_empty() {
        errors.push(error(ErrorCode::EmptyList, Some(role), None));
    }
    unit_patterns(patterns, role, facts, errors)
}

pub(crate) fn invalid_pattern(raw: &[String], role: Role, separator: &str) -> RuleError {
    error(
        ErrorCode::InvalidPattern,
        Some(role),
        Some(join(raw, separator)),
    )
}

pub(crate) fn unknown_values(
    values: &[String],
    known: &[String],
    code: ErrorCode,
    role: Role,
    errors: &mut Vec<RuleError>,
) {
    for value in values {
        if !known.contains(value) {
            errors.push(error(code, Some(role), Some(value.clone())));
        }
    }
}

/// The targets of a rule about uses, with their errors (SPEC section 5, checks 7 to 9).
pub(crate) fn use_targets(
    targets: &Targets,
    facts: &FactSet,
    errors: &mut Vec<RuleError>,
) -> Vec<Pattern> {
    let separator = facts.separator.as_str();
    let mut valid = Vec::new();
    if targets.any_of.is_empty() && targets.unverified.is_empty() {
        errors.push(error(ErrorCode::EmptyList, Some(Role::Target), None));
    }
    valid.extend(verified_targets(
        &targets.any_of,
        Role::Target,
        facts,
        errors,
    ));
    for raw in &targets.unverified {
        match Pattern::new(raw) {
            None => errors.push(invalid_pattern(raw, Role::UnverifiedTarget, separator)),
            Some(pattern) => valid.push(pattern),
        }
    }
    valid
}
