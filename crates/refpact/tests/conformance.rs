//! Runs every case in `conformance/cases` against the core, directly and through its JSON.
//!
//! The JSON encoding of the cases is defined in `conformance/SPEC.md`, section 6, and the JSON
//! of the core in section 8.
//!
//! The last test keeps `conformance/CASES.md` equal to what the cases give: for each case
//! the rule, and the report that this core prints. To write the file again after a change
//! to a case or to a report text:
//!
//! ```sh
//! REFPACT_UPDATE_CASES=1 cargo test -p refpact --test conformance
//! ```

use std::path::PathBuf;

use refpact::core::wire::Session;
use refpact::core::{
    apply_baseline, evaluate, join, parse_baseline, render_errors, render_violations, split,
    Candidates, Certainty, Constraint, Declaration, Demand, Detail, DiagramFormat, DiagramLayer,
    DiagramNodes, End, Exemption, Fact, FactSet, Include, Items, KindFilter, PathFormat, Prepared,
    Property, RawPattern, Reference, Related, Relation, Rule, RuleError, Severity, StaleEntry,
    Step, Subjects, Targets, Violation, What,
};
use serde_json::{json, Value};

/// The number of cases the directory holds. A harness that runs fewer cases must fail.
const EXPECTED_CASES: usize = 293;

