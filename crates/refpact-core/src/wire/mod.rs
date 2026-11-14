//! The core as JSON, for bindings in other languages (SPEC section 8).
//!
//! A binding keeps one [`Session`] and sends it requests as JSON text. A fact set is sent
//! once and gets a handle, and each rule names the handle, so a large fact set crosses the
//! boundary once and not for each rule.
//!
//! ```
//! use refpact_core::wire::Session;
//!
//! let mut session = Session::new();
//! let loaded = session
//!     .call(r#"{"op": "load-facts", "facts": {"separator": "::", "units": [["a"], ["b"]],
//!         "items": [], "kinds": ["use"], "markers": [], "tags": [], "facts": [
//!         {"unit": ["a"], "file": "a.rs", "line": 1, "column": 1, "tags": [],
//!          "what": {"type": "reference", "target": ["b"], "aliases": [], "kind": "use",
//!                   "certainty": "exact"}}]}}"#)
//!     .unwrap();
//! assert_eq!(loaded, r#"{"handle":1}"#);
//! let result = session
//!     .call(r#"{"op": "evaluate", "handle": 1, "rule": {"name": "a does not use b",
//!         "subjects": {"include": {"type": "any-of", "patterns": [["a"]]}},
//!         "constraint": {"type": "must-not-use", "targets": {"anyOf": [["b"]], "unverified": []}}}}"#)
//!     .unwrap();
//! assert!(result.starts_with(r#"{"ok":true,"violations":[{"rule":"a does not use b""#));
//! ```

mod facts;
mod json;
mod select;

use std::borrow::Cow;

use json::Value;

use crate::baseline::{
    apply_baseline, baseline_entries, baseline_key, format_baseline, parse_baseline, BaselineEntry,
    StaleEntry,
};
use crate::diagram::{DiagramFormat, DiagramLayer, DiagramNodes};
use crate::error::{ErrorCode, Role, RuleError};
use crate::eval::Prepared;
use crate::model::{Candidates, Certainty, KindFilter, RawPattern, Targets};
use crate::pattern::Path;
use crate::report::{render_error, render_errors, render_violation, render_violations, PathFormat};
use crate::rule::{
    Constraint, Demand, Exemption, Include, Items, Related, Rule, Severity, Subjects,
};
use crate::violation::{Detail, End, Step, Violation};

/// The version of the wire format. A binding checks it with the request `version`, so that a
/// stale build of the core fails at once and not with odd results.
pub const VERSION: u32 = 1;

/// The state of one binding: the fact sets that it loaded.
#[derive(Debug, Default)]
pub struct Session {
    /// The fact set of handle `n` is at `n - 1`. A dropped fact set leaves `None`.
    facts: Vec<Option<Prepared>>,
}

impl Session {
    /// A session with no fact sets.
    pub fn new() -> Session {
        Session::default()
    }

