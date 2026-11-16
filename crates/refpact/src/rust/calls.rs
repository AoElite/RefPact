//! Method calls and field accesses as references.
//!
//! The syntax does not say what `file.read_to_end(..)` calls: that depends on the type of
//! `file`. The index of rust-analyzer says it ([`crate::rust::scip`]). The front end notes
//! each place where such a name is written, as a *site*, and this module gives each site the
//! path that the index names there.

use std::collections::{HashMap, HashSet};

use crate::core::{Certainty, Fact, FactSet, Path, Reference, What};
use crate::rust::kinds::kind;
use crate::rust::scip::{Defined, Form, Index, Place, Symbol};

/// What the front end found for the index to resolve.
#[derive(Debug, Default)]
pub(crate) struct Pending {
    /// The sites: each is a reference without a target, with the position in the facts
    /// before which it belongs.
    pub sites: Vec<(usize, Fact)>,
    /// The functions of the `impl` blocks of traits. They are no declarations.
    pub impl_fns: Vec<ImplFn>,
    /// For each path of several names: the place of its facts, which is its first name, and
    /// the line and the column of its last name.
    pub ends: Vec<(Place, (u32, u32))>,
    /// The places of the macros whose expansion can hold code that the source does not show.
    pub macros: Vec<MacroSite>,
}

impl MacroSite {
    /// The file, the line and the column of the name of the macro.
    pub fn place(&self) -> Place {
        (self.file.clone(), self.line, self.column)
    }
}

/// A derive, an attribute macro or a macro call, at the name of the macro.
#[derive(Debug, Clone)]
pub(crate) struct MacroSite {
    pub file: String,
    pub line: u32,
    pub column: u32,
    /// The name of the macro.
    pub name: String,
    /// The module of the place, by its position in the tree of its crate.
    pub module: usize,
    /// Whether the place is test code, and the `cfg` conditions around it.
    pub test: bool,
    pub cfg: Vec<String>,
    /// The declaration around the place, or the one that an attribute is on.
    pub item: Option<Path>,
    /// The last line of the code that the macro is given: of the item of an attribute, or of
    /// the arguments of a call. The expansion can give this code again.
    pub last: u32,
}

/// A function of an `impl Trait for Type`.
#[derive(Debug, Clone)]
pub(crate) struct ImplFn {
    pub file: String,
    /// The place of the name.
    pub line: u32,
    pub column: u32,
    /// The path of the type and the name of the function.
    pub path: Path,
    /// The paths of the trait, each with the name of the function.
    pub traits: Vec<Path>,
}

/// The crates of the standard library whose items `std` gives again under the same path.
const BEHIND_STD: [&str; 2] = ["core", "alloc"];

