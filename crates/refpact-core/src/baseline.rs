//! Baselines: violations that a project accepts for now (SPEC section 7).

use crate::report::PathFormat;
use crate::violation::{Detail, Violation};

/// One line of a baseline: `count` accepted violations with one key.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct BaselineEntry {
    /// The name of the rule.
    pub rule: String,
    /// The unit, printed in the path format of the fact set.
    pub unit: String,
    /// What the violations are, as SPEC section 7 writes it for each kind of violation.
    pub what: String,
    /// The number of accepted violations with this key.
    pub count: u32,
}

/// An entry that accepts more violations than the code has.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StaleEntry {
    /// The unit of the entry.
    pub unit: String,
    /// What the entry accepts, as in [`BaselineEntry::what`].
    pub what: String,
    /// The number of violations that the baseline accepts for this key.
    pub count: u32,
    /// The number of violations that the code has.
    pub found: u32,
}

/// A baseline text that cannot be read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BaselineError {
    /// The number of the first invalid line, from 1.
    pub line: usize,
}

/// The result of [`apply_baseline`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Applied {
    /// The violations that the baseline does not accept.
    pub remaining: Vec<Violation>,
    /// The entries that accept more violations than the code has.
    pub stale: Vec<StaleEntry>,
}

const HEADER: &str = "# refpact baseline: violations that are accepted for now.\n\
# Columns, separated by tabs: rule, unit, what, count.\n";

/// Reads the text form of a baseline.
pub fn parse_baseline(text: &str) -> Result<Vec<BaselineEntry>, BaselineError> {
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let invalid = BaselineError { line: index + 1 };
        let values: Vec<&str> = line.split('\t').collect();
        let [rule, unit, what, count] = values[..] else {
            return Err(invalid);
        };
        let digits = !count.is_empty() && count.bytes().all(|byte| byte.is_ascii_digit());
        let count = if digits {
            count.parse::<u32>().unwrap_or(0)
        } else {
            0
        };
        if rule.is_empty() || unit.is_empty() || what.is_empty() || count == 0 {
            return Err(invalid);
        }
        entries.push(BaselineEntry {
            rule: rule.to_owned(),
            unit: unit.to_owned(),
            what: what.to_owned(),
            count,
        });
    }
    Ok(entries)
}

/// Writes the text form of a baseline, sorted, so that a change shows as a small difference.
pub fn format_baseline(entries: &[BaselineEntry]) -> String {
    let mut sorted = entries.to_vec();
    sorted.sort();
    let mut text = HEADER.to_owned();
    for entry in &sorted {
        text.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            entry.rule, entry.unit, entry.what, entry.count
        ));
    }
    text
}

/// The `unit` and `what` of the key of a violation.
pub fn baseline_key(violation: &Violation, format: &PathFormat) -> (String, String) {
    match &violation.detail {
        Detail::Uses {
            item, target, kind, ..
        } => (
            format.print(item.as_ref().unwrap_or(&violation.unit)),
            format!("uses {kind} {}", format.print(target)),
        ),
        Detail::Contains { marker } => {
            (format.print(&violation.unit), format!("contains {marker}"))
        }
        Detail::Cycle { steps, .. } => {
            let mut nodes: Vec<String> =
                steps.iter().map(|step| format.print(&step.from)).collect();
            let first = (0..nodes.len())
                .min_by_key(|&index| &nodes[index])
                .unwrap_or(0);
            nodes.rotate_left(first);
            (nodes[0].clone(), format!("cycle {}", nodes.join(" > ")))
        }
        Detail::Unused => (format.print(&violation.unit), "unused".to_owned()),
        Detail::Reaches { end, .. } => (
            format.print(&violation.unit),
            format!("reaches {}", format.print(&end.target)),
        ),
        Detail::Unreachable => (format.print(&violation.unit), "unreachable".to_owned()),
        Detail::Outside => (format.print(&violation.unit), "outside".to_owned()),
        Detail::Item { item, .. } => (
            format.print(&violation.unit),
            format!("item {}", format.print(item)),
        ),
        Detail::TooLong { item, .. } => (
            format.print(&violation.unit),
            match item {
                Some(item) => format!("long item {}", format.print(item)),
                None => "long".to_owned(),
            },
        ),
        Detail::Custom { message, key } => {
            // A tab or a line break would break the line of the baseline.
            let what = key.as_ref().unwrap_or(message);
            let what: String = what
                .chars()
                .map(|c| {
                    if matches!(c, '\t' | '\n' | '\r') {
                        ' '
                    } else {
                        c
                    }
                })
                .collect();
            (format.print(&violation.unit), format!("custom {what}"))
        }
    }
}

/// One entry for each key of the violations, with the number of violations that have it.
pub fn baseline_entries(violations: &[Violation], format: &PathFormat) -> Vec<BaselineEntry> {
    let mut entries: Vec<BaselineEntry> = Vec::new();
    for violation in violations {
        let (unit, what) = baseline_key(violation, format);
        match entries
            .iter_mut()
            .find(|entry| entry.rule == violation.rule && entry.unit == unit && entry.what == what)
        {
            Some(entry) => entry.count += 1,
            None => entries.push(BaselineEntry {
                rule: violation.rule.clone(),
                unit,
                what,
                count: 1,
            }),
        }
    }
    entries.sort();
    entries
}

/// Removes the violations of one rule that the baseline accepts.
pub fn apply_baseline(
    rule: &str,
    violations: Vec<Violation>,
    baseline: &[BaselineEntry],
    format: &PathFormat,
) -> Applied {
    // The allowance of each key, in the order in which the keys first occur.
    let mut allowances: Vec<(&str, &str, u32)> = Vec::new();
    for entry in baseline.iter().filter(|entry| entry.rule == rule) {
        match allowances
            .iter_mut()
            .find(|(unit, what, _)| *unit == entry.unit && *what == entry.what)
        {
            Some(allowance) => allowance.2 += entry.count,
            None => allowances.push((&entry.unit, &entry.what, entry.count)),
        }
    }
    let keys: Vec<(String, String)> = violations
        .iter()
        .map(|violation| baseline_key(violation, format))
        .collect();
    let found = |unit: &str, what: &str| {
        keys.iter()
            .filter(|(key_unit, key_what)| key_unit == unit && key_what == what)
            .count() as u32
    };
    let accepted = |unit: &str, what: &str| {
        allowances
            .iter()
            .any(|(allowed_unit, allowed_what, count)| {
                *allowed_unit == unit && *allowed_what == what && found(unit, what) <= *count
            })
    };

    let stale = allowances
        .iter()
        .filter(|(unit, what, count)| found(unit, what) < *count)
        .map(|(unit, what, count)| StaleEntry {
            unit: (*unit).to_owned(),
            what: (*what).to_owned(),
            count: *count,
            found: found(unit, what),
        })
        .collect();
    let remaining = violations
        .iter()
        .zip(&keys)
        .filter(|(_, (unit, what))| !accepted(unit, what))
        .map(|(violation, _)| violation.clone())
        .collect();
    Applied { remaining, stale }
}