    /// Answers one request. An `Err` is a request that is not valid: unknown, malformed, or
    /// naming a handle that does not exist. Errors in a rule are an `Ok` answer of
    /// `evaluate`, as the specification has them.
    pub fn call(&mut self, request: &str) -> Result<String, String> {
        // The fact set of `load-facts` is read straight from the text, and the rest of the
        // request as a tree.
        let mut reader = json::reader(request);
        let mut fields = Vec::new();
        let mut facts = None;
        reader.fields(|reader, key| {
            if key == "facts" {
                facts = Some(facts::fact_set(reader).map_err(|error| context("facts", error))?);
            } else {
                fields.push((key, reader.any()?));
            }
            Ok(())
        })?;
        reader.finish()?;
        let request = Value::Object(fields);
        let op = get(&request, "op", string)?;
        let answer: String = match op.as_str() {
            "version" => object([("version", Value::Number(f64::from(VERSION)))]).to_text(),
            "load-facts" => {
                let facts = facts.take().ok_or("facts: missing")?;
                self.facts.push(Some(Prepared::new(facts)));
                object([("handle", number(self.facts.len()))]).to_text()
            }
            "drop-facts" => {
                let handle = get(&request, "handle", whole)?;
                self.slot(handle)?.take();
                object([]).to_text()
            }
            "evaluate" => {
                let rule = get(&request, "rule", rule)?;
                let handle = get(&request, "handle", whole)?;
                let facts = self
                    .slot(handle)?
                    .as_ref()
                    .ok_or("the fact set was dropped")?;
                match facts.evaluate(&rule) {
                    Ok(violations) => object([
                        ("ok", Value::Bool(true)),
                        ("violations", list_of(&violations, violation_value)),
                    ])
                    .to_text(),
                    Err(errors) => object([
                        ("ok", Value::Bool(false)),
                        ("errors", list_of(&errors, error_value)),
                    ])
                    .to_text(),
                }
            }
            "select" => {
                // The check is that of the caller, so the constraint is not evaluated.
                let rule = get(&request, "rule", |rule| {
                    rule_with(rule, |_| Ok(Constraint::MustBeUsed))
                })?;
                let selection = get(&request, "rule", |rule| get(rule, "constraint", selection))?;
                let handle = get(&request, "handle", whole)?;
                let facts = self
                    .slot(handle)?
                    .as_ref()
                    .ok_or("the fact set was dropped")?;
                match facts.evaluate_check(&rule, &selection) {
                    Ok(_) => selection.answer(),
                    Err(errors) => object([
                        ("ok", Value::Bool(false)),
                        ("errors", list_of(&errors, error_value)),
                    ])
                    .to_text(),
                }
            }
            "diagram" => {
                // A diagram does not evaluate the constraint of the rule.
                let rule = get(&request, "rule", |rule| {
                    rule_with(rule, |_| Ok(Constraint::MustBeUsed))
                })?;
                let slices = optional(&request, "slices", patterns)?;
                let layers = optional(&request, "layers", |value| list(value, diagram_layer))?;
                let nodes = match (&slices, &layers) {
                    (Some(_), Some(_)) => {
                        return Err("a diagram has slices or layers, not both".to_owned())
                    }
                    (Some(slices), None) => DiagramNodes::Slices(slices),
                    (None, Some(layers)) => DiagramNodes::Layers(layers),
                    (None, None) => DiagramNodes::Subjects,
                };
                let format = optional(&request, "format", diagram_format)?.unwrap_or_default();
                let handle = get(&request, "handle", whole)?;
                let facts = self
                    .slot(handle)?
                    .as_ref()
                    .ok_or("the fact set was dropped")?;
                match facts.diagram(&rule, nodes, format) {
                    Ok(diagram) => object([
                        ("ok", Value::Bool(true)),
                        ("text", Value::String(Cow::Owned(diagram))),
                    ])
                    .to_text(),
                    Err(errors) => object([
                        ("ok", Value::Bool(false)),
                        ("errors", list_of(&errors, error_value)),
                    ])
                    .to_text(),
                }
            }
            "print-path" => {
                let path = get(&request, "path", strings)?;
                let format = get(&request, "format", path_format)?;
                text(format.print(&path)).to_text()
            }
            "render-violation" => {
                let violation = get(&request, "violation", violation)?;
                let format = get(&request, "format", path_format)?;
                text(render_violation(&violation, &format)).to_text()
            }
            "render-violations" => {
                let rule = get(&request, "rule", string)?;
                let reason = optional(&request, "reason", string)?;
                let severity = optional(&request, "severity", severity)?.unwrap_or_default();
                let violations = get(&request, "violations", |value| list(value, violation))?;
                let format = get(&request, "format", path_format)?;
                text(render_violations(
                    &rule,
                    reason.as_deref(),
                    severity,
                    &violations,
                    &format,
                ))
                .to_text()
            }
            "render-error" => text(render_error(&get(&request, "error", rule_error)?)).to_text(),
            "render-errors" => {
                let rule = get(&request, "rule", string)?;
                let errors = get(&request, "errors", |value| list(value, rule_error))?;
                text(render_errors(&rule, &errors)).to_text()
            }
            "baseline-key" => {
                let violation = get(&request, "violation", violation)?;
                let format = get(&request, "format", path_format)?;
                let (unit, what) = baseline_key(&violation, &format);
                object([
                    ("unit", Value::String(Cow::Owned(unit))),
                    ("what", Value::String(Cow::Owned(what))),
                ])
                .to_text()
            }
            "baseline-entries" => {
                let violations = get(&request, "violations", |value| list(value, violation))?;
                let format = get(&request, "format", path_format)?;
                let entries = baseline_entries(&violations, &format);
                object([("entries", list_of(&entries, entry_value))]).to_text()
            }
            "apply-baseline" => {
                let rule = get(&request, "rule", string)?;
                let violations = get(&request, "violations", |value| list(value, violation))?;
                let entries = get(&request, "entries", |value| list(value, entry))?;
                let format = get(&request, "format", path_format)?;
                let applied = apply_baseline(&rule, violations, &entries, &format);
                object([
                    ("remaining", list_of(&applied.remaining, violation_value)),
                    ("stale", list_of(&applied.stale, stale_value)),
                ])
                .to_text()
            }
            "format-baseline" => {
                let entries = get(&request, "entries", |value| list(value, entry))?;
                text(format_baseline(&entries)).to_text()
            }
            "parse-baseline" => match parse_baseline(&get(&request, "text", string)?) {
                Ok(entries) => object([
                    ("ok", Value::Bool(true)),
                    ("entries", list_of(&entries, entry_value)),
                ])
                .to_text(),
                Err(error) => {
                    object([("ok", Value::Bool(false)), ("line", number(error.line))]).to_text()
                }
            },
            other => return Err(format!("unknown request `{other}`")),
        };
        Ok(answer)
    }

    fn slot(&mut self, handle: u32) -> Result<&mut Option<Prepared>, String> {
        (handle as usize)
            .checked_sub(1)
            .and_then(|index| self.facts.get_mut(index))
            .ok_or_else(|| format!("no fact set has the handle {handle}"))
    }
}

// Reading. Each reader names the value that is wrong: `facts[3].line: expected a whole number`.

type Read<T> = Result<T, String>;

/// The value of a key that must be present.
fn get<T>(value: &Value, key: &str, read: impl Fn(&Value) -> Read<T>) -> Read<T> {
    match value.get(key) {
        Some(found) => read(found).map_err(|error| context(key, error)),
        None if matches!(value, Value::Object(_)) => Err(format!("{key}: missing")),
        None => Err("expected an object".to_owned()),
    }
}

/// The value of a key that may be absent or `null`.
fn optional<T>(value: &Value, key: &str, read: impl Fn(&Value) -> Read<T>) -> Read<Option<T>> {
    if !matches!(value, Value::Object(_)) {
        return Err("expected an object".to_owned());
    }
    match value.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(found) => read(found).map(Some).map_err(|error| context(key, error)),
    }
}

/// A list that may be absent, which is then empty.
fn list_or_empty<T>(value: &Value, key: &str, read: impl Fn(&Value) -> Read<T>) -> Read<Vec<T>> {
    Ok(optional(value, key, |found| list(found, &read))?.unwrap_or_default())
}

