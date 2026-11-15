//! Reads a fact set straight from the text, without the tree of a JSON value.
//!
//! A fact set is the one large value of the wire format: one object for each fact, with
//! paths in it. To build a tree first and then the fact set would make each value twice.
//! The messages are those of the other readers: `facts[3].line: expected a whole number`.

use std::borrow::Cow;

use super::json::Reader;
use super::{context, item_context, Read};
use crate::model::{Certainty, Declaration, Fact, FactSet, Property, Reference, Relation, What};
use crate::pattern::Path;

/// The value of a key that must be present.
fn required<T>(value: Option<T>, key: &str) -> Read<T> {
    value.ok_or_else(|| format!("{key}: missing"))
}

/// Reads the value of one key, and names the key in an error.
fn at<T>(key: &str, read: Read<T>) -> Read<T> {
    read.map_err(|error| context(key, error))
}

fn owned(text: Cow<'_, str>) -> String {
    text.into_owned()
}

fn list<'t, T>(
    reader: &mut Reader<'t>,
    mut read: impl FnMut(&mut Reader<'t>) -> Read<T>,
) -> Read<Vec<T>> {
    let mut items = Vec::new();
    reader.items(|reader, index| {
        items.push(read(reader).map_err(|error| item_context(index, error))?);
        Ok(())
    })?;
    Ok(items)
}

/// A list that may be `null`, which is then empty.
fn optional_list<'t, T>(
    reader: &mut Reader<'t>,
    read: impl FnMut(&mut Reader<'t>) -> Read<T>,
) -> Read<Vec<T>> {
    if reader.null() {
        return Ok(Vec::new());
    }
    list(reader, read)
}

fn strings(reader: &mut Reader<'_>) -> Read<Vec<String>> {
    list(reader, |reader| reader.text().map(owned))
}

fn optional_strings(reader: &mut Reader<'_>) -> Read<Vec<String>> {
    optional_list(reader, |reader| reader.text().map(owned))
}

fn paths(reader: &mut Reader<'_>) -> Read<Vec<Path>> {
    list(reader, strings)
}

/// Reads a fact set (SPEC section 8).
pub(super) fn fact_set(reader: &mut Reader<'_>) -> Read<FactSet> {
    let mut separator = None;
    let mut units = None;
    let mut items = None;
    let mut kinds = None;
    let mut markers = None;
    let mut tags = None;
    let mut facts = None;
    let mut set = FactSet::default();
    reader.fields(|reader, key| {
        match key.as_ref() {
            "separator" => separator = Some(at("separator", reader.text().map(owned))?),
            "units" => units = Some(at("units", paths(reader))?),
            "unitLines" => {
                set.unit_lines = at("unitLines", optional_list(reader, |reader| reader.whole()))?;
            }
            "unitFiles" => {
                set.unit_files = at(
                    "unitFiles",
                    optional_list(reader, |reader| reader.text().map(owned)),
                )?;
            }
            "items" => items = Some(at("items", paths(reader))?),
            "kinds" => kinds = Some(at("kinds", strings(reader))?),
            "markers" => markers = Some(at("markers", strings(reader))?),
            "tags" => tags = Some(at("tags", strings(reader))?),
            "facts" => facts = Some(at("facts", list(reader, fact))?),
            "declarations" => {
                set.declarations = at("declarations", optional_list(reader, declaration))?;
            }
            "itemKinds" => set.item_kinds = at("itemKinds", optional_strings(reader))?,
            "visibilities" => set.visibilities = at("visibilities", optional_strings(reader))?,
            "relations" => set.relations = at("relations", optional_strings(reader))?,
            "modifiers" => set.modifiers = at("modifiers", optional_strings(reader))?,
            "properties" => set.properties = at("properties", optional_strings(reader))?,
            "attachedPrefixes" => {
                set.attached_prefixes = at("attachedPrefixes", optional_strings(reader))?;
            }
            "hiddenPrefixes" => {
                set.hidden_prefixes = at("hiddenPrefixes", optional_strings(reader))?;
            }
            _ => {
                reader.any()?;
            }
        }
        Ok(())
    })?;
    set.separator = required(separator, "separator")?;
    set.units = required(units, "units")?;
    set.items = required(items, "items")?;
    set.kinds = required(kinds, "kinds")?;
    set.markers = required(markers, "markers")?;
    set.tags = required(tags, "tags")?;
    set.facts = required(facts, "facts")?;
    Ok(set)
}

