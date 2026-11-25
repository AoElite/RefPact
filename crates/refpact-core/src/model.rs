//! Facts as plain data (SPEC section 3), and the parts of rules that the checks share.

use crate::pattern::{is_prefix, Path};

/// What a front end found in one project.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FactSet {
    /// Text that separates segments when a path is printed.
    pub separator: String,
    /// Paths of all units. Rust: modules. TypeScript: files.
    pub units: Vec<Path>,
    /// The number of lines of each unit, in the order of `units`. Empty for front ends that
    /// do not count them.
    pub unit_lines: Vec<u32>,
    /// The file of each unit, in the order of `units`. Empty for front ends that do not
    /// give them: the file of a unit is then that of its first fact.
    pub unit_files: Vec<String>,
    /// Paths of all known named things inside units.
    pub items: Vec<Path>,
    /// All reference kinds the front end can produce.
    pub kinds: Vec<String>,
    /// All markers the front end can produce.
    pub markers: Vec<String>,
    /// All tags the front end can produce.
    pub tags: Vec<String>,
    /// The facts, in a stable order.
    pub facts: Vec<Fact>,
    /// The named items that the units declare, in a stable order.
    pub declarations: Vec<Declaration>,
    /// All item kinds the front end can produce.
    pub item_kinds: Vec<String>,
    /// The item kinds of the members of items, such as the fields and the methods of a type.
    /// A rule about items selects a declaration of such a kind only when the kinds of the
    /// rule name it. Default: none.
    pub member_kinds: Vec<String>,
    /// All visibilities the front end can produce.
    pub visibilities: Vec<String>,
    /// All relations the front end can produce.
    pub relations: Vec<String>,
    /// All modifiers of declarations the front end can produce.
    pub modifiers: Vec<String>,
    /// The names of all properties of references the front end can produce.
    pub properties: Vec<String>,
    /// Segments that reports print without a separator before them, by prefix. Default:
    /// none. See [`crate::PathFormat`].
    pub attached_prefixes: Vec<String>,
    /// Prefixes that reports leave out of the first segment of a path. Default: none.
    pub hidden_prefixes: Vec<String>,
}

impl FactSet {
    /// The number of lines of the unit at a position of `units`. `None` when the front end
    /// does not count them.
    pub fn lines_of(&self, unit: usize) -> Option<u32> {
        self.unit_lines.get(unit).copied()
    }

    /// The owner of a path: the longest unit that is a prefix of it.
    ///
    /// This looks at every unit. An evaluation does not call it: it has a table of the units.
    pub fn owner(&self, path: &[String]) -> Option<&Path> {
        self.units
            .iter()
            .filter(|unit| is_prefix(unit, path))
            .max_by_key(|unit| unit.len())
    }
}

/// One observation at one place in the source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fact {
    /// The unit that contains the fact.
    pub unit: Path,
    /// The file, as the front end names it.
    pub file: String,
    /// The line in the file.
    pub line: u32,
    /// The column in the line.
    pub column: u32,
    /// The tags of the fact, such as the tag of test code.
    pub tags: Vec<String>,
    /// Text that is copied to the violation.
    pub note: Option<String>,
    /// The path of the innermost declaration that contains the fact, such as the function
    /// whose body has the reference. `None` outside every declaration, and for front ends
    /// that do not record it.
    pub item: Option<Path>,
    /// What was observed.
    pub what: What,
}

/// A named item that a unit declares, such as a type or a function (SPEC section 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declaration {
    /// The path of the item.
    pub path: Path,
    /// The name of the item as the code writes it.
    pub name: String,
    /// The unit that declares the item.
    pub unit: Path,
    /// The file, as the front end names it.
    pub file: String,
    /// The line of the name.
    pub line: u32,
    /// The column of the name.
    pub column: u32,
    /// The line on which the item ends. `None` for front ends that do not record it.
    pub end_line: Option<u32>,
    /// The tags of the declaration, such as the tag of test code.
    pub tags: Vec<String>,
    /// The kind of item, one of the item kinds of the fact set.
    pub kind: String,
    /// The visibility, one of the visibilities of the fact set.
    pub visibility: String,
    /// The paths that the item is tied to, such as the traits that it implements.
    pub relations: Vec<Relation>,
    /// What the declaration says of the item besides its kind and its visibility, such as
    /// `async` or `documented`. Each is one of the modifiers of the fact set.
    pub modifiers: Vec<String>,
}

impl Declaration {
    /// The number of lines from the line of the name to the last line of the item. `None`
    /// when the front end does not record where the item ends.
    pub fn lines(&self) -> Option<u32> {
        Some(self.end_line?.saturating_sub(self.line) + 1)
    }
}

/// A tie between a declared item and a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relation {
    /// The kind of tie, one of the relations of the fact set.
    pub relation: String,
    /// The path.
    pub target: Path,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// What a fact observes.
pub enum What {
    /// The unit refers to a path.
    Reference(Reference),
    /// The unit contains something that has a name but no path, such as an unsafe block.
    Marker(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// A reference from a unit to a path.
pub struct Reference {
    /// The path the code refers to, resolved as far as the front end can.
    pub target: Path,
    /// Further paths the same reference denotes, such as the definition behind a re-export.
    pub aliases: Vec<Path>,
    /// The kind of the reference, one of the kinds of the fact set.
    pub kind: String,
    /// Whether the front end proved the path.
    pub certainty: Certainty,
    /// What the front end knows of the target besides its path, such as the version of a
    /// package. No rule of the core reads them; a check of the user does.
    pub properties: Vec<Property>,
}

/// One thing that a front end knows of the target of a reference: a name and a value, both
/// opaque text to the core.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Property {
    /// The name of the property, such as `version`.
    pub name: String,
    /// Its value.
    pub value: String,
}

impl Reference {
    /// The value of the property with a name, or `None` when the reference has no such
    /// property.
    pub fn property(&self, name: &str) -> Option<&str> {
        let found = self.properties.iter().find(|one| one.name == name);
        found.map(|one| one.value.as_str())
    }

    /// The list `[target, aliases...]`.
    pub(crate) fn paths(&self) -> impl Iterator<Item = &Path> {
        std::iter::once(&self.target).chain(&self.aliases)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// How sure the front end is of a path.
pub enum Certainty {
    /// The front end resolved the path.
    Exact,
    /// The front end could not prove this path, for example a name behind a glob import.
    Candidate,
}

impl Certainty {
    /// The certainty as the specification writes it.
    pub fn as_str(self) -> &'static str {
        match self {
            Certainty::Exact => "exact",
            Certainty::Candidate => "candidate",
        }
    }
}

/// A raw pattern: segments that the evaluation validates.
pub type RawPattern = Vec<String>;

/// Targets of a rule about uses: the union of both lists.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Targets {
    /// Checked for existence when they are inside the analysed project.
    pub any_of: Vec<RawPattern>,
    /// Never checked for existence.
    pub unverified: Vec<RawPattern>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
/// Which reference kinds a rule looks at.
pub enum KindFilter {
    /// All kinds.
    All,
    /// These kinds only.
    Only(Vec<String>),
    /// All kinds except these.
    Except(Vec<String>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
/// Whether a rule looks at candidate references.
pub enum Candidates {
    /// Candidates count like exact references.
    Include,
    /// Candidates are not checked.
    Ignore,
}
