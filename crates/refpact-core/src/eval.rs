//! Rule evaluation (SPEC sections 4 and 5): what every rule shares. The subjects, the
//! filters and the exemptions are applied here, and the check of the constraint finds the
//! violations (SPEC 4.15).

use crate::check::{Check, Checker, Scope, Setup};
use crate::checks::{error, unit_list, unknown_values, verified_targets};
use crate::cycles;
use crate::error::{ErrorCode, Role, RuleError};
use crate::filters::Filters;
use crate::index::{Index, Tables};
use crate::items;
use crate::lengths;
use crate::model::{FactSet, KindFilter, RawPattern, What};
use crate::pattern::{join, Pattern};
use crate::reach;
use crate::rule::{Constraint, Exemption, Include, Rule};
use crate::uses;
use crate::violation::Violation;

/// Evaluates one rule against one fact set.
///
/// Returns the violations in fact order, or the reasons why the rule cannot be evaluated.
pub fn evaluate(rule: &Rule, facts: &FactSet) -> Result<Vec<Violation>, Vec<RuleError>> {
    evaluate_with(rule, &rule.constraint, facts, &Tables::new(facts))
}

/// A fact set with the lookups that every evaluation over it needs, made once. Evaluate many
/// rules over one fact set with it: each rule then costs only what depends on the rule.
#[derive(Debug, Clone)]
pub struct Prepared {
    facts: FactSet,
    tables: Tables,
}

impl Prepared {
    /// Makes the lookups of a fact set.
    pub fn new(facts: FactSet) -> Prepared {
        let tables = Tables::new(&facts);
        Prepared { facts, tables }
    }

    /// The fact set.
    pub fn facts(&self) -> &FactSet {
        &self.facts
    }

    /// Evaluates one rule, as [`evaluate`] does.
    pub fn evaluate(&self, rule: &Rule) -> Result<Vec<Violation>, Vec<RuleError>> {
        evaluate_with(rule, &rule.constraint, &self.facts, &self.tables)
    }

    /// Evaluates the subjects, the filters and the exemptions of a rule with another check
    /// than its constraint.
    pub(crate) fn evaluate_check(
        &self,
        rule: &Rule,
        check: &dyn Check,
    ) -> Result<Vec<Violation>, Vec<RuleError>> {
        evaluate_with(rule, check, &self.facts, &self.tables)
    }
}

/// The built-in constraints, each a check like a check of the user.
impl Check for Constraint {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        match self {
            Constraint::MustNotUse(targets) => uses::not_use(targets, setup),
            Constraint::MustOnlyUse(targets) => uses::only_use(targets, setup),
            Constraint::MustNotContain(markers) => uses::not_contain(markers, setup),
            Constraint::MustBeAcyclic(slices) => cycles::acyclic(slices.as_deref(), setup),
            Constraint::MustBeIndependent(slices) => uses::independent(slices, setup),
            Constraint::MustBeUsed => reach::used(setup),
            Constraint::MustNotReach(targets) => reach::not_reach(targets, setup),
            Constraint::MustBeReachableFrom(roots) => reach::reachable_from(roots, setup),
            Constraint::MustBeIn(places) => reach::in_places(places, setup),
            Constraint::MustOnlyBeUsedBy(users) => uses::only_used_by(users, setup),
            Constraint::MustNotExceedLines(limit) => lengths::units(*limit, setup),
            Constraint::Items(rule_items) => items::prepare(rule_items, setup),
            Constraint::Custom(custom) => custom.prepare(setup),
        }
    }
}