fn context(key: &str, error: String) -> String {
    if error.starts_with('[') {
        format!("{key}{error}")
    } else if error.contains(": ") {
        format!("{key}.{error}")
    } else {
        format!("{key}: {error}")
    }
}

/// Names the item of a list in an error.
fn item_context(index: usize, error: String) -> String {
    if error.contains(": ") || error.starts_with('[') {
        format!("[{index}].{error}").replace("].[", "][")
    } else {
        format!("[{index}]: {error}")
    }
}

fn list<T>(value: &Value, read: impl Fn(&Value) -> Read<T>) -> Read<Vec<T>> {
    match value {
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| read(item).map_err(|error| item_context(index, error)))
            .collect(),
        _ => Err("expected a list".to_owned()),
    }
}

fn string(value: &Value) -> Read<String> {
    match value {
        Value::String(text) => Ok(text.as_ref().to_owned()),
        _ => Err("expected a string".to_owned()),
    }
}

fn strings(value: &Value) -> Read<Vec<String>> {
    list(value, string)
}

fn paths(value: &Value) -> Read<Vec<Path>> {
    list(value, strings)
}

fn whole(value: &Value) -> Read<u32> {
    match value {
        Value::Number(number)
            if number.fract() == 0.0 && *number >= 0.0 && *number <= f64::from(u32::MAX) =>
        {
            Ok(*number as u32)
        }
        _ => Err("expected a whole number".to_owned()),
    }
}

fn tag(value: &Value) -> Read<String> {
    get(value, "type", string)
}

fn unknown(kind: &str, found: &str) -> String {
    format!("unknown {kind} `{found}`")
}

fn path_format(value: &Value) -> Read<PathFormat> {
    Ok(PathFormat {
        separator: get(value, "separator", string)?,
        attached_prefixes: list_or_empty(value, "attachedPrefixes", string)?,
        hidden_prefixes: list_or_empty(value, "hiddenPrefixes", string)?,
    })
}

fn certainty(value: &Value) -> Read<Certainty> {
    match string(value)?.as_str() {
        "exact" => Ok(Certainty::Exact),
        "candidate" => Ok(Certainty::Candidate),
        other => Err(unknown("certainty", other)),
    }
}

fn patterns(value: &Value) -> Read<Vec<RawPattern>> {
    paths(value)
}

fn targets(value: &Value) -> Read<Targets> {
    Ok(Targets {
        any_of: get(value, "anyOf", patterns)?,
        unverified: list_or_empty(value, "unverified", strings)?,
    })
}

fn related(value: &Value) -> Read<Related> {
    Ok(Related {
        relation: get(value, "relation", string)?,
        any_of: get(value, "anyOf", patterns)?,
    })
}

fn rule(value: &Value) -> Read<Rule> {
    rule_with(value, constraint)
}

fn diagram_format(value: &Value) -> Read<DiagramFormat> {
    match string(value)?.as_str() {
        "mermaid" => Ok(DiagramFormat::Mermaid),
        "dot" => Ok(DiagramFormat::Dot),
        other => Err(unknown("format", other)),
    }
}

fn diagram_layer(value: &Value) -> Read<DiagramLayer> {
    let must_not_use = list_or_empty(value, "mustNotUse", whole)?;
    Ok(DiagramLayer {
        name: get(value, "name", string)?,
        units: get(value, "units", patterns)?,
        must_not_use: must_not_use
            .into_iter()
            .map(|layer| layer as usize)
            .collect(),
    })
}

fn severity(value: &Value) -> Read<Severity> {
    match string(value)?.as_str() {
        "error" => Ok(Severity::Error),
        "warning" => Ok(Severity::Warning),
        other => Err(unknown("severity", other)),
    }
}

/// A rule, with its constraint read by `constraint`.
fn rule_with(value: &Value, constraint: impl Fn(&Value) -> Read<Constraint>) -> Read<Rule> {
    let include = get(value, "subjects", |subjects| {
        get(subjects, "include", |include| {
            match tag(include)?.as_str() {
                "all" => Ok(Include::All),
                "any-of" => Ok(Include::AnyOf(get(include, "patterns", patterns)?)),
                other => Err(context("type", unknown("include", other))),
            }
        })
    })?;
    let except = get(value, "subjects", |subjects| {
        optional(subjects, "except", patterns)
    })?;
    let mut rule = Rule::new(
        get(value, "name", string)?,
        Subjects { include, except },
        get(value, "constraint", constraint)?,
    );
    if let Some(kinds) = optional(value, "kinds", |kinds| match tag(kinds)?.as_str() {
        "all" => Ok(KindFilter::All),
        "only" => Ok(KindFilter::Only(get(kinds, "kinds", strings)?)),
        "except" => Ok(KindFilter::Except(get(kinds, "kinds", strings)?)),
        other => Err(context("type", unknown("kind filter", other))),
    })? {
        rule.kinds = kinds;
    }
    rule.reason = optional(value, "reason", string)?;
    rule.severity = optional(value, "severity", severity)?.unwrap_or_default();
    rule.ignore_tags = list_or_empty(value, "ignoreTags", string)?;
    rule.properties = list_or_empty(value, "properties", string)?;
    if let Some(candidates) =
        optional(value, "candidates", |found| match string(found)?.as_str() {
            "include" => Ok(Candidates::Include),
            "ignore" => Ok(Candidates::Ignore),
            other => Err(unknown("candidates", other)),
        })?
    {
        rule.candidates = candidates;
    }
    rule.exemptions = list_or_empty(value, "exemptions", |exemption| {
        Ok(Exemption {
            from: get(exemption, "from", patterns)?,
            to: get(exemption, "to", patterns)?,
        })
    })?;
    Ok(rule)
}

