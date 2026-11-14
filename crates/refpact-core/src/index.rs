//! Lookups by path that an evaluation needs many times.
//!
//! A fact set is plain data: lists of paths. To find the unit of a path by a search of the
//! list costs time in proportion to the number of units, for each fact. The index is built
//! once for an evaluation and answers from a table.

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};

use crate::checks::error;
use crate::error::{ErrorCode, Role, RuleError};
use crate::model::{FactSet, Reference, What};
use crate::pattern::join;
use crate::pattern::{Path, Pattern};

/// A fast hash for paths.
///
/// The hash of the standard library is made to resist keys that an attacker chooses, and
/// that costs time. The keys here are the paths of the code under test, and an evaluation
/// hashes several paths for each fact.
#[derive(Default)]
pub(crate) struct PathHasher(u64);

impl Hasher for PathHasher {
    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.0 = (self.0.rotate_left(5) ^ u64::from_le_bytes(word))
                .wrapping_mul(0x517c_c1b7_2722_0a95);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A set with the fast hash.
pub(crate) type FastSet<T> = std::collections::HashSet<T, BuildHasherDefault<PathHasher>>;

/// A fast hash for numbers, such as the pair of nodes of an edge. Each number is mixed into
/// all bits, because the table takes the low bits for the bucket and the high bits for the
/// tag.
#[derive(Default)]
pub(crate) struct NumberHasher(u64);

impl Hasher for NumberHasher {
    fn write(&mut self, bytes: &[u8]) {
        for &byte in bytes {
            self.write_u64(u64::from(byte));
        }
    }

