//! Text form of violations and errors.

use crate::error::{ErrorCode, Role, RuleError};
use crate::model::{Certainty, FactSet};
use crate::pattern::Path;
use crate::rule::Severity;
use crate::violation::{Detail, Step, Violation};

/// How reports and baselines print a path (SPEC section 3).
///
/// The segments are joined with the separator. A segment after the first that starts with
/// one of `attached_prefixes` is joined without it, and the first segment loses the first of
/// `hidden_prefixes` that it starts with. TypeScript prints `["npm:lodash", "fp", "#map"]`
/// as `lodash/fp#map` with the separator `/`, attached `#` and hidden `npm:`.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PathFormat {
    /// Text between segments.
    pub separator: String,
    /// A segment that starts with one of these is printed without a separator before it.
    pub attached_prefixes: Vec<String>,
    /// A first segment that starts with one of these is printed without it.
    pub hidden_prefixes: Vec<String>,
}

impl PathFormat {
    /// Segments joined with `separator`, and nothing else.
    pub fn plain(separator: &str) -> PathFormat {
        PathFormat {
            separator: separator.to_owned(),
            ..PathFormat::default()
        }
    }

    /// The format of the paths of a fact set.
    pub fn of(facts: &FactSet) -> PathFormat {
        PathFormat {
            separator: facts.separator.clone(),
            attached_prefixes: facts.attached_prefixes.clone(),
            hidden_prefixes: facts.hidden_prefixes.clone(),
        }
    }

    /// Prints a path.
    pub fn print(&self, path: &Path) -> String {
        let mut text = String::new();
        for (index, segment) in path.iter().enumerate() {
            if index == 0 {
                let hidden = self
                    .hidden_prefixes
                    .iter()
                    .find_map(|prefix| segment.strip_prefix(prefix.as_str()));
                text.push_str(hidden.unwrap_or(segment));
            } else {
                if !self
                    .attached_prefixes
                    .iter()
                    .any(|prefix| segment.starts_with(prefix.as_str()))
                {
                    text.push_str(&self.separator);
                }
                text.push_str(segment);
            }
        }
        text
    }
}

/// One line for one violation: file, line, column, rule name, unit and offending path.
pub fn render_violation(violation: &Violation, format: &PathFormat) -> String {
    // A violation about a unit as a whole has no line, and can have no file.
    let place = match (violation.file.as_str(), violation.line) {
        ("", 0) => String::new(),
        (file, 0) => format!("{file}: "),
        (file, line) => format!("{file}:{line}:{}: ", violation.column),
    };
    // A use inside an item names the item, and the unit is in the place.
    let subject = match &violation.detail {
        Detail::Uses {
            item: Some(item), ..
        } => item,
        _ => &violation.unit,
    };
    let mut line = format!("{place}[{}] {} ", violation.rule, format.print(subject));
    match &violation.detail {
        Detail::Uses {
            item: _,
            target,
            written,
            kind,
            certainty,
        } => {
            line.push_str(&format!("uses {} ({kind}", format.print(target)));
            if let Some(written) = written {
                line.push_str(&format!(", written as {}", format.print(written)));
            }
            if *certainty == Certainty::Candidate {
                line.push_str(", candidate");
            }
            if let Some(note) = &violation.note {
                line.push_str(&format!(", {note}"));
            }
            line.push(')');
        }
        Detail::Contains { marker } => {
            line.push_str(&format!("contains {marker}"));
            push_note(&mut line, violation);
        }
        Detail::Custom { message, .. } => {
            line.push_str(message);
            push_note(&mut line, violation);
        }
        Detail::Cycle { steps, size } => {
            line.push_str(&format!(
                "is in a cycle of {} (the tangle has {size} in all",
                steps.len()
            ));
            if let Some(note) = &violation.note {
                line.push_str(&format!(", {note}"));
            }
            line.push_str("):");
            for step in steps {
                line.push_str("\n      ");
                line.push_str(&render_step(step, format));
            }
        }
        Detail::Unused => line.push_str("is not used from outside"),
        Detail::Unreachable => line.push_str("cannot be reached from the roots"),
        Detail::Outside => {
            line.push_str("is in none of the places of the rule");
            push_note(&mut line, violation);
        }
        Detail::Item { item, kind } => {
            line.push_str(&format!("declares {kind} {}", format.print(item)))
        }
        Detail::TooLong {
            item,
            kind,
            lines,
            limit,
        } => {
            if let Some(item) = item {
                let kind = kind.as_deref().unwrap_or("item");
                line.push_str(&format!("declares {kind} {}, which ", format.print(item)));
            }
            line.push_str(&format!("has {lines} lines, and the limit is {limit}"));
        }
        Detail::Reaches { steps, end } => {
            line.push_str(&format!(
                "reaches {} in {} step(s):",
                format.print(&end.target),
                steps.len() + 1
            ));
            for step in steps {
                line.push_str("\n      ");
                line.push_str(&render_step(step, format));
            }
            line.push_str(&format!(
                "\n      {} uses {}  at {}:{}:{} ({})",
                format.print(&end.unit),
                format.print(&end.target),
                end.file,
                end.line,
                end.column,
                end.kind,
            ));
        }
    }
    line
}