fn constraint(value: &Value) -> Read<Constraint> {
    Ok(match tag(value)?.as_str() {
        "must-not-use" => Constraint::MustNotUse(get(value, "targets", targets)?),
        "must-only-use" => Constraint::MustOnlyUse(get(value, "targets", targets)?),
        "must-not-contain" => Constraint::MustNotContain(get(value, "markers", strings)?),
        "must-be-acyclic" => Constraint::MustBeAcyclic(optional(value, "slices", patterns)?),
        "must-be-independent" => Constraint::MustBeIndependent(get(value, "slices", patterns)?),
        "must-be-used" => Constraint::MustBeUsed,
        "must-not-reach" => Constraint::MustNotReach(get(value, "targets", targets)?),
        "must-be-reachable-from" => Constraint::MustBeReachableFrom(get(value, "roots", patterns)?),
        "must-be-in" => Constraint::MustBeIn(get(value, "places", patterns)?),
        "must-only-be-used-by" => Constraint::MustOnlyBeUsedBy(get(value, "users", patterns)?),
        "must-not-exceed-lines" => Constraint::MustNotExceedLines(get(value, "lines", whole)?),
        "items" => Constraint::Items(items_with(value, |must| {
            Ok(match tag(must)?.as_str() {
                "not-exist" => Demand::NotExist,
                "reside-in" => Demand::ResideIn(get(must, "places", patterns)?),
                "be-named" => Demand::BeNamed(get(must, "names", strings)?),
                "have" => Demand::Have(related(must)?),
                "have-modifier" => Demand::HaveModifier(get(must, "modifiers", strings)?),
                "not-use" => Demand::NotUse(get(must, "targets", targets)?),
                "only-use" => Demand::OnlyUse(get(must, "targets", targets)?),
                "not-exceed-lines" => Demand::NotExceedLines(get(must, "lines", whole)?),
                "custom" => return Err(CUSTOM.to_owned()),
                other => return Err(context("type", unknown("demand", other))),
            })
        })?),
        "custom" => return Err(CUSTOM.to_owned()),
        other => return Err(context("type", unknown("constraint", other))),
    })
}

/// `evaluate` has no check for a custom constraint or a custom demand.
const CUSTOM: &str = "a custom check is evaluated with `select`";

/// The filters of a rule about items, with the demand read by `must`.
fn items_with(value: &Value, must: impl Fn(&Value) -> Read<Demand>) -> Read<Items> {
    Ok(Items {
        named: optional(value, "named", strings)?,
        kinds: optional(value, "kinds", strings)?,
        visibilities: optional(value, "visibilities", strings)?,
        related: list_or_empty(value, "related", related)?,
        modifiers: optional(value, "modifiers", strings)?,
        must: get(value, "must", must)?,
    })
}

/// The constraint of `select`: a custom check of units, or of the items that filters select.
fn selection(value: &Value) -> Read<select::Selection> {
    let lists = |custom: &Value| -> Read<(Vec<Targets>, Vec<Vec<RawPattern>>)> {
        match tag(custom)?.as_str() {
            "custom" => Ok((
                list_or_empty(custom, "targets", targets)?,
                list_or_empty(custom, "units", patterns)?,
            )),
            other => Err(context("type", unknown("custom check", other))),
        }
    };
    if get(value, "type", string)? != "items" {
        let (targets, units) = lists(value)?;
        return Ok(select::Selection::new(targets, units, None));
    }
    // The demand is the check of the caller.
    let filters = items_with(value, |_| Ok(Demand::NotExist))?;
    let (targets, units) = get(value, "must", lists)?;
    Ok(select::Selection::new(targets, units, Some(filters)))
}

fn step(value: &Value) -> Read<Step> {
    Ok(Step {
        from: get(value, "from", strings)?,
        to: get(value, "to", strings)?,
        file: get(value, "file", string)?,
        line: get(value, "line", whole)?,
        column: get(value, "column", whole)?,
        target: get(value, "target", strings)?,
        kind: get(value, "kind", string)?,
    })
}

fn violation(value: &Value) -> Read<Violation> {
    let detail = get(value, "detail", |detail| {
        Ok(match tag(detail)?.as_str() {
            "uses" => Detail::Uses {
                item: optional(detail, "item", strings)?,
                target: get(detail, "target", strings)?,
                written: optional(detail, "written", strings)?,
                kind: get(detail, "kind", string)?,
                certainty: get(detail, "certainty", certainty)?,
            },
            "contains" => Detail::Contains {
                marker: get(detail, "marker", string)?,
            },
            "cycle" => Detail::Cycle {
                steps: get(detail, "steps", |found| list(found, step))?,
                size: get(detail, "size", whole)? as usize,
            },
            "unused" => Detail::Unused,
            "reaches" => Detail::Reaches {
                steps: get(detail, "steps", |found| list(found, step))?,
                end: get(detail, "end", |end| {
                    Ok(End {
                        unit: get(end, "unit", strings)?,
                        file: get(end, "file", string)?,
                        line: get(end, "line", whole)?,
                        column: get(end, "column", whole)?,
                        target: get(end, "target", strings)?,
                        kind: get(end, "kind", string)?,
                    })
                })?,
            },
            "unreachable" => Detail::Unreachable,
            "outside" => Detail::Outside,
            "item" => Detail::Item {
                item: get(detail, "item", strings)?,
                kind: get(detail, "kind", string)?,
            },
            "too-long" => Detail::TooLong {
                item: optional(detail, "item", strings)?,
                kind: optional(detail, "kind", string)?,
                lines: get(detail, "lines", whole)?,
                limit: get(detail, "limit", whole)?,
            },
            "custom" => Detail::Custom {
                message: get(detail, "message", string)?,
                key: optional(detail, "key", string)?,
            },
            other => return Err(context("type", unknown("detail", other))),
        })
    })?;
    Ok(Violation {
        rule: get(value, "rule", string)?,
        unit: get(value, "unit", strings)?,
        file: get(value, "file", string)?,
        line: get(value, "line", whole)?,
        column: get(value, "column", whole)?,
        note: optional(value, "note", string)?,
        detail,
    })
}