fn evaluate_with(
    rule: &Rule,
    check: &dyn Check,
    facts: &FactSet,
    tables: &Tables,
) -> Result<Vec<Violation>, Vec<RuleError>> {
    if !tables.invalid.is_empty() {
        return Err(tables.invalid.clone());
    }
    let mut index = Index::new(facts, tables);

    let mut errors = Vec::new();

    if rule.name.is_empty() {
        errors.push(error(ErrorCode::EmptyName, None, None));
    }

    // Subjects.
    let errors_before_subjects = errors.len();
    let include = match &rule.subjects.include {
        Include::All => None,
        Include::AnyOf(patterns) => Some(unit_list(patterns, Role::Subject, facts, &mut errors)),
    };
    let except = match &rule.subjects.except {
        None => Vec::new(),
        Some(patterns) => unit_list(patterns, Role::SubjectException, facts, &mut errors),
    };
    // For each unit, by position: whether the rule selects it.
    let selected: Vec<bool> = facts
        .units
        .iter()
        .map(|unit| {
            let included = match &include {
                None => true,
                Some(patterns) => patterns.iter().any(|pattern| pattern.matches(unit)),
            };
            included && !except.iter().any(|pattern| pattern.matches(unit))
        })
        .collect();
    if errors.len() == errors_before_subjects && !selected.contains(&true) {
        errors.push(error(ErrorCode::NoSubjects, None, None));
    }

    // Constraint.
    let checker = check.prepare(&mut Setup::new(facts, &selected, &mut errors));

    // Filters.
    match &rule.kinds {
        KindFilter::All => {}
        KindFilter::Only(kinds) => {
            if kinds.is_empty() {
                errors.push(error(ErrorCode::EmptyList, Some(Role::Kind), None));
            }
            unknown_values(
                kinds,
                &facts.kinds,
                ErrorCode::UnknownKind,
                Role::Kind,
                &mut errors,
            );
        }
        KindFilter::Except(kinds) => {
            unknown_values(
                kinds,
                &facts.kinds,
                ErrorCode::UnknownKind,
                Role::Kind,
                &mut errors,
            );
        }
    }
    unknown_values(
        &rule.ignore_tags,
        &facts.tags,
        ErrorCode::UnknownTag,
        Role::Tag,
        &mut errors,
    );

    unknown_values(
        &rule.properties,
        &facts.properties,
        ErrorCode::UnknownProperty,
        Role::Property,
        &mut errors,
    );

    let exemptions: Vec<(Vec<Pattern>, Vec<Pattern>)> = rule
        .exemptions
        .iter()
        .map(|exemption| exemption_patterns(exemption, facts, &mut errors))
        .collect();

    if !errors.is_empty() {
        return Err(errors);
    }

    let filters = Filters {
        name: &rule.name,
        ignore_tags: &rule.ignore_tags,
        kinds: &rule.kinds,
        candidates: rule.candidates,
    };
    if !exemptions.is_empty() {
        let unused = exempt_facts(
            &filters,
            &mut index,
            &exemptions,
            checker.reads_references(),
        );
        if !unused.is_empty() {
            let separator = facts.separator.as_str();
            let list = |patterns: &[RawPattern]| {
                let printed: Vec<String> = patterns
                    .iter()
                    .map(|pattern| join(pattern, separator))
                    .collect();
                printed.join(", ")
            };
            return Err(unused
                .into_iter()
                .map(|number| {
                    let exemption = &rule.exemptions[number];
                    let value = format!("{} -> {}", list(&exemption.from), list(&exemption.to));
                    error(
                        ErrorCode::UnusedExemption,
                        Some(Role::Exemption),
                        Some(value),
                    )
                })
                .collect());
        }
    }

    Ok(checker.violations(&Scope::new(filters, &index, &selected)))
}

/// The patterns of one exemption, with their errors (SPEC section 5, check 13).
fn exemption_patterns(
    exemption: &Exemption,
    facts: &FactSet,
    errors: &mut Vec<RuleError>,
) -> (Vec<Pattern>, Vec<Pattern>) {
    let from = unit_list(&exemption.from, Role::ExemptionFrom, facts, errors);
    if exemption.to.is_empty() {
        errors.push(error(ErrorCode::EmptyList, Some(Role::ExemptionTo), None));
    }
    let to = verified_targets(&exemption.to, Role::ExemptionTo, facts, errors);
    (from, to)
}

/// SPEC 4.13: marks the facts that the exemptions let pass in the index, and returns the
/// numbers of the exemptions that are not used (check 14).
fn exempt_facts(
    filters: &Filters,
    index: &mut Index,
    exemptions: &[(Vec<Pattern>, Vec<Pattern>)],
    reads_references: bool,
) -> Vec<usize> {
    let facts = index.facts;
    let mut exempt = vec![false; facts.facts.len()];
    let mut used = vec![false; exemptions.len()];
    for (position, fact) in facts.facts.iter().enumerate() {
        let What::Reference(reference) = &fact.what else {
            continue;
        };
        for (number, (from, to)) in exemptions.iter().enumerate() {
            let lets_pass = from.iter().any(|pattern| pattern.matches(&fact.unit))
                && reference
                    .paths()
                    .any(|path| to.iter().any(|pattern| pattern.matches_within(path)));
            if lets_pass {
                exempt[position] = true;
                used[number] |=
                    reads_references && !filters.ignores(&fact.tags) && filters.admits(reference);
            }
        }
    }
    index.set_exempt(exempt);
    (0..exemptions.len())
        .filter(|&number| !used[number])
        .collect()
}