/// The note of a violation after its text: ` (<note>)`.
fn push_note(line: &mut String, violation: &Violation) {
    if let Some(note) = &violation.note {
        line.push_str(&format!(" ({note})"));
    }
}

fn render_step(step: &Step, format: &PathFormat) -> String {
    format!(
        "{} -> {}  at {}:{}:{}, uses {} ({})",
        format.print(&step.from),
        format.print(&step.to),
        step.file,
        step.line,
        step.column,
        format.print(&step.target),
        step.kind,
    )
}

/// All violations of one rule, one per line, with a heading, and the reason of the rule
/// when it has one.
pub fn render_violations(
    rule: &str,
    reason: Option<&str>,
    severity: Severity,
    violations: &[Violation],
    format: &PathFormat,
) -> String {
    let warning = match severity {
        Severity::Error => "",
        Severity::Warning => "warning: ",
    };
    let mut text = format!(
        "{warning}rule `{rule}` is violated in {} place(s):",
        violations.len()
    );
    if let Some(reason) = reason.filter(|reason| !reason.is_empty()) {
        text.push_str("\n  because ");
        text.push_str(reason);
    }
    for violation in violations {
        text.push_str("\n  ");
        text.push_str(&render_violation(violation, format));
    }
    text
}

/// One sentence for one error.
pub fn render_error(error: &RuleError) -> String {
    let value = error.value.as_deref().unwrap_or("");
    let role = match error.role {
        Some(Role::Unit) => "unit",
        Some(Role::Subject) => "subject pattern",
        Some(Role::SubjectException) => "subject exception",
        Some(Role::Target) => "target pattern",
        Some(Role::UnverifiedTarget) => "unverified target pattern",
        Some(Role::Marker) => "marker",
        Some(Role::Slice) => "slice pattern",
        Some(Role::Root) => "root pattern",
        Some(Role::Kind) => "kind",
        Some(Role::Tag) => "tag",
        Some(Role::Place) => "place pattern",
        Some(Role::User) => "user pattern",
        Some(Role::ExemptionFrom) => "exemption unit pattern",
        Some(Role::ExemptionException) => "exemption exception",
        Some(Role::ExemptionTo) => "exemption target pattern",
        Some(Role::Exemption) => "exemption",
        Some(Role::Name) => "name",
        Some(Role::ItemKind) => "item kind",
        Some(Role::Visibility) => "visibility",
        Some(Role::Relation) => "relation",
        Some(Role::Related) => "related pattern",
        Some(Role::RequiredName) => "required name",
        Some(Role::Required) => "required pattern",
        Some(Role::Modifier) => "modifier",
        Some(Role::RequiredModifier) => "required modifier",
        Some(Role::Layer) => "layer pattern",
        Some(Role::Property) => "property",
        None => "",
    };
    match error.code {
        ErrorCode::InvalidFacts => {
            format!("the fact set is invalid: {role} `{value}` is not declared")
        }
        ErrorCode::EmptyName => "the rule has no name".to_owned(),
        ErrorCode::EmptyList => format!("the {role} list is empty"),
        ErrorCode::InvalidPattern => format!("{role} `{value}` is not a valid pattern"),
        ErrorCode::PatternMatchesNothing => {
            format!("{role} `{value}` matches nothing in the analysed code")
        }
        ErrorCode::NoSubjects => "the subjects select no unit that the rule can check".to_owned(),
        ErrorCode::TooFewNodes => {
            "fewer than two units or slices are selected, so the rule cannot fail".to_owned()
        }
        ErrorCode::UnusedExemption => {
            format!("exemption `{value}` lets no reference pass; remove it")
        }
        ErrorCode::UnknownMarker
        | ErrorCode::UnknownKind
        | ErrorCode::UnknownTag
        | ErrorCode::UnknownItemKind
        | ErrorCode::UnknownVisibility
        | ErrorCode::UnknownRelation
        | ErrorCode::UnknownModifier
        | ErrorCode::UnknownProperty => {
            format!("{role} `{value}` is not known to this front end")
        }
    }
}

/// All errors of one rule, one per line, with a heading.
pub fn render_errors(rule: &str, errors: &[RuleError]) -> String {
    let mut text = format!("rule `{rule}` cannot be checked:");
    for error in errors {
        text.push_str("\n  ");
        text.push_str(&render_error(error));
    }
    text
}
