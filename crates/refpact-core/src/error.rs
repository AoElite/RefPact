//! Why a rule cannot be evaluated (SPEC section 5): the errors.

/// Why a rule cannot be evaluated.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleError {
    /// The check that failed.
    pub code: ErrorCode,
    /// The part of the rule or of the fact set that the error is about, when there is one.
    pub role: Option<Role>,
    /// The offending value, such as a pattern, when there is one.
    pub value: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The check that failed (SPEC section 5).
pub enum ErrorCode {
    /// The fact set names a unit, kind, marker, tag or property that it does not declare.
    InvalidFacts,
    /// The rule has an empty name.
    EmptyName,
    /// A list of the rule is empty.
    EmptyList,
    /// A pattern is not valid.
    InvalidPattern,
    /// A pattern matches nothing in the fact set.
    PatternMatchesNothing,
    /// The subjects select no unit that the rule can check.
    NoSubjects,
    /// Fewer than two nodes exist, so the rule cannot fail.
    TooFewNodes,
    /// The rule names a marker that the front end does not produce.
    UnknownMarker,
    /// The rule names a reference kind that the front end does not produce.
    UnknownKind,
    /// The rule names a tag that the front end does not produce.
    UnknownTag,
    /// An exemption lets no reference pass.
    UnusedExemption,
    /// The rule names an item kind that the front end does not produce.
    UnknownItemKind,
    /// The rule names a visibility that the front end does not produce.
    UnknownVisibility,
    /// The rule names a relation that the front end does not produce.
    UnknownRelation,
    /// The rule names a modifier that the front end does not produce.
    UnknownModifier,
    /// The rule names a property that the front end does not produce.
    UnknownProperty,
}

impl ErrorCode {
    /// Every code.
    pub(crate) const ALL: [ErrorCode; 16] = [
        ErrorCode::InvalidFacts,
        ErrorCode::EmptyName,
        ErrorCode::EmptyList,
        ErrorCode::InvalidPattern,
        ErrorCode::PatternMatchesNothing,
        ErrorCode::NoSubjects,
        ErrorCode::TooFewNodes,
        ErrorCode::UnknownMarker,
        ErrorCode::UnknownKind,
        ErrorCode::UnknownTag,
        ErrorCode::UnusedExemption,
        ErrorCode::UnknownItemKind,
        ErrorCode::UnknownVisibility,
        ErrorCode::UnknownRelation,
        ErrorCode::UnknownModifier,
        ErrorCode::UnknownProperty,
    ];

    /// The code as the specification writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            ErrorCode::InvalidFacts => "invalid-facts",
            ErrorCode::EmptyName => "empty-name",
            ErrorCode::EmptyList => "empty-list",
            ErrorCode::InvalidPattern => "invalid-pattern",
            ErrorCode::PatternMatchesNothing => "pattern-matches-nothing",
            ErrorCode::NoSubjects => "no-subjects",
            ErrorCode::TooFewNodes => "too-few-nodes",
            ErrorCode::UnknownMarker => "unknown-marker",
            ErrorCode::UnknownKind => "unknown-kind",
            ErrorCode::UnknownTag => "unknown-tag",
            ErrorCode::UnusedExemption => "unused-exemption",
            ErrorCode::UnknownItemKind => "unknown-item-kind",
            ErrorCode::UnknownVisibility => "unknown-visibility",
            ErrorCode::UnknownRelation => "unknown-relation",
            ErrorCode::UnknownModifier => "unknown-modifier",
            ErrorCode::UnknownProperty => "unknown-property",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// The part of a rule or of a fact set that an error is about.
pub enum Role {
    /// The unit of a fact.
    Unit,
    /// A pattern that selects subjects.
    Subject,
    /// An exception to the subjects.
    SubjectException,
    /// A target that is checked for existence.
    Target,
    /// A target that is not checked for existence.
    UnverifiedTarget,
    /// A marker.
    Marker,
    /// A slice pattern.
    Slice,
    /// A root pattern.
    Root,
    /// A reference kind.
    Kind,
    /// A tag.
    Tag,
    /// A place that units must be in.
    Place,
    /// A unit that may use the selected units.
    User,
    /// A pattern of the units of an exemption.
    ExemptionFrom,
    /// A pattern of the paths of an exemption.
    ExemptionTo,
    /// An exemption.
    Exemption,
    /// A glob that selects items by name.
    Name,
    /// An item kind.
    ItemKind,
    /// A visibility.
    Visibility,
    /// A relation.
    Relation,
    /// A pattern that selects items by a relation.
    Related,
    /// A glob that names of items must match.
    RequiredName,
    /// A pattern of a relation that items must have.
    Required,
    /// A modifier that selects items.
    Modifier,
    /// A modifier that items must have.
    RequiredModifier,
    /// A pattern of the units of a layer of a diagram.
    Layer,
    /// The name of a property of a reference.
    Property,
}

impl Role {
    /// Every role.
    pub(crate) const ALL: [Role; 26] = [
        Role::Unit,
        Role::Subject,
        Role::SubjectException,
        Role::Target,
        Role::UnverifiedTarget,
        Role::Marker,
        Role::Slice,
        Role::Root,
        Role::Kind,
        Role::Tag,
        Role::Place,
        Role::User,
        Role::ExemptionFrom,
        Role::ExemptionTo,
        Role::Exemption,
        Role::Name,
        Role::ItemKind,
        Role::Visibility,
        Role::Relation,
        Role::Related,
        Role::RequiredName,
        Role::Required,
        Role::Modifier,
        Role::RequiredModifier,
        Role::Layer,
        Role::Property,
    ];

    /// The role as the specification writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Role::Unit => "unit",
            Role::Subject => "subject",
            Role::SubjectException => "subject-exception",
            Role::Target => "target",
            Role::UnverifiedTarget => "unverified-target",
            Role::Marker => "marker",
            Role::Slice => "slice",
            Role::Root => "root",
            Role::Kind => "kind",
            Role::Tag => "tag",
            Role::Place => "place",
            Role::User => "user",
            Role::ExemptionFrom => "exemption-from",
            Role::ExemptionTo => "exemption-to",
            Role::Exemption => "exemption",
            Role::Name => "name",
            Role::ItemKind => "item-kind",
            Role::Visibility => "visibility",
            Role::Relation => "relation",
            Role::Related => "related",
            Role::RequiredName => "required-name",
            Role::Required => "required",
            Role::Modifier => "modifier",
            Role::RequiredModifier => "required-modifier",
            Role::Layer => "layer",
            Role::Property => "property",
        }
    }
}
