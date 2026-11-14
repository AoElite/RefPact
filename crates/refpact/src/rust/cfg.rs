//! The `cfg` conditions of the code: each is a tag of the facts under it, and a load for one
//! target leaves out what the conditions switch off there.

use std::collections::BTreeSet;

use crate::core::FactSet;
use syn::punctuated::Punctuated;
use syn::{Attribute, Meta, Token};

/// The options that the target of a build decides. Other options, such as `debug_assertions`
/// and `test`, depend on how the code is built.
const TARGET_KEYS: [&str; 11] = [
    "target_abi",
    "target_arch",
    "target_endian",
    "target_env",
    "target_family",
    "target_has_atomic",
    "target_os",
    "target_pointer_width",
    "target_vendor",
    "unix",
    "windows",
];

/// The tags of the `#[cfg(..)]` attributes among some attributes: one for each condition
/// that must hold, so `#[cfg(all(unix, feature = "tls"))]` gives `cfg(unix)` and
/// `cfg(feature = "tls")`.
pub(crate) fn tags(attrs: &[Attribute]) -> Vec<String> {
    let mut tags = Vec::new();
    for attr in attrs {
        if attr.path().is_ident("cfg") {
            if let Ok(meta) = attr.parse_args::<Meta>() {
                conjuncts(&meta, &mut tags);
            }
        }
    }
    tags
}

/// The tag of a condition that a rule names, such as `windows` or `feature = "tls"`. A text
/// that is no condition is its own tag, which no fact has.
pub(crate) fn tag_of(condition: &str) -> String {
    match syn::parse_str::<Meta>(condition) {
        Ok(meta) => format!("cfg({})", text(&meta)),
        Err(_) => format!("cfg({condition})"),
    }
}

/// The tags of a condition as a manifest writes it for a table of dependencies:
/// `cfg(windows)`. The name of a target, which is no condition, has no tag.
pub(crate) fn tags_of_target(target: &str) -> Vec<String> {
    let mut tags = Vec::new();
    let inside = target.trim().strip_prefix("cfg(");
    let inside = inside.and_then(|rest| rest.strip_suffix(')'));
    if let Some(Ok(meta)) = inside.map(syn::parse_str::<Meta>) {
        conjuncts(&meta, &mut tags);
    }
    tags
}

fn members(meta: &Meta) -> Vec<Meta> {
    let Meta::List(list) = meta else {
        return Vec::new();
    };
    list.parse_args_with(Punctuated::<Meta, Token![,]>::parse_terminated)
        .map(|metas| metas.into_iter().collect())
        .unwrap_or_default()
}

fn conjuncts(meta: &Meta, tags: &mut Vec<String>) {
    if meta.path().is_ident("all") && matches!(meta, Meta::List(_)) {
        for member in &members(meta) {
            conjuncts(member, tags);
        }
        return;
    }
    let tag = format!("cfg({})", text(meta));
    if !tags.contains(&tag) {
        tags.push(tag);
    }
}

fn name(meta: &Meta) -> String {
    let names = meta.path().segments.iter();
    let names: Vec<String> = names.map(|segment| segment.ident.to_string()).collect();
    names.join("::")
}

/// The value of `key = "value"`.
fn value(meta: &Meta) -> Option<String> {
    let Meta::NameValue(pair) = meta else {
        return None;
    };
    match &pair.value {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(text),
            ..
        }) => Some(text.value()),
        _ => None,
    }
}

/// A condition in one form, whatever the spaces of the code are.
fn text(meta: &Meta) -> String {
    match meta {
        Meta::Path(_) => name(meta),
        Meta::NameValue(_) => match value(meta) {
            Some(value) => format!("{} = {value:?}", name(meta)),
            None => format!("{} = ?", name(meta)),
        },
        Meta::List(_) => {
            let members: Vec<String> = members(meta).iter().map(text).collect();
            format!("{}({})", name(meta), members.join(", "))
        }
    }
}

/// What a load knows of the build that it checks. Empty by default: all code is checked.
#[derive(Debug, Clone, Default)]
pub(crate) struct Settings {
    /// The name of the target, or `None` in it for the target of this machine.
    pub target: Option<Option<String>>,
    /// The Cargo features that are on.
    pub features: Option<BTreeSet<String>>,
    /// The options of the target, as `rustc --print cfg` writes them.
    options: BTreeSet<String>,
}

impl Settings {
    /// Asks the compiler for the options of the target, when a target is set.
    pub fn resolve(
        &mut self,
        options_of: impl Fn(Option<&str>) -> Result<String, String>,
    ) -> Result<(), String> {
        if let Some(target) = &self.target {
            let printed = options_of(target.as_deref())?;
            self.options = printed.lines().map(|line| line.trim().to_owned()).collect();
        }
        Ok(())
    }

    /// Whether the condition of a tag holds in the build. `None` when the settings do not
    /// say, and for a tag that is no condition.
    fn holds(&self, tag: &str) -> Option<bool> {
        let condition = tag.strip_prefix("cfg(")?.strip_suffix(')')?;
        self.eval(&syn::parse_str::<Meta>(condition).ok()?)
    }

    fn eval(&self, meta: &Meta) -> Option<bool> {
        let key = name(meta);
        match meta {
            Meta::Path(_) => self.option(&key, None),
            Meta::NameValue(_) => self.option(&key, Some(&value(meta)?)),
            Meta::List(_) => {
                let members = members(meta);
                let mut results = members.iter().map(|member| self.eval(member));
                match key.as_str() {
                    "not" if members.len() == 1 => results.next()?.map(|holds| !holds),
                    // False with one false member, true when all are true.
                    "all" => results
                        .try_fold(Some(true), |all, one| match (all, one) {
                            (_, Some(false)) => Err(()),
                            (Some(true), Some(true)) => Ok(Some(true)),
                            _ => Ok(None),
                        })
                        .unwrap_or(Some(false)),
                    "any" => results
                        .try_fold(Some(false), |any, one| match (any, one) {
                            (_, Some(true)) => Err(()),
                            (Some(false), Some(false)) => Ok(Some(false)),
                            _ => Ok(None),
                        })
                        .unwrap_or(Some(true)),
                    _ => None,
                }
            }
        }
    }