fn rule_error(value: &Value) -> Read<RuleError> {
    let code = get(value, "code", string)?;
    let role = optional(value, "role", string)?;
    Ok(RuleError {
        code: ErrorCode::ALL
            .into_iter()
            .find(|known| known.as_str() == code)
            .ok_or_else(|| context("code", unknown("error code", &code)))?,
        role: match role {
            None => None,
            Some(role) => Some(
                Role::ALL
                    .into_iter()
                    .find(|known| known.as_str() == role)
                    .ok_or_else(|| context("role", unknown("role", &role)))?,
            ),
        },
        value: optional(value, "value", string)?,
    })
}

fn entry(value: &Value) -> Read<BaselineEntry> {
    Ok(BaselineEntry {
        rule: get(value, "rule", string)?,
        unit: get(value, "unit", string)?,
        what: get(value, "what", string)?,
        count: get(value, "count", whole)?,
    })
}

// Writing. The shapes are those that the readers above read.

fn object<'a, const N: usize>(fields: [(&'static str, Value<'a>); N]) -> Value<'a> {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (Cow::Borrowed(key), value))
            .collect(),
    )
}

fn text(text: String) -> Value<'static> {
    object([("text", Value::String(Cow::Owned(text)))])
}

/// A string that borrows the data that it is written from.
fn borrowed(text: &str) -> Value<'_> {
    Value::String(Cow::Borrowed(text))
}

fn number(value: impl TryInto<u32>) -> Value<'static> {
    Value::Number(f64::from(value.try_into().unwrap_or(u32::MAX)))
}

fn path_value(path: &[String]) -> Value<'_> {
    Value::Array(path.iter().map(|segment| borrowed(segment)).collect())
}

fn list_of<'a, T>(items: &'a [T], write: impl Fn(&'a T) -> Value<'a>) -> Value<'a> {
    Value::Array(items.iter().map(write).collect())
}

fn step_value(step: &Step) -> Value<'_> {
    object([
        ("from", path_value(&step.from)),
        ("to", path_value(&step.to)),
        ("file", borrowed(&step.file)),
        ("line", number(step.line)),
        ("column", number(step.column)),
        ("target", path_value(&step.target)),
        ("kind", borrowed(&step.kind)),
    ])
}

fn violation_value(violation: &Violation) -> Value<'_> {
    let detail = match &violation.detail {
        Detail::Uses {
            item,
            target,
            written,
            kind,
            certainty,
        } => {
            let mut fields = vec![(Cow::Borrowed("type"), borrowed("uses"))];
            if let Some(item) = item {
                fields.push((Cow::Borrowed("item"), path_value(item)));
            }
            fields.push((Cow::Borrowed("target"), path_value(target)));
            if let Some(written) = written {
                fields.push((Cow::Borrowed("written"), path_value(written)));
            }
            fields.push((Cow::Borrowed("kind"), borrowed(kind)));
            fields.push((Cow::Borrowed("certainty"), borrowed(certainty.as_str())));
            Value::Object(fields)
        }
        Detail::Contains { marker } => {
            object([("type", borrowed("contains")), ("marker", borrowed(marker))])
        }
        Detail::Cycle { steps, size } => object([
            ("type", borrowed("cycle")),
            ("steps", list_of(steps, step_value)),
            ("size", number(*size)),
        ]),
        Detail::Unused => object([("type", borrowed("unused"))]),
        Detail::Reaches { steps, end } => object([
            ("type", borrowed("reaches")),
            ("steps", list_of(steps, step_value)),
            (
                "end",
                object([
                    ("unit", path_value(&end.unit)),
                    ("file", borrowed(&end.file)),
                    ("line", number(end.line)),
                    ("column", number(end.column)),
                    ("target", path_value(&end.target)),
                    ("kind", borrowed(&end.kind)),
                ]),
            ),
        ]),
        Detail::Unreachable => object([("type", borrowed("unreachable"))]),
        Detail::Outside => object([("type", borrowed("outside"))]),
        Detail::Item { item, kind } => object([
            ("type", borrowed("item")),
            ("item", path_value(item)),
            ("kind", borrowed(kind)),
        ]),
        Detail::TooLong {
            item,
            kind,
            lines,
            limit,
        } => {
            let mut fields = vec![(Cow::Borrowed("type"), borrowed("too-long"))];
            if let Some(item) = item {
                fields.push((Cow::Borrowed("item"), path_value(item)));
            }
            if let Some(kind) = kind {
                fields.push((Cow::Borrowed("kind"), borrowed(kind)));
            }
            fields.push((Cow::Borrowed("lines"), number(*lines)));
            fields.push((Cow::Borrowed("limit"), number(*limit)));
            Value::Object(fields)
        }
        Detail::Custom { message, key } => {
            let mut fields = vec![
                (Cow::Borrowed("type"), borrowed("custom")),
                (Cow::Borrowed("message"), borrowed(message)),
            ];
            if let Some(key) = key {
                fields.push((Cow::Borrowed("key"), borrowed(key)));
            }
            Value::Object(fields)
        }
    };
    let mut fields = vec![
        (Cow::Borrowed("rule"), borrowed(&violation.rule)),
        (Cow::Borrowed("unit"), path_value(&violation.unit)),
        (Cow::Borrowed("file"), borrowed(&violation.file)),
        (Cow::Borrowed("line"), number(violation.line)),
        (Cow::Borrowed("column"), number(violation.column)),
    ];
    if let Some(note) = &violation.note {
        fields.push((Cow::Borrowed("note"), borrowed(note)));
    }
    fields.push((Cow::Borrowed("detail"), detail));
    Value::Object(fields)
}