/// Adds the sites that the index resolves to the facts, each at its place in their order,
/// and the two kinds to the kinds of the fact set. A site that the index does not resolve
/// gives no fact.
///
/// `packages` gives the first segment of the paths of a package of the fact set, by the name
/// of the package: `crate` for the one crate of a fact set, the name of the crate in a
/// workspace.
pub(crate) fn add(
    facts: &mut FactSet,
    pending: Pending,
    index: &Index,
    packages: &HashMap<String, String>,
    implementations: bool,
) {
    // The functions that implement a function of a trait, by the path of that function.
    let mut implemented: HashMap<&Path, Vec<&Path>> = HashMap::new();
    if implementations {
        for function in &pending.impl_fns {
            for of_trait in &function.traits {
                let known = implemented.entry(of_trait).or_default();
                if !known.contains(&&function.path) {
                    known.push(&function.path);
                }
            }
        }
        for known in implemented.values_mut() {
            known.sort();
        }
    }
    // What the code of the fact set defines at a place: the path, and further paths.
    let mut defined: HashMap<Place, (&Path, &[Path])> = HashMap::new();
    for function in &pending.impl_fns {
        let place = (function.file.clone(), function.line, function.column);
        defined.insert(place, (&function.path, &function.traits));
    }
    for declaration in &facts.declarations {
        let place = (
            declaration.file.clone(),
            declaration.line,
            declaration.column,
        );
        defined.insert(place, (&declaration.path, &[]));
    }

    // The names that the code of the fact set defines and that the syntax does not show, such
    // as an item that a macro makes from its arguments, are known paths. They are not
    // declarations: the index does not say their kind or their visibility.
    let known: HashSet<&Path> = facts.items.iter().collect();
    let mut generated: Vec<Path> = Vec::new();
    for (symbol, place) in index.definitions() {
        if defined.contains_key(place) || !packages.contains_key(symbol.package) {
            continue;
        }
        let path = written(&symbol, packages);
        if !known.contains(&path) && !generated.contains(&path) {
            generated.push(path);
        }
    }
    drop(known);

    let mut resolved: Vec<(usize, Fact)> = Vec::new();
    for (before, mut fact) in pending.sites {
        let What::Reference(reference) = &mut fact.what else {
            continue;
        };
        let place = (fact.file.clone(), fact.line, fact.column);
        let Some((symbol, definition)) = index.at(&place) else {
            continue;
        };
        // A call names a function, and a field access a field. A path, such as
        // `Self::new`, can name both.
        let fits = match reference.kind.as_str() {
            kind::METHOD_CALL => symbol.form == Form::Method,
            kind::FIELD => symbol.form == Form::Term,
            _ => true,
        };
        if !fits {
            continue;
        }
        match definition.and_then(|place| defined.get(place)) {
            Some((path, more)) => {
                reference.target = (*path).clone();
                reference.aliases = more.to_vec();
            }
            None => {
                reference.target = written(&symbol, packages);
                if BEHIND_STD.contains(&reference.target[0].as_str()) {
                    let mut through_std = reference.target.clone();
                    through_std[0] = "std".to_owned();
                    reference.aliases = vec![through_std];
                }
            }
        }
        if let Some(name) = symbol.of_trait {
            fact.note = Some(format!("of the trait `{name}`"));
        }
        // A call of a function of a trait can run each function that implements it.
        let (target, kind) = (reference.target.clone(), reference.kind.clone());
        let behind = implemented.get(&target).into_iter().flatten();
        let behind: Vec<Fact> = behind
            .map(|function| Fact {
                note: Some(format!("may be called through `{}`", target.join("::"))),
                what: What::Reference(Reference {
                    target: (*function).clone(),
                    aliases: Vec::new(),
                    kind: kind.clone(),
                    certainty: Certainty::Candidate,
                    properties: Vec::new(),
                }),
                ..fact.clone()
            })
            .collect();
        resolved.push((before, fact));
        resolved.extend(behind.into_iter().map(|may| (before, may)));
    }
    drop(defined);
    generated.sort();
    facts.items.extend(generated);

    let wrong = definitions(&mut facts.facts, &pending.ends, index, packages);

    let mut all = Vec::with_capacity(facts.facts.len() + resolved.len());
    let mut resolved = resolved.into_iter().peekable();
    for (position, fact) in std::mem::take(&mut facts.facts).into_iter().enumerate() {
        while let Some((_, site)) = resolved.next_if(|(before, _)| *before <= position) {
            all.push(site);
        }
        if !wrong[position] {
            all.push(fact);
        }
    }
    all.extend(resolved.map(|(_, site)| site));
    facts.facts = all;
    for kind in [kind::METHOD_CALL, kind::FIELD] {
        facts.kinds.push(kind.to_owned());
    }
}

/// Gives each path that the syntax resolved what the index knows of it.
///
/// - A path into a crate outside the fact set gets the path of the definition as an alias,
///   so a rule that names `rand_core::OsRng` sees a use that is written `rand::rngs::OsRng`.
/// - A candidate, which is a name that a glob import may bring in, is exact when the index
///   names the same item, and is wrong when the index names another item or a local
///   variable. The result says which facts are wrong, by their position.
fn definitions(
    facts: &mut [Fact],
    ends: &[(Place, (u32, u32))],
    index: &Index,
    packages: &HashMap<String, String>,
) -> Vec<bool> {
    let ends: HashMap<&Place, (u32, u32)> = ends.iter().map(|(place, end)| (place, *end)).collect();
    let mut wrong = vec![false; facts.len()];
    let mut start = 0;
    while start < facts.len() {
        // The facts of one path are next to each other and have one place.
        let first = &facts[start];
        let place = (first.file.clone(), first.line, first.column);
        let mut end = start + 1;
        while end < facts.len()
            && (facts[end].line, facts[end].column) == (place.1, place.2)
            && facts[end].file == place.0
        {
            end += 1;
        }
        let last = match ends.get(&place) {
            Some((line, column)) => (place.0, *line, *column),
            None => place,
        };
        let found = index.named_at(&last).map(|named| Found {
            inside: packages.contains_key(named.package),
            is_macro: named.is_macro,
            path: path_of(&named, packages),
        });
        refine(
            &mut facts[start..end],
            &mut wrong[start..end],
            found,
            || index.is_local(&last),
        );
        start = end;
    }
    wrong
}

/// What the index names at a path.
struct Found {
    /// The path of the definition.
    path: Path,
    /// Whether a crate of the fact set defines it.
    inside: bool,
    is_macro: bool,
}