    fn option(&self, key: &str, value: Option<&str>) -> Option<bool> {
        if key == "feature" {
            let features = self.features.as_ref()?;
            return Some(value.is_some_and(|value| features.contains(value)));
        }
        self.target.as_ref()?;
        if !TARGET_KEYS.contains(&key) {
            return None;
        }
        let printed = match value {
            Some(value) => format!("{key}={value:?}"),
            None => key.to_owned(),
        };
        Some(self.options.contains(&printed))
    }

    /// Whether one of the tags is a condition that does not hold in the build.
    pub fn leaves_out(&self, tags: &[String]) -> bool {
        tags.iter().any(|tag| self.holds(tag) == Some(false))
    }
}

/// Completes a fact set: its tags are the tag of test code and the conditions of its facts
/// and its declarations, and what the build of the settings leaves out is taken away.
pub(crate) fn finish(facts: &mut FactSet, settings: &Settings) {
    let mut conditions: BTreeSet<&String> = BTreeSet::new();
    let own = facts.facts.iter().map(|fact| &fact.tags);
    let declared = facts
        .declarations
        .iter()
        .map(|declaration| &declaration.tags);
    for tags in own.chain(declared) {
        conditions.extend(tags.iter().filter(|tag| !facts.tags.contains(tag)));
    }
    let conditions: Vec<String> = conditions.into_iter().cloned().collect();
    facts.tags.extend(conditions);

    let off: BTreeSet<String> = facts
        .tags
        .iter()
        .filter(|tag| settings.holds(tag) == Some(false))
        .cloned()
        .collect();
    if off.is_empty() {
        return;
    }
    let left_out = |tags: &[String]| tags.iter().any(|tag| off.contains(tag));
    facts.facts.retain(|fact| !left_out(&fact.tags));
    facts
        .declarations
        .retain(|declaration| !left_out(&declaration.tags));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn attrs(text: &str) -> Vec<Attribute> {
        let item: syn::ItemFn = syn::parse_str(&format!("{text} fn f() {{}}")).unwrap();
        item.attrs
    }

    #[test]
    fn each_condition_that_must_hold_is_a_tag() {
        assert_eq!(tags(&attrs("#[cfg(windows)]")), ["cfg(windows)"]);
        assert_eq!(
            tags(&attrs(
                r#"#[cfg(all(unix,feature="tls"))] #[cfg(not( windows ))] #[inline]"#
            )),
            ["cfg(unix)", r#"cfg(feature = "tls")"#, "cfg(not(windows))"]
        );
        assert_eq!(
            tags(&attrs(
                r#"#[cfg(any(unix, all(windows, test)))] #[cfg_attr(unix, inline)]"#
            )),
            ["cfg(any(unix, all(windows, test)))"]
        );
        assert_eq!(tag_of(r#"feature="tls""#), r#"cfg(feature = "tls")"#);
        assert_eq!(tag_of("not a condition"), "cfg(not a condition)");
        assert_eq!(
            tags_of_target(r#"cfg(all(unix, target_arch="x86_64"))"#),
            ["cfg(unix)", r#"cfg(target_arch = "x86_64")"#]
        );
        assert_eq!(
            tags_of_target("x86_64-pc-windows-msvc"),
            Vec::<String>::new()
        );
    }

    #[test]
    fn a_condition_holds_is_off_or_is_not_known() {
        let mut linux = Settings {
            target: Some(None),
            features: Some(BTreeSet::from(["tls".to_owned()])),
            ..Settings::default()
        };
        let printed = "debug_assertions\ntarget_os=\"linux\"\ntarget_family=\"unix\"\nunix\n";
        linux.resolve(|_| Ok(printed.to_owned())).unwrap();
        let holds = |condition: &str| linux.holds(&tag_of(condition));
        assert_eq!(holds("unix"), Some(true));
        assert_eq!(holds("windows"), Some(false));
        assert_eq!(holds(r#"target_os = "linux""#), Some(true));
        assert_eq!(holds(r#"target_os = "macos""#), Some(false));
        assert_eq!(holds(r#"feature = "tls""#), Some(true));
        assert_eq!(holds(r#"feature = "gui""#), Some(false));
        // The target does not decide these.
        assert_eq!(holds("test"), None);
        assert_eq!(holds("debug_assertions"), None);
        assert_eq!(holds("not(windows)"), Some(true));
        assert_eq!(holds("not(test)"), None);
        assert_eq!(holds("any(windows, test)"), None);
        assert_eq!(holds("any(windows, unix, test)"), Some(true));
        assert_eq!(holds("all(unix, test)"), None);
        assert_eq!(holds("all(windows, test)"), Some(false));
        assert_eq!(holds("any(windows, target_os = \"macos\")"), Some(false));
        assert_eq!(holds("all(unix, not(windows))"), Some(true));
        assert!(linux.leaves_out(&["test".to_owned(), "cfg(windows)".to_owned()]));
        assert!(!linux.leaves_out(&["test".to_owned(), "cfg(unix)".to_owned()]));

        // Without settings, nothing is known.
        let all = Settings::default();
        assert_eq!(all.holds("cfg(windows)"), None);
        assert_eq!(all.holds(r#"cfg(feature = "tls")"#), None);
    }
}
