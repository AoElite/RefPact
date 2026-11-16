//! The code that macros write.
//!
//! The source shows the arguments of a macro, not what the macro makes of them: a derive
//! that writes a call of `std::fs`, an attribute that wraps a function, the body of a
//! `macro_rules!`. rust-analyzer expands each macro, also a procedural one. This module asks
//! it for the expansion of each macro of the code, reads the expansion as the source is
//! read, and adds what it finds at the place of the macro, with the tag [`TAG`].

use std::collections::{HashMap, HashSet};

use crate::core::{Declaration, Fact, FactSet, Path, What};
use crate::rust::calls::MacroSite;
use crate::rust::extract;
use crate::rust::items;
use crate::rust::scope::Module;
use crate::rust::sources::Expansions;
use crate::rust::syntax::LoadedFile;

/// The tag of what the expansion of a macro holds.
pub(crate) const TAG: &str = "generated";

/// The function that holds an expansion that is no list of items.
const WRAPPER: &str = "__refpact_expansion";

/// Reads the expansion of each macro and adds what it holds to the facts.
///
/// `sites` are the macros of one crate, whose module tree is `modules`. `rename` gives the
/// name of the crate in a workspace and the directory of the crate: the facts are then in
/// the names and the files of the workspace, and the expansions are too.
pub(crate) fn add(
    facts: &mut FactSet,
    modules: &[Module],
    sites: &[MacroSite],
    expansions: &Expansions,
    rename: Option<(&str, &str)>,
) {
    let in_workspace = |file: &str| match rename {
        Some((_, dir)) => crate::rust::sources::join(dir, file),
        None => file.to_owned(),
    };
    let renamed = |path: &mut Path| {
        if let (Some((name, _)), Some("crate")) = (rename, path.first().map(String::as_str)) {
            path[0] = name.to_owned();
        }
    };
    // What the source has in each file, by line: an expansion that gives the code of its
    // macro again does not add it a second time.
    let mut written: HashMap<&str, Vec<(u32, &str, &Path)>> = HashMap::new();
    for fact in &facts.facts {
        if let What::Reference(reference) = &fact.what {
            let known = written.entry(fact.file.as_str()).or_default();
            known.push((fact.line, reference.kind.as_str(), &reference.target));
        }
    }
    let mut declared: HashSet<Path> = facts.declarations.iter().map(|d| d.path.clone()).collect();

    let mut new_facts: HashMap<String, Vec<Fact>> = HashMap::new();
    let mut new_declarations: Vec<Declaration> = Vec::new();
    let mut impls = Vec::new();
    for site in sites {
        let file = in_workspace(&site.file);
        let Some(text) = expansions.get(&(file.clone(), site.line, site.column)) else {
            continue;
        };
        let Some(ast) = parse(text) else {
            continue;
        };
        let place = LoadedFile {
            path: site.file.clone(),
            module: site.module,
            test: site.test,
            cfg: site.cfg.clone(),
        };
        let found = extract::extract(modules, &place, &ast, false);
        let wrapper = |path: &Path| path.last().is_some_and(|name| name == WRAPPER);
        let given: HashSet<(&str, &Path)> = written
            .get(file.as_str())
            .into_iter()
            .flatten()
            .filter(|(line, ..)| (site.line..=site.last).contains(line))
            .map(|(_, kind, target)| (*kind, *target))
            .collect();
        let note = format!("in the expansion of `{}`", site.name);
        for mut fact in found.facts {
            fact.file.clone_from(&file);
            fact.line = site.line;
            fact.column = site.column;
            fact.note = Some(note.clone());
            fact.tags.push(TAG.to_owned());
            if fact.item.as_ref().map_or(true, wrapper) {
                fact.item.clone_from(&site.item);
            }
            renamed(&mut fact.unit);
            fact.item.as_mut().map(renamed);
            if let What::Reference(reference) = &mut fact.what {
                renamed(&mut reference.target);
                reference.aliases.iter_mut().for_each(renamed);
                if given.contains(&(reference.kind.as_str(), &reference.target)) {
                    continue;
                }
            }
            let known = new_facts.entry(file.clone()).or_default();
            if !known.contains(&fact) {
                known.push(fact);
            }
        }
        for mut declaration in found.declarations {
            renamed(&mut declaration.path);
            renamed(&mut declaration.unit);
            if wrapper(&declaration.path) || !declared.insert(declaration.path.clone()) {
                continue;
            }
            declaration.file.clone_from(&file);
            declaration.line = site.line;
            declaration.column = site.column;
            declaration.end_line = Some(site.line);
            declaration.tags.push(TAG.to_owned());
            for tie in &mut declaration.relations {
                renamed(&mut tie.target);
            }
            new_declarations.push(declaration);
        }
        for mut found in found.impls {
            found.types.iter_mut().for_each(renamed);
            found.traits.iter_mut().for_each(renamed);
            impls.push(found);
        }
    }
    drop(written);

    // Each new fact goes after the facts of its file that are before it in the text.
    let mut all = Vec::with_capacity(facts.facts.len());
    let mut old = std::mem::take(&mut facts.facts).into_iter().peekable();
    while let Some(first) = old.next() {
        let file = first.file.clone();
        let mut of_file = vec![first];
        while let Some(next) = old.next_if(|next| next.file == file) {
            of_file.push(next);
        }
        of_file.extend(new_facts.remove(&file).unwrap_or_default());
        of_file.sort_by_key(|fact| (fact.line, fact.column));
        all.extend(of_file);
    }
    // A file that had no fact.
    let mut rest: Vec<(String, Vec<Fact>)> = new_facts.into_iter().collect();
    rest.sort_by(|a, b| a.0.cmp(&b.0));
    all.extend(rest.into_iter().flat_map(|(_, found)| found));
    facts.facts = all;

    for declaration in &new_declarations {
        if !facts.items.contains(&declaration.path) {
            facts.items.push(declaration.path.clone());
        }
    }
    facts.declarations.extend(new_declarations);
    items::attach_impls(&mut facts.declarations, &impls);
}

/// An expansion as a file: a list of items, or else statements, which are read as the body
/// of a function.
fn parse(text: &str) -> Option<syn::File> {
    syn::parse_file(text)
        .or_else(|_| syn::parse_file(&format!("fn {WRAPPER}() {{ {text} }}")))
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_expansion_is_a_list_of_items_or_the_body_of_a_function() {
        assert_eq!(parse("impl A { fn f() {} }").unwrap().items.len(), 1);
        let body = parse("std::time::Instant::now()").unwrap();
        assert!(matches!(&body.items[0], syn::Item::Fn(function) if function.sig.ident == WRAPPER));
        assert!(parse("let a = ;").is_none());
    }
}