/// The case files in the order of their names: the file name and the content.
fn case_files() -> Vec<(String, Value)> {
    let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/cases");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .expect("conformance/cases exists")
        .map(|entry| entry.expect("readable entry").path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
        .collect();
    files.sort();
    files
        .iter()
        .map(|file| {
            let text = std::fs::read_to_string(file).expect("readable case file");
            let json: Value = serde_json::from_str(&text)
                .unwrap_or_else(|err| panic!("{}: invalid JSON: {err}", file.display()));
            (
                file.file_name()
                    .expect("file name")
                    .to_string_lossy()
                    .into_owned(),
                json,
            )
        })
        .collect()
}

#[test]
fn all_conformance_cases_pass() {
    let mut failures = Vec::new();
    let mut count = 0;
    for (file, json) in &case_files() {
        let separator = json["separator"].as_str().expect("separator").to_owned();
        let facts = fact_set(&json["facts"], &separator);
        for case in json["cases"].as_array().expect("cases") {
            count += 1;
            let name = case["name"].as_str().expect("case name");
            let rule = rule(&case["rule"], &separator);
            let mut result = evaluate(&rule, &facts);
            let mut stale = Vec::new();
            if let Some(diagram) = diagram(case, &rule, &facts, &separator) {
                if let Err(reason) = check_diagram(&case["expect"], &diagram, &separator) {
                    failures.push(format!(
                        "{file} / {name}\n    {reason}\n    result: {diagram:?}"
                    ));
                }
                continue;
            }
            let outcome = match case["baseline"].as_str().map(parse_baseline) {
                Some(Err(error)) => check_baseline_error(&case["expect"], error.line),
                baseline => {
                    if let (Some(Ok(entries)), Ok(violations)) = (baseline, &result) {
                        let applied = apply_baseline(
                            &rule.name,
                            violations.clone(),
                            &entries,
                            &PathFormat::plain(&separator),
                        );
                        stale = applied.stale;
                        result = Ok(applied.remaining);
                    }
                    check(&case["expect"], &result, &separator)
                        .and_then(|()| check_stale(&case["expect"]["stale"], &stale))
                }
            };
            if let Err(reason) = outcome {
                failures.push(format!(
                    "{file} / {name}\n    {reason}\n    result: {result:?}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "{} case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
    assert_eq!(
        count, EXPECTED_CASES,
        "the number of conformance cases changed; update EXPECTED_CASES"
    );
}

/// The core answers the same through its JSON (SPEC section 8) as through Rust: each case gives
/// the same report when its fact set and its rule cross the wire, as they do from TypeScript.
#[test]
fn every_case_gives_the_same_report_through_the_wire() {
    let mut failures = Vec::new();
    for (file, json) in &case_files() {
        let separator = json["separator"].as_str().expect("separator").to_owned();
        let facts = fact_set(&json["facts"], &separator);
        let mut session = Session::new();
        let loaded: Value = serde_json::from_str(
            &session
                .call(&json!({"op": "load-facts", "facts": wire_facts(&facts)}).to_string())
                .expect("the fact set loads"),
        )
        .expect("JSON");
        for case in json["cases"].as_array().expect("cases") {
            let rule = rule(&case["rule"], &separator);
            if let Some(direct) = diagram(case, &rule, &facts, &separator) {
                let mut request = json!({"op": "diagram", "handle": loaded["handle"],
                    "rule": wire_rule(&rule), "format": case["diagram"]});
                match diagram_layers(case, &separator) {
                    Some(layers) => {
                        request["layers"] = layers
                            .iter()
                            .map(|layer| {
                                json!({"name": layer.name, "units": wire_patterns(&layer.units),
                                    "mustNotUse": layer.must_not_use})
                            })
                            .collect();
                    }
                    None => request["slices"] = json!(diagram_slices(&rule)),
                }
                let answer: Value =
                    serde_json::from_str(&session.call(&request.to_string()).expect("diagram"))
                        .expect("JSON");
                let same = match &direct {
                    Ok(text) => answer["text"].as_str() == Some(text.as_str()),
                    Err(errors) => {
                        let request = json!({"op": "render-errors", "rule": rule.name,
                            "errors": answer["errors"]});
                        let rendered: Value = serde_json::from_str(
                            &session.call(&request.to_string()).expect("render"),
                        )
                        .expect("JSON");
                        rendered["text"].as_str()
                            == Some(render_errors(&rule.name, errors).as_str())
                    }
                };
                if !same {
                    failures.push(format!(
                        "{file} / {}:\n  wire:   {answer}\n  direct: {direct:?}",
                        case["name"]
                    ));
                }
                continue;
            }
            let request =
                json!({"op": "evaluate", "handle": loaded["handle"], "rule": wire_rule(&rule)});
            let answer: Value =
                serde_json::from_str(&session.call(&request.to_string()).expect("evaluate"))
                    .expect("JSON");
            let format = json!({"separator": separator});
            let through_wire = if answer["ok"] == json!(true) {
                let request = json!({"op": "render-violations", "rule": rule.name,
                    "reason": rule.reason, "severity": rule.severity.as_str(),
                    "violations": answer["violations"], "format": format});
                session.call(&request.to_string()).expect("render")
            } else {
                let request =
                    json!({"op": "render-errors", "rule": rule.name, "errors": answer["errors"]});
                session.call(&request.to_string()).expect("render")
            };
            let direct = match evaluate(&rule, &facts) {
                Ok(violations) => render_violations(
                    &rule.name,
                    rule.reason.as_deref(),
                    rule.severity,
                    &violations,
                    &PathFormat::plain(&separator),
                ),
                Err(errors) => render_errors(&rule.name, &errors),
            };
            let through_wire: Value = serde_json::from_str(&through_wire).expect("JSON");
            if through_wire["text"].as_str() != Some(direct.as_str()) {
                failures.push(format!(
                    "{file} / {}:\n  wire:   {}\n  direct: {direct}",
                    case["name"], through_wire["text"]
                ));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn wire_patterns(patterns: &[Vec<String>]) -> Value {
    json!(patterns)
}

fn wire_facts(facts: &FactSet) -> Value {
    let fact = |fact: &Fact| {
        let what = match &fact.what {
            What::Reference(reference) => json!({"type": "reference", "target": reference.target,
                "aliases": reference.aliases, "kind": reference.kind,
                "certainty": reference.certainty.as_str(),
                "properties": reference.properties.iter()
                    .map(|one| (one.name.clone(), json!(one.value)))
                    .collect::<serde_json::Map<String, Value>>()}),
            What::Marker(marker) => json!({"type": "marker", "marker": marker}),
        };
        json!({"unit": fact.unit, "file": fact.file, "line": fact.line, "column": fact.column,
            "tags": fact.tags, "note": fact.note, "item": fact.item, "what": what})
    };
    let declaration = |declaration: &Declaration| {
        json!({"path": declaration.path, "name": declaration.name, "unit": declaration.unit,
            "file": declaration.file, "line": declaration.line, "column": declaration.column,
            "endLine": declaration.end_line,
            "tags": declaration.tags, "kind": declaration.kind, "visibility": declaration.visibility,
            "relations": declaration.relations.iter().map(|tie| json!({"relation": tie.relation, "target": tie.target})).collect::<Vec<_>>(),
            "modifiers": declaration.modifiers})
    };
    json!({
        "separator": facts.separator, "units": facts.units, "unitLines": facts.unit_lines,
        "items": facts.items,
        "kinds": facts.kinds, "markers": facts.markers, "tags": facts.tags,
        "facts": facts.facts.iter().map(fact).collect::<Vec<_>>(),
        "declarations": facts.declarations.iter().map(declaration).collect::<Vec<_>>(),
        "itemKinds": facts.item_kinds, "visibilities": facts.visibilities,
        "relations": facts.relations, "modifiers": facts.modifiers,
        "properties": facts.properties,
    })
}

fn wire_rule(rule: &Rule) -> Value {
    let targets =
        |targets: &Targets| json!({"anyOf": targets.any_of, "unverified": targets.unverified});
    let related =
        |related: &Related| json!({"relation": related.relation, "anyOf": related.any_of});
    let constraint = match &rule.constraint {
        Constraint::MustNotUse(found) => json!({"type": "must-not-use", "targets": targets(found)}),
        Constraint::MustOnlyUse(found) => {
            json!({"type": "must-only-use", "targets": targets(found)})
        }
        Constraint::MustNotContain(markers) => {
            json!({"type": "must-not-contain", "markers": markers})
        }
        Constraint::MustBeAcyclic(slices) => json!({"type": "must-be-acyclic", "slices": slices}),
        Constraint::MustBeIndependent(slices) => {
            json!({"type": "must-be-independent", "slices": wire_patterns(slices)})
        }
        Constraint::MustBeUsed => json!({"type": "must-be-used"}),
        Constraint::MustNotReach(found) => {
            json!({"type": "must-not-reach", "targets": targets(found)})
        }
        Constraint::MustBeReachableFrom(roots) => {
            json!({"type": "must-be-reachable-from", "roots": roots})
        }
        Constraint::MustBeIn(places) => json!({"type": "must-be-in", "places": places}),
        Constraint::MustOnlyBeUsedBy(users) => {
            json!({"type": "must-only-be-used-by", "users": users})
        }
        Constraint::MustNotExceedLines(lines) => {
            json!({"type": "must-not-exceed-lines", "lines": lines})
        }
        Constraint::Items(items) => {
            let must = match &items.must {
                Demand::NotExist => json!({"type": "not-exist"}),
                Demand::ResideIn(places) => json!({"type": "reside-in", "places": places}),
                Demand::BeNamed(names) => json!({"type": "be-named", "names": names}),
                Demand::Have(found) => {
                    json!({"type": "have", "relation": found.relation, "anyOf": found.any_of})
                }
                Demand::HaveModifier(modifiers) => {
                    json!({"type": "have-modifier", "modifiers": modifiers})
                }
                Demand::NotUse(found) => json!({"type": "not-use", "targets": targets(found)}),
                Demand::OnlyUse(found) => json!({"type": "only-use", "targets": targets(found)}),
                Demand::NotExceedLines(lines) => {
                    json!({"type": "not-exceed-lines", "lines": lines})
                }
                Demand::Custom(_) => unreachable!("a case is plain data"),
            };
            json!({"type": "items", "named": items.named, "kinds": items.kinds,
                "visibilities": items.visibilities, "modifiers": items.modifiers,
                "related": items.related.iter().map(related).collect::<Vec<_>>(), "must": must})
        }
        Constraint::Custom(_) => unreachable!("a case is plain data"),
    };
    let include = match &rule.subjects.include {
        Include::All => json!({"type": "all"}),
        Include::AnyOf(patterns) => json!({"type": "any-of", "patterns": patterns}),
    };
    let kinds = match &rule.kinds {
        KindFilter::All => json!({"type": "all"}),
        KindFilter::Only(kinds) => json!({"type": "only", "kinds": kinds}),
        KindFilter::Except(kinds) => json!({"type": "except", "kinds": kinds}),
    };
    json!({
        "name": rule.name,
        "reason": rule.reason,
        "severity": rule.severity.as_str(),
        "subjects": {"include": include, "except": rule.subjects.except},
        "constraint": constraint,
        "kinds": kinds,
        "ignoreTags": rule.ignore_tags,
        "properties": rule.properties,
        "candidates": match rule.candidates { Candidates::Include => "include", Candidates::Ignore => "ignore" },
        "exemptions": rule.exemptions.iter().map(|exemption| json!({"from": exemption.from, "to": exemption.to})).collect::<Vec<_>>(),
    })
}

fn path(value: &Value, separator: &str) -> Vec<String> {
    match value {
        Value::String(text) => split(text, separator),
        Value::Array(segments) => segments
            .iter()
            .map(|segment| segment.as_str().expect("segment").to_owned())
            .collect(),
        other => panic!("a path is a string or an array, found {other}"),
    }
}

fn paths(value: &Value, separator: &str) -> Vec<Vec<String>> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(list) => list.iter().map(|entry| path(entry, separator)).collect(),
        other => panic!("expected a list of paths, found {other}"),
    }
}

fn properties(value: &Value) -> Vec<Property> {
    match value {
        Value::Null => Vec::new(),
        Value::Object(pairs) => pairs
            .iter()
            .map(|(name, value)| Property {
                name: name.clone(),
                value: value.as_str().expect("string").to_owned(),
            })
            .collect(),
        other => panic!("expected an object of properties, found {other}"),
    }
}

fn strings(value: &Value) -> Vec<String> {
    match value {
        Value::Null => Vec::new(),
        Value::Array(list) => list
            .iter()
            .map(|entry| entry.as_str().expect("string").to_owned())
            .collect(),
        other => panic!("expected a list of strings, found {other}"),
    }
}

fn fact_set(json: &Value, separator: &str) -> FactSet {
    let facts = json["facts"]
        .as_array()
        .expect("facts")
        .iter()
        .map(|fact| Fact {
            unit: path(&fact["unit"], separator),
            file: fact["file"].as_str().expect("file").to_owned(),
            line: fact["line"].as_u64().expect("line") as u32,
            column: fact["column"].as_u64().unwrap_or(1) as u32,
            tags: strings(&fact["tags"]),
            note: fact["note"].as_str().map(str::to_owned),
            item: (!fact["item"].is_null()).then(|| path(&fact["item"], separator)),
            what: match (&fact["reference"], &fact["marker"]) {
                (Value::Object(_), Value::Null) => {
                    let reference = &fact["reference"];
                    What::Reference(Reference {
                        target: path(&reference["target"], separator),
                        aliases: paths(&reference["aliases"], separator),
                        kind: reference["kind"].as_str().expect("kind").to_owned(),
                        certainty: match reference["certainty"].as_str() {
                            None | Some("exact") => Certainty::Exact,
                            Some("candidate") => Certainty::Candidate,
                            Some(other) => panic!("unknown certainty {other}"),
                        },
                        properties: properties(&reference["properties"]),
                    })
                }
                (Value::Null, Value::String(marker)) => What::Marker(marker.clone()),
                _ => panic!("a fact has exactly one of `reference` and `marker`"),
            },
        })
        .collect();
    FactSet {
        separator: separator.to_owned(),
        attached_prefixes: Vec::new(),
        hidden_prefixes: Vec::new(),
        units: paths(&json["units"], separator),
        unit_lines: match &json["unit_lines"] {
            Value::Null => Vec::new(),
            list => list
                .as_array()
                .expect("unit_lines")
                .iter()
                .map(|lines| lines.as_u64().expect("a number of lines") as u32)
                .collect(),
        },
        items: paths(&json["items"], separator),
        kinds: strings(&json["kinds"]),
        markers: strings(&json["markers"]),
        tags: strings(&json["tags"]),
        facts,
        declarations: match &json["declarations"] {
            Value::Null => Vec::new(),
            list => list
                .as_array()
                .expect("declarations")
                .iter()
                .map(|declaration| Declaration {
                    path: path(&declaration["path"], separator),
                    name: declaration["name"].as_str().expect("name").to_owned(),
                    unit: path(&declaration["unit"], separator),
                    file: declaration["file"].as_str().expect("file").to_owned(),
                    line: declaration["line"].as_u64().expect("line") as u32,
                    column: declaration["column"].as_u64().unwrap_or(1) as u32,
                    end_line: declaration["end_line"].as_u64().map(|line| line as u32),
                    tags: strings(&declaration["tags"]),
                    kind: declaration["kind"].as_str().expect("kind").to_owned(),
                    visibility: declaration["visibility"]
                        .as_str()
                        .expect("visibility")
                        .to_owned(),
                    relations: match &declaration["relations"] {
                        Value::Null => Vec::new(),
                        list => list
                            .as_array()
                            .expect("relations")
                            .iter()
                            .map(|tie| Relation {
                                relation: tie["relation"].as_str().expect("relation").to_owned(),
                                target: path(&tie["target"], separator),
                            })
                            .collect(),
                    },
                    modifiers: strings(&declaration["modifiers"]),
                })
                .collect(),
        },
        item_kinds: strings(&json["item_kinds"]),
        visibilities: strings(&json["visibilities"]),
        relations: strings(&json["relations"]),
        modifiers: strings(&json["modifiers"]),
        properties: strings(&json["properties"]),
    }
}

/// Globs: a string as it is, or an array of segments joined.
fn globs(value: &Value) -> Vec<String> {
    value
        .as_array()
        .expect("a list of globs")
        .iter()
        .map(|entry| match entry {
            Value::String(text) => text.clone(),
            Value::Array(segments) => segments
                .iter()
                .map(|segment| segment.as_str().expect("segment"))
                .collect(),
            other => panic!("a glob is a string or an array, found {other}"),
        })
        .collect()
}

fn related(json: &Value, separator: &str) -> Related {
    Related {
        relation: json["relation"].as_str().expect("relation").to_owned(),
        any_of: paths(&json["any_of"], separator),
    }
}

fn targets(json: &Value, separator: &str) -> Targets {
    Targets {
        any_of: paths(&json["any_of"], separator),
        unverified: paths(&json["unverified"], separator),
    }
}

fn items(json: &Value, separator: &str) -> Items {
    let optional = |key: &str| (!json[key].is_null()).then(|| strings(&json[key]));
    let must = &json["must"];
    Items {
        named: (!json["named"].is_null()).then(|| globs(&json["named"])),
        kinds: optional("kinds"),
        visibilities: optional("visibilities"),
        modifiers: optional("modifiers"),
        related: match &json["related"] {
            Value::Null => Vec::new(),
            list => list
                .as_array()
                .expect("related is a list")
                .iter()
                .map(|entry| related(entry, separator))
                .collect(),
        },
        must: if must.as_str() == Some("not_exist") {
            Demand::NotExist
        } else if !must["reside_in"].is_null() {
            Demand::ResideIn(paths(&must["reside_in"], separator))
        } else if !must["be_named"].is_null() {
            Demand::BeNamed(globs(&must["be_named"]))
        } else if !must["not_use"].is_null() {
            Demand::NotUse(targets(&must["not_use"], separator))
        } else if !must["only_use"].is_null() {
            Demand::OnlyUse(targets(&must["only_use"], separator))
        } else if !must["have_modifier"].is_null() {
            Demand::HaveModifier(strings(&must["have_modifier"]))
        } else if !must["not_exceed_lines"].is_null() {
            Demand::NotExceedLines(must["not_exceed_lines"].as_u64().expect("lines") as u32)
        } else {
            Demand::Have(related(&must["have"], separator))
        },
    }
}

fn rule(json: &Value, separator: &str) -> Rule {
    let subjects = &json["subjects"];
    let include = match (&subjects["all"], &subjects["any_of"]) {
        (Value::Bool(true), Value::Null) => Include::All,
        (Value::Null, list @ Value::Array(_)) => Include::AnyOf(paths(list, separator)),
        _ => panic!("subjects have exactly one of `all: true` and `any_of`"),
    };
    let except = match &subjects["except"] {
        Value::Null => None,
        list => Some(paths(list, separator)),
    };
    let targets = |value: &Value| targets(value, separator);
    const KEYS: [&str; 12] = [
        "must_not_use",
        "must_only_use",
        "must_not_contain",
        "must_be_acyclic",
        "must_be_independent",
        "must_be_used",
        "must_not_reach",
        "must_be_reachable_from",
        "must_be_in",
        "must_only_be_used_by",
        "must_not_exceed_lines",
        "items",
    ];
    let given: Vec<&str> = KEYS
        .into_iter()
        .filter(|key| !json[*key].is_null())
        .collect();
    let [key] = given[..] else {
        panic!("a rule has exactly one constraint")
    };
    let value = &json[key];
    let list = |key: &str| match &value[key] {
        Value::Null => None,
        list => Some(paths(list, separator)),
    };
    let constraint = match key {
        "must_not_use" => Constraint::MustNotUse(targets(value)),
        "must_only_use" => Constraint::MustOnlyUse(targets(value)),
        "must_not_contain" => Constraint::MustNotContain(strings(value)),
        "must_be_acyclic" => Constraint::MustBeAcyclic(list("slices")),
        "must_be_independent" => Constraint::MustBeIndependent(list("slices").expect("slices")),
        "must_be_used" => Constraint::MustBeUsed,
        "must_not_reach" => Constraint::MustNotReach(targets(value)),
        "must_be_reachable_from" => Constraint::MustBeReachableFrom(list("roots").expect("roots")),
        "must_be_in" => Constraint::MustBeIn(list("places").expect("places")),
        "must_only_be_used_by" => Constraint::MustOnlyBeUsedBy(list("users").expect("users")),
        "must_not_exceed_lines" => {
            Constraint::MustNotExceedLines(value["lines"].as_u64().expect("lines") as u32)
        }
        _ => Constraint::Items(items(value, separator)),
    };
    let kinds = match &json["kinds"] {
        Value::Null => KindFilter::All,
        filter => match (&filter["only"], &filter["except"]) {
            (only @ Value::Array(_), Value::Null) => KindFilter::Only(strings(only)),
            (Value::Null, except @ Value::Array(_)) => KindFilter::Except(strings(except)),
            _ => panic!("`kinds` has exactly one of `only` and `except`"),
        },
    };
    Rule {
        name: json["name"].as_str().expect("rule name").to_owned(),
        reason: json["reason"].as_str().map(str::to_owned),
        severity: match json["severity"].as_str() {
            None | Some("error") => Severity::Error,
            Some("warning") => Severity::Warning,
            Some(other) => panic!("unknown severity {other}"),
        },
        subjects: Subjects { include, except },
        constraint,
        kinds,
        ignore_tags: strings(&json["ignore_tags"]),
        properties: strings(&json["properties"]),
        candidates: match json["candidates"].as_str() {
            None | Some("include") => Candidates::Include,
            Some("ignore") => Candidates::Ignore,
            Some(other) => panic!("unknown candidates setting {other}"),
        },
        exemptions: match &json["exemptions"] {
            Value::Null => Vec::new(),
            list => list
                .as_array()
                .expect("exemptions is a list")
                .iter()
                .map(|exemption| Exemption {
                    from: paths(&exemption["from"], separator),
                    to: paths(&exemption["to"], separator),
                })
                .collect(),
        },
    }
}

fn check(
    expect: &Value,
    result: &Result<Vec<Violation>, Vec<RuleError>>,
    separator: &str,
) -> Result<(), String> {
    match (&expect["violations"], &expect["errors"], result) {
        (Value::Array(expected), Value::Null, Ok(violations)) => {
            if expected.len() != violations.len() {
                return Err(format!(
                    "expected {} violation(s), found {}",
                    expected.len(),
                    violations.len()
                ));
            }
            for (index, (expected, violation)) in expected.iter().zip(violations).enumerate() {
                check_violation(expected, violation, separator)
                    .map_err(|reason| format!("violation {index}: {reason}"))?;
            }
            Ok(())
        }
        (Value::Null, Value::Array(expected), Err(errors)) => {
            let expected: Vec<(String, Option<String>, Option<String>)> = expected
                .iter()
                .map(|error| {
                    (
                        error["code"].as_str().expect("code").to_owned(),
                        error["role"].as_str().map(str::to_owned),
                        error["value"].as_str().map(str::to_owned),
                    )
                })
                .collect();
            let found: Vec<(String, Option<String>, Option<String>)> = errors
                .iter()
                .map(|error| {
                    (
                        error.code.as_str().to_owned(),
                        error.role.map(|role| role.as_str().to_owned()),
                        error.value.clone(),
                    )
                })
                .collect();
            if expected == found {
                Ok(())
            } else {
                Err(format!("expected errors {expected:?}, found {found:?}"))
            }
        }
        (Value::Array(_), Value::Null, Err(_)) => Err("expected violations, found errors".into()),
        (Value::Null, Value::Array(_), Ok(_)) => Err("expected errors, found violations".into()),
        (Value::Null, Value::Null, _) if !expect["baseline_error"].is_null() => {
            Err("expected a baseline that cannot be read, but it was read".into())
        }
        _ => panic!("an expectation has exactly one of `violations` and `errors`"),
    }
}

fn check_baseline_error(expect: &Value, line: usize) -> Result<(), String> {
    match expect["baseline_error"].as_u64() {
        Some(expected) if expected == line as u64 => Ok(()),
        Some(expected) => Err(format!(
            "expected a baseline error in line {expected}, found line {line}"
        )),
        None => Err(format!(
            "line {line} of the baseline is invalid, which the case does not expect"
        )),
    }
}

fn check_stale(expect: &Value, stale: &[StaleEntry]) -> Result<(), String> {
    let expected: Vec<StaleEntry> = match expect {
        Value::Null => Vec::new(),
        list => list
            .as_array()
            .expect("stale is a list")
            .iter()
            .map(|entry| StaleEntry {
                unit: entry["unit"].as_str().expect("unit").to_owned(),
                what: entry["what"].as_str().expect("what").to_owned(),
                count: entry["count"].as_u64().expect("count") as u32,
                found: entry["found"].as_u64().expect("found") as u32,
            })
            .collect(),
    };
    if expected == stale {
        Ok(())
    } else {
        Err(format!(
            "expected stale entries {expected:?}, found {stale:?}"
        ))
    }
}

fn check_step(expected: &Value, step: &Step, separator: &str) -> Result<(), String> {
    for (key, value) in expected.as_object().expect("step object") {
        let equal = match key.as_str() {
            "from" => path(value, separator) == step.from,
            "to" => path(value, separator) == step.to,
            "file" => value.as_str() == Some(step.file.as_str()),
            "line" => value.as_u64() == Some(u64::from(step.line)),
            "column" => value.as_u64() == Some(u64::from(step.column)),
            "target" => path(value, separator) == step.target,
            "kind" => value.as_str() == Some(step.kind.as_str()),
            other => panic!("unknown key `{other}` in an expected step"),
        };
        if !equal {
            return Err(format!(
                "`{key}` of a step differs: expected {value}, found {step:?}"
            ));
        }
    }
    Ok(())
}

fn check_end(expected: &Value, end: &End, separator: &str) -> Result<(), String> {
    for (key, value) in expected.as_object().expect("end object") {
        let equal = match key.as_str() {
            "unit" => path(value, separator) == end.unit,
            "file" => value.as_str() == Some(end.file.as_str()),
            "line" => value.as_u64() == Some(u64::from(end.line)),
            "column" => value.as_u64() == Some(u64::from(end.column)),
            "target" => path(value, separator) == end.target,
            "kind" => value.as_str() == Some(end.kind.as_str()),
            other => panic!("unknown key `{other}` in an expected end"),
        };
        if !equal {
            return Err(format!(
                "`{key}` of the end differs: expected {value}, found {end:?}"
            ));
        }
    }
    Ok(())
}

fn check_steps(expected: &Value, steps: Option<&Vec<Step>>, separator: &str) -> Result<(), String> {
    let expected = expected.as_array().expect("a list of steps");
    let steps = steps.ok_or("the violation has no steps of this form")?;
    if expected.len() != steps.len() {
        return Err(format!(
            "expected {} step(s), found {steps:?}",
            expected.len()
        ));
    }
    for (expected, step) in expected.iter().zip(steps) {
        check_step(expected, step, separator)?;
    }
    Ok(())
}

fn check_violation(expected: &Value, violation: &Violation, separator: &str) -> Result<(), String> {
    let (target, written, kind, certainty, marker) = match &violation.detail {
        Detail::Uses {
            target,
            written,
            kind,
            certainty,
            ..
        } => (
            Some(target),
            written.as_ref(),
            Some(kind),
            Some(certainty.as_str()),
            None,
        ),
        Detail::Contains { marker } => (None, None, None, None, Some(marker)),
        _ => (None, None, None, None, None),
    };
    let (lines, limit) = match &violation.detail {
        Detail::TooLong { lines, limit, .. } => (Some(*lines), Some(*limit)),
        _ => (None, None),
    };
    let (item, item_kind) = match &violation.detail {
        Detail::Item { item, kind } => (Some(item), Some(kind)),
        Detail::TooLong { item, kind, .. } => (item.as_ref(), kind.as_ref()),
        Detail::Uses {
            item: Some(item), ..
        } => (Some(item), None),
        _ => (None, None),
    };
    let (cycle, size) = match &violation.detail {
        Detail::Cycle { steps, size } => (Some(steps), Some(*size)),
        _ => (None, None),
    };
    let (chain, end) = match &violation.detail {
        Detail::Reaches { steps, end } => (Some(steps), Some(end)),
        _ => (None, None),
    };
    let form = match &violation.detail {
        Detail::Uses { .. } => "uses",
        Detail::Contains { .. } => "contains",
        Detail::Cycle { .. } => "cycle",
        Detail::Unused => "unused",
        Detail::Reaches { .. } => "reaches",
        Detail::Unreachable => "unreachable",
        Detail::Outside => "outside",
        Detail::Item { .. } => "item",
        Detail::TooLong { .. } => "too-long",
        Detail::Custom { .. } => "custom",
    };
    for (key, value) in expected.as_object().expect("violation object") {
        let equal = match key.as_str() {
            "unit" => path(value, separator) == violation.unit,
            "file" => value.as_str() == Some(violation.file.as_str()),
            "line" => value.as_u64() == Some(u64::from(violation.line)),
            "column" => value.as_u64() == Some(u64::from(violation.column)),
            "note" => value.as_str() == violation.note.as_deref(),
            "target" => Some(&path(value, separator)) == target,
            "written" => match value {
                Value::Null => written.is_none(),
                value => Some(&path(value, separator)) == written,
            },
            "kind" => value.as_str() == kind.map(String::as_str),
            "certainty" => value.as_str() == certainty,
            "marker" => value.as_str() == marker.map(String::as_str),
            "size" => value.as_u64() == size.map(|size| size as u64),
            "lines" => value.as_u64() == lines.map(u64::from),
            "limit" => value.as_u64() == limit.map(u64::from),
            "detail" => value.as_str() == Some(form),
            "item" => Some(&path(value, separator)) == item,
            "item_kind" => value.as_str() == item_kind.map(String::as_str),
            "cycle" => check_steps(value, cycle, separator).map(|()| true)?,
            "chain" => check_steps(value, chain, separator).map(|()| true)?,
            "end" => check_end(value, end.ok_or("expected an end")?, separator).map(|()| true)?,
            other => panic!("unknown key `{other}` in an expected violation"),
        };
        if !equal {
            return Err(format!(
                "`{key}` differs: expected {value}, found {}",
                refpact::core::render_violation(violation, &PathFormat::plain(separator))
            ));
        }
    }
    Ok(())
}

/// One cell of a markdown table.
fn cell(text: &str) -> String {
    if text.is_empty() {
        String::new()
    } else {
        format!("`{}`", text.replace('|', "\\|"))
    }
}

/// The facts of a case file as a markdown table.
fn facts_table(facts: &FactSet, separator: &str) -> String {
    let mut text = String::from("| # | unit | place | fact | tags |\n|---|---|---|---|---|\n");
    for (index, fact) in facts.facts.iter().enumerate() {
        let mut what = match &fact.what {
            What::Marker(marker) => format!("marker {}", cell(marker)),
            What::Reference(reference) => {
                let mut what = format!(
                    "{} {}",
                    reference.kind,
                    cell(&join(&reference.target, separator))
                );
                for alias in &reference.aliases {
                    what.push_str(&format!(", also {}", cell(&join(alias, separator))));
                }
                if reference.certainty == Certainty::Candidate {
                    what.push_str(" (candidate)");
                }
                for one in &reference.properties {
                    what.push_str(&format!(", {} {}", one.name, cell(&one.value)));
                }
                what
            }
        };
        if let Some(item) = &fact.item {
            what.push_str(&format!(", in {}", cell(&join(item, separator))));
        }
        text.push_str(&format!(
            "| {} | {} | {} | {what} | {} |\n",
            index + 1,
            cell(&join(&fact.unit, separator)),
            cell(&format!("{}:{}:{}", fact.file, fact.line, fact.column)),
            fact.tags
                .iter()
                .map(|tag| cell(tag))
                .collect::<Vec<_>>()
                .join(", "),
        ));
    }
    text
}

/// The declarations of a case file as a markdown table, when it has some.
/// The lines of the units of a case file as a markdown table.
fn lines_table(facts: &FactSet, separator: &str) -> String {
    if facts.unit_lines.is_empty() {
        return String::new();
    }
    let mut text = String::from("\n| unit | lines |\n|---|---|\n");
    for (unit, lines) in facts.units.iter().zip(&facts.unit_lines) {
        text.push_str(&format!("| {} | {lines} |\n", cell(&join(unit, separator))));
    }
    text
}

fn declarations_table(facts: &FactSet, separator: &str) -> String {
    if facts.declarations.is_empty() {
        return String::new();
    }
    let mut text = String::from(
        "\n| # | item | unit | place | end line | kind | visibility | relations | modifiers | tags |\n\
         |---|---|---|---|---|---|---|---|---|---|\n",
    );
    for (index, declaration) in facts.declarations.iter().enumerate() {
        let relations: Vec<String> = declaration
            .relations
            .iter()
            .map(|tie| format!("{} {}", tie.relation, cell(&join(&tie.target, separator))))
            .collect();
        let tags: Vec<String> = declaration.tags.iter().map(|tag| cell(tag)).collect();
        let modifiers: Vec<String> = declaration
            .modifiers
            .iter()
            .map(|name| cell(name))
            .collect();
        text.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | {} | {} | {} |\n",
            index + 1,
            cell(&join(&declaration.path, separator)),
            cell(&join(&declaration.unit, separator)),
            cell(&format!(
                "{}:{}:{}",
                declaration.file, declaration.line, declaration.column
            )),
            declaration
                .end_line
                .map_or_else(String::new, |line| line.to_string()),
            cell(&declaration.kind),
            cell(&declaration.visibility),
            relations.join(", "),
            modifiers.join(", "),
            tags.join(", "),
        ));
    }
    text
}

/// A text as a fenced block. The text of a report has no fence of its own.
fn block(language: &str, text: &str) -> String {
    format!("```{language}\n{}\n```\n", text.trim_end_matches('\n'))
}

/// What one case gives: the report of the rule, after the baseline when the case has one.
fn outcome(case: &Value, facts: &FactSet, separator: &str) -> String {
    let rule = rule(&case["rule"], separator);
    match diagram(case, &rule, facts, separator) {
        Some(Ok(text)) => return text,
        Some(Err(errors)) => return render_errors(&rule.name, &errors),
        None => {}
    }
    let violations = match evaluate(&rule, facts) {
        Ok(violations) => violations,
        Err(errors) => return render_errors(&rule.name, &errors),
    };
    let (remaining, stale) = match case["baseline"].as_str().map(parse_baseline) {
        None => (violations, Vec::new()),
        Some(Err(error)) => return format!("The baseline is invalid at line {}.", error.line),
        Some(Ok(entries)) => {
            let applied = apply_baseline(
                &rule.name,
                violations,
                &entries,
                &PathFormat::plain(separator),
            );
            (applied.remaining, applied.stale)
        }
    };
    let mut text = if remaining.is_empty() {
        String::from("No violation.\n")
    } else {
        render_violations(
            &rule.name,
            rule.reason.as_deref(),
            rule.severity,
            &remaining,
            &PathFormat::plain(separator),
        )
    };
    for entry in &stale {
        text.push_str(&format!(
            "\nStale baseline entry: {} / {} accepts {}, the code has {}.",
            entry.unit, entry.what, entry.count, entry.found
        ));
    }
    text
}

/// The slices that the diagram of a rule draws: those of `must_be_acyclic` or
/// `must_be_independent` (SPEC 4.17).
fn diagram_slices(rule: &Rule) -> Option<&[RawPattern]> {
    match &rule.constraint {
        Constraint::MustBeAcyclic(slices) => slices.as_deref(),
        Constraint::MustBeIndependent(slices) => Some(slices.as_slice()),
        _ => None,
    }
}

/// The layers of a case that asks for a diagram of layers, if it has `"layers"`.
fn diagram_layers(case: &Value, separator: &str) -> Option<Vec<DiagramLayer>> {
    let layers = case["layers"].as_array()?;
    Some(
        layers
            .iter()
            .map(|layer| DiagramLayer {
                name: layer["name"].as_str().expect("layer name").to_owned(),
                units: paths(&layer["units"], separator),
                must_not_use: layer["must_not_use"]
                    .as_array()
                    .map(|list| {
                        list.iter()
                            .map(|to| to.as_u64().expect("layer number") as usize)
                            .collect()
                    })
                    .unwrap_or_default(),
            })
            .collect(),
    )
}

/// The diagram that a case asks for with `"diagram": "mermaid"` or `"dot"`, if it asks.
fn diagram(
    case: &Value,
    rule: &Rule,
    facts: &FactSet,
    separator: &str,
) -> Option<Result<String, Vec<RuleError>>> {
    let format = match case["diagram"].as_str()? {
        "mermaid" => DiagramFormat::Mermaid,
        "dot" => DiagramFormat::Dot,
        other => panic!("unknown diagram format {other}"),
    };
    let layers = diagram_layers(case, separator);
    let nodes = match (&layers, diagram_slices(rule)) {
        (Some(layers), _) => DiagramNodes::Layers(layers),
        (None, Some(slices)) => DiagramNodes::Slices(slices),
        (None, None) => DiagramNodes::Subjects,
    };
    Some(Prepared::new(facts.clone()).diagram(rule, nodes, format))
}

/// Whether the outcome of a case that asks for a diagram is the diagram, not errors.
fn outcome_is_diagram(case: &Value, facts: &FactSet, separator: &str) -> bool {
    matches!(
        diagram(case, &rule(&case["rule"], separator), facts, separator),
        Some(Ok(_))
    )
}

fn check_diagram(
    expect: &Value,
    diagram: &Result<String, Vec<RuleError>>,
    separator: &str,
) -> Result<(), String> {
    match (&expect["diagram"], diagram) {
        (Value::String(expected), Ok(text)) if expected == text => Ok(()),
        (Value::String(expected), Ok(text)) => {
            Err(format!("expected the diagram\n{expected}\nfound\n{text}"))
        }
        (Value::Null, Err(errors)) => check(expect, &Err(errors.clone()), separator),
        _ => Err("expected a diagram, found errors, or the other way".to_owned()),
    }
}

/// The content of `conformance/CASES.md`.
fn case_reference() -> String {
    let mut text = String::from(
        "# Conformance cases\n\n\
         This file shows what each case in `cases/` gives: the facts of each file, and for each\n\
         case the rule and the report that the Rust core prints. The conformance test of the\n\
         `refpact` crate makes this file and fails when it is not up to date. Do not edit it.\n\n\
         `SPEC.md` defines the behaviour. The rule is shown in the JSON encoding of `SPEC.md`,\n\
         section 6.\n",
    );
    for (file, json) in &case_files() {
        let separator = json["separator"].as_str().expect("separator");
        let facts = fact_set(&json["facts"], separator);
        text.push_str(&format!(
            "\n## {file}\n\n{}\n\n",
            json["description"].as_str().expect("description")
        ));
        text.push_str(&format!(
            "Paths are printed with the separator `{separator}`.\n\n"
        ));
        text.push_str(&facts_table(&facts, separator));
        text.push_str(&lines_table(&facts, separator));
        text.push_str(&declarations_table(&facts, separator));
        for case in json["cases"].as_array().expect("cases") {
            text.push_str(&format!(
                "\n### {}\n\n",
                case["name"].as_str().expect("case name")
            ));
            text.push_str(&block("json", &case["rule"].to_string()));
            if let Some(format) = case["diagram"].as_str() {
                text.push_str(&format!("\nThe case asks for the diagram in `{format}`.\n"));
                if let Some(layers) = case.get("layers") {
                    text.push_str(&format!(
                        "\nThe nodes are these layers:\n\n{}",
                        block("json", &layers.to_string())
                    ));
                }
            }
            if let Some(baseline) = case["baseline"].as_str() {
                text.push_str(&format!("\nBaseline:\n\n{}", block("text", baseline)));
            }
            let language = match case["diagram"].as_str() {
                Some(format) if outcome_is_diagram(case, &facts, separator) => format,
                _ => "text",
            };
            text.push_str(&format!(
                "\nResult:\n\n{}",
                block(language, &outcome(case, &facts, separator))
            ));
        }
    }
    text
}

#[test]
fn the_case_reference_is_what_the_cases_give() {
    let file = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../conformance/CASES.md");
    let expected = case_reference();
    if std::env::var_os("REFPACT_UPDATE_CASES").is_some() {
        std::fs::write(&file, &expected).expect("conformance/CASES.md is written");
    }
    let found = std::fs::read_to_string(&file).unwrap_or_default();
    assert!(
        found == expected,
        "conformance/CASES.md is not what the cases give. Write it again with\n\
         REFPACT_UPDATE_CASES=1 cargo test -p refpact --test conformance"
    );
}
