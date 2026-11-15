//! What an evaluation reports.

use crate::index::Index;
use crate::model::Certainty;
use crate::pattern::Path;

/// One place where the code breaks a rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    /// The name of the rule.
    pub rule: String,
    /// The unit that breaks the rule.
    pub unit: Path,
    /// The file of the place. For a violation about a unit as a whole, the file of the first
    /// fact of the unit, or empty.
    pub file: String,
    /// The line, or 0 for a violation about a unit as a whole.
    pub line: u32,
    /// The column, or 0 for a violation about a unit as a whole.
    pub column: u32,
    /// The note of the fact, when it has one.
    pub note: Option<String>,
    /// What the unit does.
    pub detail: Detail,
}

impl Violation {
    /// A violation that is about a unit as a whole and has no place in the source: the
    /// file is that of the unit (SPEC 4.7), and the line and column are 0.
    pub(crate) fn of_unit(rule: &str, unit: &Path, index: &Index, detail: Detail) -> Violation {
        let facts = index.facts;
        let number = index.unit(unit);
        // The front end says which file the unit has. Without that, a fact or a
        // declaration of the unit names it.
        let given = number
            .and_then(|number| facts.unit_files.get(number))
            .filter(|file| !file.is_empty());
        let of_fact = || Some(&facts.facts[index.first_fact(number?)?].file);
        let declared = || {
            let mut declarations = facts.declarations.iter();
            Some(&declarations.find(|item| item.unit == *unit)?.file)
        };
        let file = given.or_else(of_fact).or_else(declared).cloned();
        Violation {
            rule: rule.to_owned(),
            unit: unit.clone(),
            file: file.unwrap_or_default(),
            line: 0,
            column: 0,
            note: None,
            detail,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// What a violation is about.
pub enum Detail {
    /// The unit refers to a path that the rule does not allow.
    Uses {
        /// For a rule about items: the item whose code has the reference. `None` for a rule
        /// about units.
        item: Option<Path>,
        /// The forbidden path.
        target: Path,
        /// The path as the code names it, when it differs from `target`.
        written: Option<Path>,
        /// The kind of the reference.
        kind: String,
        /// Whether the front end proved the path.
        certainty: Certainty,
    },
    /// The unit contains a marker that the rule forbids.
    Contains {
        /// The marker.
        marker: String,
    },
    /// One cycle of a tangle: a group of nodes that all reach each other.
    Cycle {
        /// The edges of the cycle, in order. The last step leads back to the first node.
        steps: Vec<Step>,
        /// The number of nodes in the tangle.
        size: usize,
    },
    /// Nothing outside the unit and the units below it refers to it.
    Unused,
    /// A chain of references leads from the unit to a forbidden target.
    Reaches {
        /// The edges from the unit to the unit that uses the target. Empty when the unit
        /// uses the target itself.
        steps: Vec<Step>,
        /// The use of the target at the end of the chain.
        end: End,
    },
    /// No chain of references leads from a root to the unit.
    Unreachable,
    /// The unit is in none of the places of the rule.
    Outside,
    /// A declaration that does not meet a rule about items.
    Item {
        /// The path of the item.
        item: Path,
        /// The kind of the item.
        kind: String,
    },
    /// The unit, or one declaration of it, has more lines than the rule allows (SPEC 4.16).
    TooLong {
        /// For a rule about items: the path of the item. `None` for a rule about units.
        item: Option<Path>,
        /// For a rule about items: the kind of the item.
        kind: Option<String>,
        /// The number of lines.
        lines: u32,
        /// The number of lines that the rule allows.
        limit: u32,
    },
    /// What a check of the user found (SPEC 4.15).
    Custom {
        /// What the unit does, printed after the unit.
        message: String,
        /// What a baseline counts the violation as. `None`: the message. Give a key when the
        /// message has details that change, such as a number, so that the baseline stays.
        key: Option<String>,
    },
}

impl Detail {
    /// A detail of a check of the user, which a baseline counts by its message.
    pub fn custom(message: impl Into<String>) -> Detail {
        Detail::Custom {
            message: message.into(),
            key: None,
        }
    }

    /// A detail of a check of the user, which a baseline counts by `key`.
    pub fn custom_with_key(message: impl Into<String>, key: impl Into<String>) -> Detail {
        Detail::Custom {
            message: message.into(),
            key: Some(key.into()),
        }
    }
}

/// The reference at the end of a chain: the use of the forbidden target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct End {
    /// The unit that uses the target.
    pub unit: Path,
    /// The file of the use.
    pub file: String,
    /// The line of the use.
    pub line: u32,
    /// The column of the use.
    pub column: u32,
    /// The forbidden path.
    pub target: Path,
    /// The kind of the reference.
    pub kind: String,
}

/// One edge of a cycle or of a chain, with the reference that makes it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Step {
    /// The node the edge leaves.
    pub from: Path,
    /// The node the edge leads to.
    pub to: Path,
    /// The file of the reference.
    pub file: String,
    /// The line of the reference.
    pub line: u32,
    /// The column of the reference.
    pub column: u32,
    /// The path the reference names.
    pub target: Path,
    /// The kind of the reference.
    pub kind: String,
}