fn fact(reader: &mut Reader<'_>) -> Read<Fact> {
    let mut unit = None;
    let mut file = None;
    let mut line = None;
    let mut column = None;
    let mut tags = Vec::new();
    let mut note = None;
    let mut item = None;
    let mut what = None;
    reader.fields(|reader, key| {
        match key.as_ref() {
            "unit" => unit = Some(at("unit", strings(reader))?),
            "file" => file = Some(at("file", reader.text().map(owned))?),
            "line" => line = Some(at("line", reader.whole())?),
            "column" => column = Some(at("column", reader.whole())?),
            "tags" => tags = at("tags", optional_strings(reader))?,
            "note" => {
                note = if reader.null() {
                    None
                } else {
                    Some(at("note", reader.text().map(owned))?)
                }
            }
            "item" => {
                item = if reader.null() {
                    None
                } else {
                    Some(at("item", strings(reader))?)
                }
            }
            "what" => what = Some(at("what", observation(reader))?),
            _ => {
                reader.any()?;
            }
        }
        Ok(())
    })?;
    Ok(Fact {
        unit: required(unit, "unit")?,
        file: required(file, "file")?,
        line: required(line, "line")?,
        column: required(column, "column")?,
        tags,
        note,
        item,
        what: required(what, "what")?,
    })
}

/// What a fact observed: a reference or a marker.
fn observation(reader: &mut Reader<'_>) -> Read<What> {
    let mut kind_of_fact: Option<String> = None;
    let mut target = None;
    let mut aliases = Vec::new();
    let mut kind = None;
    let mut certainty = None;
    let mut marker = None;
    let mut properties = Vec::new();
    reader.fields(|reader, key| {
        match key.as_ref() {
            "properties" => {
                if !reader.null() {
                    let each = |reader: &mut Reader<'_>, name: Cow<'_, str>| {
                        let value = at(&name, reader.text().map(owned))?;
                        properties.push(Property {
                            name: name.into_owned(),
                            value,
                        });
                        Ok(())
                    };
                    at("properties", reader.fields(each))?;
                }
            }
            "type" => kind_of_fact = Some(at("type", reader.text().map(owned))?),
            "target" => target = Some(at("target", strings(reader))?),
            "aliases" => aliases = at("aliases", optional_list(reader, strings))?,
            "kind" => kind = Some(at("kind", reader.text().map(owned))?),
            "certainty" => {
                let text = at("certainty", reader.text())?;
                certainty = Some(match text.as_ref() {
                    "exact" => Certainty::Exact,
                    "candidate" => Certainty::Candidate,
                    other => {
                        return Err(context("certainty", format!("unknown certainty `{other}`")))
                    }
                });
            }
            "marker" => marker = Some(at("marker", reader.text().map(owned))?),
            _ => {
                reader.any()?;
            }
        }
        Ok(())
    })?;
    match required(kind_of_fact, "type")?.as_str() {
        "reference" => Ok(What::Reference(Reference {
            target: required(target, "target")?,
            aliases,
            kind: required(kind, "kind")?,
            certainty: required(certainty, "certainty")?,
            properties,
        })),
        "marker" => Ok(What::Marker(required(marker, "marker")?)),
        other => Err(context("type", format!("unknown fact `{other}`"))),
    }
}