fn error_value(error: &RuleError) -> Value<'_> {
    let mut fields = vec![(Cow::Borrowed("code"), borrowed(error.code.as_str()))];
    if let Some(role) = error.role {
        fields.push((Cow::Borrowed("role"), borrowed(role.as_str())));
    }
    if let Some(value) = &error.value {
        fields.push((Cow::Borrowed("value"), borrowed(value)));
    }
    Value::Object(fields)
}

fn entry_value(entry: &BaselineEntry) -> Value<'_> {
    object([
        ("rule", borrowed(&entry.rule)),
        ("unit", borrowed(&entry.unit)),
        ("what", borrowed(&entry.what)),
        ("count", number(entry.count)),
    ])
}

fn stale_value(entry: &StaleEntry) -> Value<'_> {
    object([
        ("unit", borrowed(&entry.unit)),
        ("what", borrowed(&entry.what)),
        ("count", number(entry.count)),
        ("found", number(entry.found)),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn call(session: &mut Session, request: &str) -> Value<'static> {
        json::parse(&session.call(request).unwrap())
            .unwrap()
            .into_owned()
    }

    const FACTS: &str = r#"{"separator": "/", "units": [["src", "a.ts"], ["src", "b.ts"]],
        "items": [["src", "b.ts", "@x"]], "kinds": ["import"], "markers": ["eval"],
        "tags": ["test"], "attachedPrefixes": ["@"], "hiddenPrefixes": ["npm:"], "facts": [
        {"unit": ["src", "a.ts"], "file": "src/a.ts", "line": 2, "column": 3, "tags": [],
         "note": "via barrel", "what": {"type": "reference", "target": ["src", "b.ts", "@x"],
         "aliases": [["npm:lib", "@y"]], "kind": "import", "certainty": "candidate"}},
        {"unit": ["src", "a.ts"], "file": "src/a.ts", "line": 3, "column": 1,
         "what": {"type": "marker", "marker": "eval"}}]}"#;

    /// A check of a binding about items gets the items that the filters of the rule select,
    /// and the references inside them.
    #[test]
    fn select_answers_the_items_that_the_filters_select() {
        let facts = r#"{"separator": "/", "units": [["a"], ["b"]], "items": [], "kinds": ["use"],
            "markers": [], "tags": ["test"], "itemKinds": ["fn", "struct"], "visibilities": ["pub"],
            "declarations": [
                {"path": ["a", "One"], "name": "One", "unit": ["a"], "file": "a", "line": 1, "column": 1, "kind": "struct", "visibility": "pub"},
                {"path": ["a", "skipped"], "name": "skipped", "unit": ["a"], "file": "a", "line": 2, "column": 1, "tags": ["test"], "kind": "fn", "visibility": "pub"},
                {"path": ["a", "two"], "name": "two", "unit": ["a"], "file": "a", "line": 3, "column": 1, "kind": "fn", "visibility": "pub"},
                {"path": ["b", "three"], "name": "three", "unit": ["b"], "file": "b", "line": 1, "column": 1, "kind": "fn", "visibility": "pub"}],
            "facts": [
                {"unit": ["a"], "file": "a", "line": 1, "column": 1, "what": {"type": "reference", "target": ["b", "three"], "kind": "use", "certainty": "exact"}},
                {"unit": ["a"], "file": "a", "line": 4, "column": 1, "item": ["a", "two", "inner"], "what": {"type": "reference", "target": ["b", "three"], "kind": "use", "certainty": "exact"}},
                {"unit": ["a"], "file": "a", "line": 5, "column": 1, "item": ["a", "One"], "what": {"type": "reference", "target": ["std", "fs"], "kind": "use", "certainty": "exact"}}]}"#;
        let mut session = Session::new();
        session
            .call(&format!(r#"{{"op": "load-facts", "facts": {facts}}}"#))
            .unwrap();
        let rule = |must: &str| {
            format!(
                r#"{{"op": "select", "handle": 1, "rule": {{"name": "r", "ignoreTags": ["test"],
                    "subjects": {{"include": {{"type": "any-of", "patterns": [["a"]]}}}},
                    "constraint": {{"type": "items", "kinds": ["fn"], "must": {must}}}}}}}"#
            )
        };
        let selected = call(
            &mut session,
            &rule(r#"{"type": "custom", "targets": [{"anyOf": [["b"]]}]}"#),
        );
        assert_eq!(
            selected.to_text(),
            r#"{"ok":true,"subjects":[0],"units":[],"references":[{"fact":0,"unit":0,"owners":[1],"hits":[0]},{"fact":1,"unit":0,"owners":[1],"hits":[0],"inside":true},{"fact":2,"unit":0,"owners":[null]}],"markers":[],"declarations":[0,2],"items":[2]}"#
        );
        // The errors of the filters are those of a rule about items.
        let invalid = rule(r#"{"type": "custom"}"#).replace(r#"["fn"]"#, r#"["class"]"#);
        assert_eq!(
            call(&mut session, &invalid).to_text(),
            r#"{"ok":false,"errors":[{"code":"unknown-item-kind","role":"item-kind","value":"class"}]}"#
        );
        // `select` takes no built-in demand, and `evaluate` no custom one.
        let built_in = session.call(&rule(r#"{"type": "not-exist"}"#)).unwrap_err();
        assert!(built_in.contains("custom check"), "{built_in}");
        let evaluated = rule(r#"{"type": "custom"}"#).replace("select", "evaluate");
        assert!(session.call(&evaluated).unwrap_err().contains("select"));
    }

    /// A violation goes out and comes back unchanged, with the path format of the facts.
    #[test]
    fn a_session_round_trip() {
        let mut session = Session::new();
        assert_eq!(
            call(&mut session, r#"{"op": "version"}"#).get("version"),
            Some(&Value::Number(1.0))
        );
        let loaded = call(
            &mut session,
            &format!(r#"{{"op": "load-facts", "facts": {FACTS}}}"#),
        );
        assert_eq!(loaded.get("handle"), Some(&Value::Number(1.0)));
        let result = call(
            &mut session,
            r#"{"op": "evaluate", "handle": 1, "rule": {"name": "r",
                "subjects": {"include": {"type": "all"}, "except": [["src", "b.ts"]]},
                "constraint": {"type": "must-not-use", "targets": {"anyOf": [["src", "b.ts"]]}},
                "kinds": {"type": "only", "kinds": ["import"]}, "candidates": "include",
                "ignoreTags": ["test"]}}"#,
        );
        assert_eq!(result.get("ok"), Some(&Value::Bool(true)));
        let violations = result.get("violations").unwrap().to_text();
        assert_eq!(
            violations,
            r#"[{"rule":"r","unit":["src","a.ts"],"file":"src/a.ts","line":2,"column":3,"note":"via barrel","detail":{"type":"uses","target":["src","b.ts","@x"],"kind":"import","certainty":"candidate"}}]"#
        );
        let format =
            r##"{"separator": "/", "attachedPrefixes": ["@"], "hiddenPrefixes": ["npm:"]}"##;
        let rendered = call(
            &mut session,
            &format!(
                r#"{{"op": "render-violations", "rule": "r", "violations": {violations}, "format": {format}}}"#
            ),
        );
        assert_eq!(
            rendered.get("text"),
            Some(&Value::String(
                "rule `r` is violated in 1 place(s):\n  src/a.ts:2:3: [r] src/a.ts uses src/b.ts@x (import, candidate, via barrel)".into()
            ))
        );
        let entries = call(
            &mut session,
            &format!(
                r#"{{"op": "baseline-entries", "violations": {violations}, "format": {format}}}"#
            ),
        );
        let entries = entries.get("entries").unwrap().to_text();
        assert_eq!(
            entries,
            r##"[{"rule":"r","unit":"src/a.ts","what":"uses import src/b.ts@x","count":1}]"##
        );
        let applied = call(
            &mut session,
            &format!(
                r#"{{"op": "apply-baseline", "rule": "r", "violations": {violations}, "entries": {entries}, "format": {format}}}"#
            ),
        );
        assert_eq!(applied.to_text(), r#"{"remaining":[],"stale":[]}"#);
        let printed = call(
            &mut session,
            &format!(
                r#"{{"op": "print-path", "path": ["npm:lib", "fp", "@map"], "format": {format}}}"#
            ),
        );
        assert_eq!(printed.to_text(), r#"{"text":"lib/fp@map"}"#);
        let text = call(
            &mut session,
            &format!(r#"{{"op": "format-baseline", "entries": {entries}}}"#),
        );
        let parsed = call(
            &mut session,
            &Value::Object(vec![
                (Cow::Borrowed("op"), borrowed("parse-baseline")),
                (Cow::Borrowed("text"), text.get("text").unwrap().clone()),
            ])
            .to_text(),
        );
        assert_eq!(parsed.get("entries").unwrap().to_text(), entries);
        let bad = call(
            &mut session,
            r#"{"op": "parse-baseline", "text": "not a baseline"}"#,
        );
        assert_eq!(bad.to_text(), r#"{"ok":false,"line":1}"#);
    }

    /// A check of a binding gets what the rule sees, and its violations come back as data.
    #[test]
    fn select_answers_what_the_rule_sees() {
        let mut session = Session::new();
        session
            .call(&format!(r#"{{"op": "load-facts", "facts": {FACTS}}}"#))
            .unwrap();
        let selected = call(
            &mut session,
            r#"{"op": "select", "handle": 1, "rule": {"name": "r",
                "subjects": {"include": {"type": "any-of", "patterns": [["src", "a.ts"]]}},
                "constraint": {"type": "custom",
                    "targets": [{"anyOf": [], "unverified": [["npm:lib"]]}, {"anyOf": [["src", "**"]]}],
                    "units": [[["src", "b.ts"]], [["src", "*"]]]}}}"#,
        );
        assert_eq!(
            selected.to_text(),
            r#"{"ok":true,"subjects":[0],"units":[[1],[0,1]],"references":[{"fact":0,"unit":0,"owners":[1,null],"hits":[1,0]}],"markers":[1],"declarations":[]}"#
        );
        // The filters of the rule apply.
        let filtered = call(
            &mut session,
            r#"{"op": "select", "handle": 1, "rule": {"name": "r",
                "subjects": {"include": {"type": "all"}}, "candidates": "ignore",
                "constraint": {"type": "custom"}}}"#,
        );
        assert_eq!(
            filtered.to_text(),
            r#"{"ok":true,"subjects":[0,1],"units":[],"references":[],"markers":[1],"declarations":[]}"#
        );
        // So do the errors of every rule, and of the targets.
        let invalid = call(
            &mut session,
            r#"{"op": "select", "handle": 1, "rule": {"name": "r",
                "subjects": {"include": {"type": "all"}},
                "constraint": {"type": "custom", "targets": [{"anyOf": [["src", "nothing.ts"]]}],
                    "units": [[["lib", "*"]], []]}}}"#,
        );
        assert_eq!(
            invalid.to_text(),
            r#"{"ok":false,"errors":[{"code":"pattern-matches-nothing","role":"target","value":"src/nothing.ts"},{"code":"pattern-matches-nothing","role":"place","value":"lib/*"},{"code":"empty-list","role":"place"}]}"#
        );
        // `evaluate` has no check for a custom constraint, and `select` takes no other one.
        let custom = r#"{"op": "evaluate", "handle": 1, "rule": {"name": "r",
            "subjects": {"include": {"type": "all"}}, "constraint": {"type": "custom"}}}"#;
        assert!(session.call(custom).unwrap_err().contains("select"));
        let other = r#"{"op": "select", "handle": 1, "rule": {"name": "r",
            "subjects": {"include": {"type": "all"}}, "constraint": {"type": "must-be-used"}}}"#;
        assert!(session.call(other).unwrap_err().contains("custom check"));

        let violation = r#"{"rule":"r","unit":["src","a.ts"],"file":"src/a.ts","line":2,"column":3,"detail":{"type":"custom","message":"is too long","key":"long"}}"#;
        let format = r#"{"separator": "/"}"#;
        let rendered = call(
            &mut session,
            &format!(
                r#"{{"op": "render-violation", "violation": {violation}, "format": {format}}}"#
            ),
        );
        assert_eq!(
            rendered.to_text(),
            r#"{"text":"src/a.ts:2:3: [r] src/a.ts is too long"}"#
        );
        let key = call(
            &mut session,
            &format!(r#"{{"op": "baseline-key", "violation": {violation}, "format": {format}}}"#),
        );
        assert_eq!(key.to_text(), r#"{"unit":"src/a.ts","what":"custom long"}"#);
        let entries = call(
            &mut session,
            &format!(
                r#"{{"op": "apply-baseline", "rule": "r", "violations": [{violation}], "entries": [], "format": {format}}}"#
            ),
        );
        assert_eq!(
            entries.get("remaining").unwrap().to_text(),
            format!("[{violation}]")
        );
    }

    #[test]
    fn errors_of_a_rule_are_an_answer() {
        let mut session = Session::new();
        session
            .call(&format!(r#"{{"op": "load-facts", "facts": {FACTS}}}"#))
            .unwrap();
        let result = call(
            &mut session,
            r#"{"op": "evaluate", "handle": 1, "rule": {"name": "",
                "subjects": {"include": {"type": "any-of", "patterns": [["nothing"]]}},
                "constraint": {"type": "must-not-contain", "markers": ["eval"]}}}"#,
        );
        let errors = result.get("errors").unwrap().to_text();
        assert_eq!(
            errors,
            r#"[{"code":"empty-name"},{"code":"pattern-matches-nothing","role":"subject","value":"nothing"}]"#
        );
        let rendered = call(
            &mut session,
            &format!(r#"{{"op": "render-errors", "rule": "", "errors": {errors}}}"#),
        );
        assert!(rendered.get("text").is_some());
    }

    /// A request that is not valid says what is wrong and where.
    #[test]
    fn invalid_requests() {
        let mut session = Session::new();
        let error = |session: &mut Session, request: &str| session.call(request).unwrap_err();
        assert_eq!(error(&mut session, "{}"), "op: missing");
        assert_eq!(
            error(&mut session, r#"{"op": "fly"}"#),
            "unknown request `fly`"
        );
        assert_eq!(
            error(
                &mut session,
                r#"{"op": "evaluate", "handle": 1, "rule": {"name": "r", "subjects": {"include": {"type": "all"}}, "constraint": {"type": "must-be-used"}}}"#
            ),
            "no fact set has the handle 1"
        );
        let broken = FACTS.replace(r#""line": 3"#, r#""line": "three""#);
        assert_eq!(
            error(
                &mut session,
                &format!(r#"{{"op": "load-facts", "facts": {broken}}}"#)
            ),
            "facts.facts[1].line: expected a whole number"
        );
        assert_eq!(
            error(
                &mut session,
                r#"{"op": "evaluate", "handle": 1, "rule": {"name": "r", "subjects": {"include": {"type": "some"}}}}"#
            ),
            "rule.subjects.include.type: unknown include `some`"
        );
        session
            .call(&format!(r#"{{"op": "load-facts", "facts": {FACTS}}}"#))
            .unwrap();
        session
            .call(r#"{"op": "drop-facts", "handle": 1}"#)
            .unwrap();
        assert_eq!(
            error(
                &mut session,
                r#"{"op": "evaluate", "handle": 1, "rule": {"name": "r", "subjects": {"include": {"type": "all"}}, "constraint": {"type": "must-be-used"}}}"#
            ),
            "the fact set was dropped"
        );
    }
}