    fn write_u64(&mut self, number: u64) {
        // The finaliser of SplitMix64 over the running state.
        let mut x = (self.0 ^ number).wrapping_add(0x9e37_79b9_7f4a_7c15);
        x = (x ^ (x >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        x = (x ^ (x >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        self.0 = x ^ (x >> 31);
    }

    fn write_usize(&mut self, number: usize) {
        self.write_u64(number as u64);
    }

    fn finish(&self) -> u64 {
        self.0
    }
}

/// A set of numbers or pairs of numbers with the fast hash.
pub(crate) type NumberSet<T> = std::collections::HashSet<T, BuildHasherDefault<NumberHasher>>;

/// A table with the fast hash.
pub(crate) type FastMap<K, V> = HashMap<K, V, BuildHasherDefault<PathHasher>>;

/// The units of a fact set by path, owned so that the tables outlive one evaluation.
type UnitNumbers = HashMap<Path, usize, BuildHasherDefault<PathHasher>>;

/// What the evaluations of all rules over one fact set share: the lookups by path, made once.
/// The number of a unit is its position in `units`. When two units have the same path, the
/// first one stands for both.
#[derive(Debug, Clone)]
pub(crate) struct Tables {
    numbers: UnitNumbers,
    /// For each fact, the number of its unit. `None` when the fact set does not have the unit.
    fact_units: Vec<Option<usize>>,
    /// For each unit, the first fact of the unit.
    first_facts: Vec<Option<usize>>,
    /// The owners of the paths of the references, `[target, aliases..]` of each fact one
    /// after the other. The paths of fact `n` start at `owner_starts[n]`.
    owners: Vec<Option<usize>>,
    owner_starts: Vec<usize>,
    /// For each unit, the units that are a prefix of it, the longest first.
    unit_prefixes: Vec<Vec<usize>>,
    /// The errors of an invalid fact set (SPEC section 3). Empty when it is valid.
    pub(crate) invalid: Vec<RuleError>,
}

impl Tables {
    pub(crate) fn new(facts: &FactSet) -> Tables {
        let mut numbers: UnitNumbers =
            HashMap::with_capacity_and_hasher(facts.units.len(), BuildHasherDefault::default());
        for (number, unit) in facts.units.iter().enumerate() {
            if !numbers.contains_key(unit.as_slice()) {
                numbers.insert(unit.clone(), number);
            }
        }
        let unit = |path: &[String]| numbers.get(path).copied();
        let prefixes = |path: &[String]| -> Vec<usize> {
            (0..=path.len())
                .rev()
                .filter_map(|length| unit(&path[..length]))
                .collect()
        };
        // The facts of one unit follow each other in most fact sets, so the unit of a fact
        // is most often the unit of the fact before it. To compare is faster than to look up.
        let mut fact_units: Vec<Option<usize>> = Vec::with_capacity(facts.facts.len());
        let mut before: Option<(&[String], Option<usize>)> = None;
        for fact in &facts.facts {
            let number = match before {
                Some((path, number)) if path == fact.unit.as_slice() => number,
                _ => unit(&fact.unit),
            };
            before = Some((fact.unit.as_slice(), number));
            fact_units.push(number);
        }
        let mut first_facts = vec![None; facts.units.len()];
        for (fact, number) in fact_units.iter().enumerate() {
            if let Some(number) = *number {
                first_facts[number].get_or_insert(fact);
            }
        }
        // Many references name the same path, so the owner of each path is found once.
        let mut known: FastMap<&[String], Option<usize>> = FastMap::default();
        let mut owners = Vec::with_capacity(facts.facts.len());
        let mut owner_starts = Vec::with_capacity(facts.facts.len() + 1);
        for fact in &facts.facts {
            owner_starts.push(owners.len());
            if let What::Reference(reference) = &fact.what {
                for path in reference.paths() {
                    let owner = *known.entry(path.as_slice()).or_insert_with(|| {
                        (0..=path.len())
                            .rev()
                            .find_map(|length| unit(&path[..length]))
                    });
                    owners.push(owner);
                }
            }
        }
        owner_starts.push(owners.len());
        let unit_prefixes = facts.units.iter().map(|path| prefixes(path)).collect();
        let mut tables = Tables {
            numbers,
            fact_units,
            first_facts,
            owners,
            owner_starts,
            unit_prefixes,
            invalid: Vec::new(),
        };
        tables.invalid = invalid_facts(facts, &tables);
        tables
    }
}

/// The lookups of one evaluation: the tables of the fact set and what depends on the rule.
pub(crate) struct Index<'a> {
    pub(crate) facts: &'a FactSet,
    tables: &'a Tables,
    /// For each fact, whether an exemption of the rule lets it pass. Empty when the rule has
    /// no exemption.
    exempt: Vec<bool>,
}

impl<'a> Index<'a> {
    pub(crate) fn new(facts: &'a FactSet, tables: &'a Tables) -> Index<'a> {
        Index {
            facts,
            tables,
            exempt: Vec::new(),
        }
    }

    /// Sets the facts that the exemptions of the rule let pass, by position.
    pub(crate) fn set_exempt(&mut self, exempt: Vec<bool>) {
        self.exempt = exempt;
    }

    /// SPEC 4.13: whether an exemption lets the fact pass, by the position of the fact.
    pub(crate) fn exempt(&self, fact: usize) -> bool {
        self.exempt.get(fact).copied().unwrap_or(false)
    }

    /// The number of the unit with this path.
    pub(crate) fn unit(&self, path: &[String]) -> Option<usize> {
        self.tables.numbers.get(path).copied()
    }

    /// The number of the unit of a fact, by the position of the fact.
    pub(crate) fn unit_of_fact(&self, fact: usize) -> Option<usize> {
        self.tables.fact_units[fact]
    }

    /// The first fact of a unit, by position.
    pub(crate) fn first_fact(&self, unit: usize) -> Option<usize> {
        self.tables.first_facts[unit]
    }

    /// The units that are a prefix of a unit, itself first.
    pub(crate) fn unit_prefixes(&self, unit: usize) -> &[usize] {
        &self.tables.unit_prefixes[unit]
    }

    /// The paths of the reference of a fact, each with its owner: the longest unit that is
    /// a prefix of it.
    pub(crate) fn owned_paths<'r>(
        &'r self,
        fact: usize,
        reference: &'r Reference,
    ) -> impl Iterator<Item = (&'r Path, Option<usize>)> + 'r {
        let owners =
            &self.tables.owners[self.tables.owner_starts[fact]..self.tables.owner_starts[fact + 1]];
        reference.paths().zip(owners.iter().copied())
    }

    /// Check 5 of SPEC 4.2: the first path of the reference that is forbidden in `unit`.
    pub(crate) fn forbidden<'r>(
        &'r self,
        fact: usize,
        unit: Option<usize>,
        reference: &'r Reference,
        targets: &[Pattern],
    ) -> Option<&'r Path> {
        self.owned_paths(fact, reference)
            .find(|(path, owner)| {
                targets.iter().any(|pattern| pattern.matches_within(path)) && *owner != unit
            })
            .map(|(path, _)| path)
    }
}

/// Check 1 of SPEC section 5: the errors of a fact set that is not valid.
fn invalid_facts(facts: &FactSet, tables: &Tables) -> Vec<RuleError> {
    let unit = |path: &[String]| tables.numbers.get(path).copied();
    let mut errors: Vec<RuleError> = Vec::new();
    let mut report = |role: Role, value: String| {
        let entry = error(ErrorCode::InvalidFacts, Some(role), Some(value));
        if !errors.contains(&entry) {
            errors.push(entry);
        }
    };
    for (position, fact) in facts.facts.iter().enumerate() {
        if tables.fact_units[position].is_none() {
            report(Role::Unit, join(&fact.unit, &facts.separator));
        }
        match &fact.what {
            What::Reference(reference) => {
                if !facts.kinds.contains(&reference.kind) {
                    report(Role::Kind, reference.kind.clone());
                }
                for property in &reference.properties {
                    if !facts.properties.contains(&property.name) {
                        report(Role::Property, property.name.clone());
                    }
                }
            }
            What::Marker(marker) => {
                if !facts.markers.contains(marker) {
                    report(Role::Marker, marker.clone());
                }
            }
        }
        for tag in &fact.tags {
            if !facts.tags.contains(tag) {
                report(Role::Tag, tag.clone());
            }
        }
    }
    for declaration in &facts.declarations {
        if unit(&declaration.unit).is_none() {
            report(Role::Unit, join(&declaration.unit, &facts.separator));
        }
        if !facts.item_kinds.contains(&declaration.kind) {
            report(Role::ItemKind, declaration.kind.clone());
        }
        if !facts.visibilities.contains(&declaration.visibility) {
            report(Role::Visibility, declaration.visibility.clone());
        }
        for tie in &declaration.relations {
            if !facts.relations.contains(&tie.relation) {
                report(Role::Relation, tie.relation.clone());
            }
        }
        for tag in &declaration.tags {
            if !facts.tags.contains(tag) {
                report(Role::Tag, tag.clone());
            }
        }
        for modifier in &declaration.modifiers {
            if !facts.modifiers.contains(modifier) {
                report(Role::Modifier, modifier.clone());
            }
        }
    }
    errors
}