fn declaration(reader: &mut Reader<'_>) -> Read<Declaration> {
    let mut path = None;
    let mut name = None;
    let mut unit = None;
    let mut file = None;
    let mut line = None;
    let mut column = None;
    let mut end_line = None;
    let mut tags = Vec::new();
    let mut kind = None;
    let mut visibility = None;
    let mut relations = Vec::new();
    let mut modifiers = Vec::new();
    reader.fields(|reader, key| {
        match key.as_ref() {
            "path" => path = Some(at("path", strings(reader))?),
            "name" => name = Some(at("name", reader.text().map(owned))?),
            "unit" => unit = Some(at("unit", strings(reader))?),
            "file" => file = Some(at("file", reader.text().map(owned))?),
            "line" => line = Some(at("line", reader.whole())?),
            "column" => column = Some(at("column", reader.whole())?),
            "endLine" => {
                end_line = if reader.null() {
                    None
                } else {
                    Some(at("endLine", reader.whole())?)
                }
            }
            "tags" => tags = at("tags", optional_strings(reader))?,
            "kind" => kind = Some(at("kind", reader.text().map(owned))?),
            "visibility" => visibility = Some(at("visibility", reader.text().map(owned))?),
            "relations" => relations = at("relations", optional_list(reader, relation))?,
            "modifiers" => modifiers = at("modifiers", optional_strings(reader))?,
            _ => {
                reader.any()?;
            }
        }
        Ok(())
    })?;
    Ok(Declaration {
        path: required(path, "path")?,
        name: required(name, "name")?,
        unit: required(unit, "unit")?,
        file: required(file, "file")?,
        line: required(line, "line")?,
        column: required(column, "column")?,
        end_line,
        tags,
        kind: required(kind, "kind")?,
        visibility: required(visibility, "visibility")?,
        relations,
        modifiers,
    })
}

fn relation(reader: &mut Reader<'_>) -> Read<Relation> {
    let mut relation = None;
    let mut target = None;
    reader.fields(|reader, key| {
        match key.as_ref() {
            "relation" => relation = Some(at("relation", reader.text().map(owned))?),
            "target" => target = Some(at("target", strings(reader))?),
            _ => {
                reader.any()?;
            }
        }
        Ok(())
    })?;
    Ok(Relation {
        relation: required(relation, "relation")?,
        target: required(target, "target")?,
    })
}

#[cfg(test)]
mod tests {
    use super::super::json;
    use super::*;

    fn read(what: &str) -> Read<FactSet> {
        let text = format!(
            r#"{{"separator": "/", "units": [["a"]], "items": [], "kinds": ["dependency"],
            "markers": [], "tags": [], "facts": [{{"unit": ["a"], "file": "a", "line": 1,
            "column": 1, "what": {what}}}]}}"#
        );
        fact_set(&mut json::reader(&text))
    }

    /// The properties of a reference are read in their order, and a reference without them
    /// has none.
    #[test]
    fn a_reference_has_the_properties_of_the_text() {
        let reference = |what: &str| match read(what).unwrap().facts.remove(0).what {
            What::Reference(reference) => reference,
            What::Marker(_) => panic!("a marker"),
        };
        let plain =
            r#""type": "reference", "target": ["b"], "kind": "dependency", "certainty": "exact""#;
        let with = reference(&format!(
            r#"{{{plain}, "properties": {{"version": "1.2.0", "license": "MIT"}}}}"#
        ));
        let pairs = with.properties.iter();
        let pairs: Vec<(&str, &str)> = pairs
            .map(|one| (one.name.as_str(), one.value.as_str()))
            .collect();
        assert_eq!(pairs, [("version", "1.2.0"), ("license", "MIT")]);
        assert_eq!(with.property("license"), Some("MIT"));
        assert_eq!(with.property("source"), None);
        assert!(reference(&format!("{{{plain}}}")).properties.is_empty());
        let null = reference(&format!(r#"{{{plain}, "properties": null}}"#));
        assert!(null.properties.is_empty());
        let broken = read(&format!(r#"{{{plain}, "properties": {{"version": 1}}}}"#));
        assert_eq!(
            broken.unwrap_err(),
            "facts[0].what.properties.version: expected a string"
        );
    }
}
