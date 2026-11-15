//! Rules as plain data (SPEC section 4). A rule names its subjects, its filters and its
//! constraint. Every constraint is a [`Check`]: the built-in ones are data that the core
//! knows, and [`Constraint::Custom`] holds a check of the user.

use crate::check::Custom;
use crate::item_check::CustomItems;
use crate::model::{Candidates, KindFilter, RawPattern, Targets};

/// One architecture rule.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rule {
    /// The name, printed with every violation. An empty name is an error.
    pub name: String,
    /// Why the rule exists, printed with its violations. `None`: no reason is given.
    pub reason: Option<String>,
    /// Whether a violation fails the check or is only reported. It does not change the
    /// evaluation.
    pub severity: Severity,
    /// The units the rule applies to.
    pub subjects: Subjects,
    /// What the units must do or must not do.
    pub constraint: Constraint,
    /// The reference kinds the rule looks at.
    pub kinds: KindFilter,
    /// Facts with one of these tags are not checked.
    pub ignore_tags: Vec<String>,
    /// Whether candidate references count.
    pub candidates: Candidates,
    /// References that the rule does not see.
    pub exemptions: Vec<Exemption>,
    /// The names of the properties of references that the check of the rule reads. One that
    /// the front end does not produce is an error.
    pub properties: Vec<String>,
}

impl Rule {
    /// A rule with the default filters: all kinds, no ignored tags, candidates included.
    pub fn new(name: impl Into<String>, subjects: Subjects, constraint: Constraint) -> Rule {
        Rule {
            name: name.into(),
            reason: None,
            severity: Severity::Error,
            subjects,
            constraint,
            kinds: KindFilter::All,
            ignore_tags: Vec::new(),
            candidates: Candidates::Include,
            exemptions: Vec::new(),
            properties: Vec::new(),
        }
    }
}

/// How much a violation of a rule weighs (SPEC section 4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum Severity {
    /// A violation fails the check.
    #[default]
    Error,
    /// A violation is reported, and the check passes.
    Warning,
}

impl Severity {
    /// The severity as the specification writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warning => "warning",
        }
    }
}

/// References that a rule does not see (SPEC 4.13): those of the units that `from`
/// selects, with a path that a `to` pattern matches within.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Exemption {
    /// The units, selected as the subjects of a rule are.
    pub from: Subjects,
    /// The paths. An empty list is an error.
    pub to: Vec<RawPattern>,
}

/// A selection of units: those a rule applies to, or those of an exemption.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subjects {
    /// The units that are selected before the exceptions.
    pub include: Include,
    /// `None`: no exceptions. `Some(vec![])` is an error.
    pub except: Option<Vec<RawPattern>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// The units a rule selects before its exceptions.
pub enum Include {
    /// All units.
    All,
    /// The units that at least one of the patterns matches exactly. An empty list is an error.
    AnyOf(Vec<RawPattern>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// What a rule demands of its subjects (SPEC section 4).
pub enum Constraint {
    /// No reference may lead to one of the targets.
    MustNotUse(Targets),
    /// Every reference must lead to one of the targets, or stay inside its own unit.
    MustOnlyUse(Targets),
    /// The units must not contain one of the markers.
    MustNotContain(Vec<String>),
    /// The references between the units, or between the slices, must not form a cycle.
    /// `None`: each unit is a node. `Some(vec![])` is an error.
    MustBeAcyclic(Option<Vec<RawPattern>>),
    /// No reference may lead from one slice to another slice.
    MustBeIndependent(Vec<RawPattern>),
    /// Each unit must be referred to from outside itself and the units below it.
    MustBeUsed,
    /// No chain of references may lead from the units to one of the targets.
    MustNotReach(Targets),
    /// A chain of references must lead from one of the roots to each unit.
    MustBeReachableFrom(Vec<RawPattern>),
    /// Each unit must be matched exactly by one of the places.
    MustBeIn(Vec<RawPattern>),
    /// Only the units that a user pattern matches, and the selected units, may refer to the
    /// selected units.
    MustOnlyBeUsedBy(Vec<RawPattern>),
    /// Each unit must have at most this number of lines (SPEC 4.16).
    MustNotExceedLines(u32),
    /// The declarations of the units that the filters select must meet a demand.
    Items(Items),
    /// A check of the user (SPEC 4.15). It is not plain data, so `evaluate` of the JSON of
    /// section 8 does not take it: a binding in another language runs its check after
    /// `select`.
    Custom(Custom),
}

/// A rule about declarations (SPEC 4.12): filters, which all must hold, and a demand.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Items {
    /// `None`: any name. Otherwise a glob must match the name. An empty list is an error.
    pub named: Option<Vec<String>>,
    /// `None`: any kind. An empty list is an error.
    pub kinds: Option<Vec<String>>,
    /// `None`: any visibility. An empty list is an error.
    pub visibilities: Option<Vec<String>>,
    /// The relations that the item must have, each to one of its paths. Empty: any.
    pub related: Vec<Related>,
    /// `None`: any modifiers. Otherwise the item must have one of these. An empty list is an
    /// error.
    pub modifiers: Option<Vec<String>>,
    /// What each selected declaration must meet.
    pub must: Demand,
}

/// A relation to one of some paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Related {
    /// The kind of tie, such as `implements`.
    pub relation: String,
    /// The paths, which match the target within. An empty list is an error.
    pub any_of: Vec<RawPattern>,
}

/// What each declaration of a rule about items must meet.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Demand {
    /// No declaration may be selected.
    NotExist,
    /// The unit of the declaration must be matched exactly by one of the places.
    ResideIn(Vec<RawPattern>),
    /// A glob must match the name.
    BeNamed(Vec<String>),
    /// The item must have this relation to one of the paths.
    Have(Related),
    /// The item must have one of these modifiers.
    HaveModifier(Vec<String>),
    /// No reference inside the item may lead to one of the targets (SPEC 4.12).
    NotUse(Targets),
    /// Every reference inside the item must lead to one of the targets, or stay inside its
    /// own unit (SPEC 4.12).
    OnlyUse(Targets),
    /// The item must have at most this number of lines (SPEC 4.16).
    NotExceedLines(u32),
    /// A check of the user (SPEC 4.15), over the declarations that the filters select. Like
    /// [`Constraint::Custom`], it is not plain data.
    Custom(CustomItems),
}