/// The path of a definition in the names of the fact set.
fn path_of(named: &Defined<'_>, packages: &HashMap<String, String>) -> Path {
    let first = match packages.get(named.package) {
        Some(name) => name.clone(),
        None => named.package.replace('-', "_"),
    };
    let mut path = vec![first];
    path.extend(named.names.iter().map(|name| (*name).to_owned()));
    path
}

fn reference(fact: &Fact) -> Option<&Reference> {
    match &fact.what {
        What::Reference(reference) => Some(reference),
        What::Marker(_) => None,
    }
}

/// `std` gives the items of `core` and of `alloc` again, so the three are one crate for a name
/// that a glob import of `std` brings in.
fn library(name: &str) -> &str {
    if BEHIND_STD.contains(&name) || name == "std" {
        "std"
    } else {
        name
    }
}

/// The facts of one path, with the definition that the index names at the path.
fn refine(
    facts: &mut [Fact],
    wrong: &mut [bool],
    found: Option<Found>,
    is_local: impl Fn() -> bool,
) {
    let candidates: Vec<usize> = (0..facts.len())
        .filter(|&at| reference(&facts[at]).is_some_and(|r| r.certainty == Certainty::Candidate))
        .collect();
    let Some(found) = found else {
        // A local variable is not a name of a glob import.
        if !candidates.is_empty() && is_local() {
            for at in candidates {
                wrong[at] = true;
            }
        }
        return;
    };
    // A macro that is exported is at the root of its crate, and the index names its module.
    if found.is_macro {
        return;
    }
    let definition = found.path;
    // The candidates that the definition proves: those that name the definition, or else the
    // one candidate in the crate of the definition. When the definition is what an exact
    // fact of the path names, no candidate is right.
    let names_definition = |at: &usize| {
        reference(&facts[*at])
            .is_some_and(|r| r.target == definition || r.aliases.contains(&definition))
    };
    let same_crate = |at: &usize| {
        reference(&facts[*at]).is_some_and(|r| {
            r.target.last() == definition.last()
                && r.target.first().map(|name| library(name)) == Some(library(&definition[0]))
        })
    };
    let mut proved: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(names_definition)
        .collect();
    if proved.is_empty() {
        let near: Vec<usize> = candidates.iter().copied().filter(same_crate).collect();
        if near.len() == 1 {
            proved = near;
        }
    }
    // The definition is in a crate that no candidate names, so a crate gives it again. The
    // standard library does not give the items of other crates, and its prelude does not
    // give this name.
    if proved.is_empty() && !found.inside && library(&definition[0]) != "std" {
        let possible = |at: &usize| {
            reference(&facts[*at]).is_some_and(|r| {
                r.target.last() == definition.last()
                    && r.target.first().is_some_and(|name| library(name) != "std")
            })
        };
        let possible: Vec<usize> = candidates.iter().copied().filter(possible).collect();
        if possible.len() == 1 {
            proved = possible;
        }
    }
    let exact = (0..facts.len()).any(|at| !candidates.contains(&at) && names_definition(&at));
    if exact || !proved.is_empty() {
        for at in &candidates {
            wrong[*at] = !proved.contains(at);
        }
    }
    // With a candidate that is proved, another reading of the path is wrong: `rngs::OsRng`
    // after `use rand::*;` is not an item of a crate `rngs`.
    if !proved.is_empty() {
        for at in 0..facts.len() {
            if !candidates.contains(&at) && reference(&facts[at]).is_some() {
                wrong[at] = !names_definition(&at);
            }
        }
    }
    for (at, fact) in facts.iter_mut().enumerate() {
        if wrong[at] {
            continue;
        }
        let What::Reference(reference) = &mut fact.what else {
            continue;
        };
        if proved.contains(&at) {
            reference.certainty = Certainty::Exact;
            if let Some(note) = &mut fact.note {
                *note = note.replacen("may come through", "through", 1);
            }
        } else if reference.certainty == Certainty::Candidate {
            continue;
        }
        // The resolver follows the re-exports of the crates of the fact set itself.
        let known = reference.target == definition || reference.aliases.contains(&definition);
        if !found.inside && !known {
            reference.aliases.push(definition.clone());
        }
    }
}

/// The path of a symbol as the index writes it: the crate, the modules, the type or the
/// trait, and the name. For a function of an `impl`, the modules are those of the `impl`.
fn written(symbol: &Symbol<'_>, packages: &HashMap<String, String>) -> Path {
    let first = match packages.get(symbol.package) {
        Some(name) => name.clone(),
        None => symbol.package.replace('-', "_"),
    };
    let mut path = vec![first];
    path.extend(symbol.modules.iter().map(|name| (*name).to_owned()));
    path.extend(symbol.owner.map(str::to_owned));
    path.push(symbol.name.to_owned());
    path
}
