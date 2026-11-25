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

/// The start of the `what` of a cycle.
const CYCLE: &str = "cycle ";

/// The report of the stale entries of one rule in a baseline file.
pub fn render_stale(rule: &str, file: &str, entries: &[StaleEntry]) -> String {
    let mut text = format!("rule `{rule}`: the baseline {file} accepts more than the code has:");
    for entry in entries {
        // The numbers of a cycle are the nodes of its tangle (SPEC 7.2).
        let of = if entry.what.starts_with(CYCLE) {
            "a tangle of "
        } else {
            ""
        };
        text.push_str(&format!(
            "\n  {}: {} (the baseline accepts {of}{}, the code has {of}{})",
            entry.unit, entry.what, entry.count, entry.found
        ));
    }
    text.push_str("\nRun the tests with REFPACT_BASELINE=update to write the baseline again.");
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
            // A check of the user can give a cycle without steps: its unit is the key.
            let unit = nodes
                .first()
                .cloned()
                .unwrap_or_else(|| format.print(&violation.unit));
            (unit, format!("{CYCLE}{}", nodes.join(" > ")))
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

/// What a violation counts for in a baseline: the nodes of the tangle for a cycle, so that a
/// tangle with more nodes is more than its entry accepts, and 1 for every other violation.
/// It is 1 or more, also for the cycle of a check of the user that gives no size: an entry
/// with a count of 0 cannot be read.
fn weight(violation: &Violation) -> u32 {
    match &violation.detail {
        Detail::Cycle { size, .. } => u32::try_from(*size).unwrap_or(u32::MAX).max(1),
        _ => 1,
    }
}

/// The note of a cycle that remains though the baseline has its key: without it, nothing
/// says why the entry does not accept the cycle. A tangle has two nodes or more, so for a
/// cycle with a size a count of 1 is that of a baseline from before version 0.2.0, which
/// had the number of cycles. The cycle of a check of the user that gives no size counts 1.
fn accepted_tangle(count: u32, size: usize) -> String {
    let mut note = format!("the baseline accepts a tangle of {count}");
    if count < 2 && size >= 2 {
        note.push_str(
            ": a baseline of a version before 0.2.0 has the number of cycles there, write it again",
        );
    }
    note
}

/// One entry for each key of the violations, with what the violations that have it count for.
pub fn baseline_entries(violations: &[Violation], format: &PathFormat) -> Vec<BaselineEntry> {
    let mut entries: Vec<BaselineEntry> = Vec::new();
    for violation in violations {
        let (unit, what) = baseline_key(violation, format);
        match entries
            .iter_mut()
            .find(|entry| entry.rule == violation.rule && entry.unit == unit && entry.what == what)
        {
            Some(entry) => entry.count = entry.count.saturating_add(weight(violation)),
            None => entries.push(BaselineEntry {
                rule: violation.rule.clone(),
                unit,
                what,
                count: weight(violation),
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
            Some(allowance) => allowance.2 = allowance.2.saturating_add(entry.count),
            None => allowances.push((&entry.unit, &entry.what, entry.count)),
        }
    }
    let keys: Vec<(String, String)> = violations
        .iter()
        .map(|violation| baseline_key(violation, format))
        .collect();
    let found = |unit: &str, what: &str| {
        keys.iter()
            .zip(&violations)
            .filter(|((key_unit, key_what), _)| key_unit == unit && key_what == what)
            .map(|(_, violation)| weight(violation))
            .fold(0u32, u32::saturating_add)
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
        .map(|(violation, (unit, what))| {
            let mut violation = violation.clone();
            let allowance = allowances.iter().find(|(allowed_unit, allowed_what, _)| {
                allowed_unit == unit && allowed_what == what
            });
            if let (Detail::Cycle { size, .. }, Some((_, _, count))) =
                (&violation.detail, allowance)
            {
                let said = accepted_tangle(*count, *size);
                violation.note = Some(match violation.note.take() {
                    Some(note) => format!("{note}, {said}"),
                    None => said,
                });
            }
            violation
        })
        .collect();
    Applied { remaining, stale }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A cycle of the unit `a` with no steps, as a check of the user may give it.
    fn cycle(size: usize) -> Violation {
        Violation {
            rule: "r".to_owned(),
            unit: vec!["a".to_owned()],
            file: "a".to_owned(),
            line: 1,
            column: 1,
            note: None,
            detail: Detail::Cycle {
                steps: Vec::new(),
                size,
            },
        }
    }

    /// A cycle of a check of the user may give no size and no steps. Its entry still has a
    /// unit and a count that a baseline can hold, and the baseline that is written accepts
    /// the violation.
    #[test]
    fn a_cycle_without_a_size_is_written_with_a_count_that_can_be_read() {
        let violation = cycle(0);
        let format = PathFormat::plain("::");
        let entries = baseline_entries(std::slice::from_ref(&violation), &format);
        assert_eq!((entries[0].unit.as_str(), entries[0].count), ("a", 1));
        let read = parse_baseline(&format_baseline(&entries)).unwrap();
        let applied = apply_baseline("r", vec![violation.clone()], &read, &format);
        assert_eq!((applied.remaining.len(), applied.stale.len()), (0, 0));
        // Two of them are more than the entry accepts. Its count of 1 is that of this
        // version, so the note does not name an older one.
        let applied = apply_baseline("r", vec![violation.clone(), violation], &read, &format);
        assert_eq!(
            applied.remaining[1].note.as_deref(),
            Some("the baseline accepts a tangle of 1")
        );
    }

    /// The sizes that a check of the user gives can be any number: their sum stays a count,
    /// and it is not a small number that an entry accepts.
    #[test]
    fn the_weights_of_large_cycles_do_not_wrap() {
        let format = PathFormat::plain("::");
        let large = vec![cycle(u32::MAX as usize), cycle(3)];
        let entries = baseline_entries(&large, &format);
        assert_eq!(entries[0].count, u32::MAX);
        let small = BaselineEntry {
            count: 2,
            ..entries[0].clone()
        };
        let applied = apply_baseline("r", large.clone(), &[small.clone(), small], &format);
        assert_eq!(applied.remaining.len(), 2);
        let twice = [entries[0].clone(), entries[0].clone()];
        let applied = apply_baseline("r", large, &twice, &format);
        assert_eq!((applied.remaining.len(), applied.stale.len()), (0, 0));
    }
}
