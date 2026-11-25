//! The filters of a rule: what it leaves out, why it exists and how much it weighs.
//!
//! A [`Rule`](crate::Rule), [`Layers`](crate::Layers) and an [`Onion`](crate::Onion) have the
//! same filters. Each filter is written once, here: [`filter_methods`] gives its method to
//! all of them, so a new filter cannot be missing from one.

use crate::core::{Candidates, Exemption, KindFilter, Severity};

/// The kinds that a rule with the filter `known` counts when it also counts only `kinds`.
/// Each filter of a rule holds: a later one does not take the place of an earlier one.
fn only(known: &KindFilter, kinds: Vec<String>) -> KindFilter {
    let mut kinds = kinds;
    match known {
        KindFilter::All => {}
        KindFilter::Only(counted) => kinds.retain(|kind| counted.contains(kind)),
        KindFilter::Except(ignored) => kinds.retain(|kind| !ignored.contains(kind)),
    }
    KindFilter::Only(kinds)
}

/// The kinds that a rule with the filter `known` counts when it also leaves out `kinds`.
fn except(known: &KindFilter, kinds: Vec<String>) -> KindFilter {
    match known {
        KindFilter::All => KindFilter::Except(kinds),
        KindFilter::Only(counted) => {
            let left = counted.iter().filter(|kind| !kinds.contains(kind));
            KindFilter::Only(left.cloned().collect())
        }
        KindFilter::Except(ignored) => {
            let more = kinds.into_iter().filter(|kind| !ignored.contains(kind));
            KindFilter::Except(ignored.iter().cloned().chain(more).collect())
        }
    }
}

/// One filter, as a value that a set of rules keeps until its rules are made.
#[derive(Debug, Clone)]
pub(crate) enum Filter {
    /// Why the rule exists.
    Reason(String),
    /// The violations are reported and do not fail the check.
    Warning,
    /// Facts with this tag are left out.
    IgnoreTag(String),
    /// References that the analysis could not prove are left out.
    IgnoreCandidates,
    /// Only these kinds of reference count.
    OnlyKinds(Vec<String>),
    /// These kinds of reference do not count.
    IgnoreKinds(Vec<String>),
    /// The rule does not see these references.
    Exempt(Exemption),
}

impl Filter {
    /// Sets the filter on a rule of the core.
    pub fn apply(self, rule: &mut crate::core::Rule) {
        match self {
            Filter::Reason(reason) => rule.reason = Some(reason),
            Filter::Warning => rule.severity = Severity::Warning,
            Filter::IgnoreTag(tag) => rule.ignore_tags.push(tag),
            Filter::IgnoreCandidates => rule.candidates = Candidates::Ignore,
            Filter::OnlyKinds(kinds) => rule.kinds = only(&rule.kinds, kinds),
            Filter::IgnoreKinds(kinds) => rule.kinds = except(&rule.kinds, kinds),
            Filter::Exempt(exemption) => rule.exemptions.push(exemption),
        }
    }

    /// Whether the filter says which references a rule sees. A rule that looks at no
    /// reference, such as "every module is in a layer", does not get such a filter: there, a
    /// tag that no code has or an exemption that lets nothing pass would be an error.
    pub fn is_about_references(&self) -> bool {
        !matches!(self, Filter::Reason(_) | Filter::Warning)
    }
}

/// What has filters. The methods of [`filter_methods`] call this.
pub(crate) trait Filtered: Sized {
    /// The same value with one more filter.
    fn filter(self, filter: Filter) -> Self;
}

/// The public methods of the filters, for a type that is [`Filtered`].
macro_rules! filter_methods {
    ($type:ident) => {
        impl $type {
            /// Says why this exists. The report of a violation prints the reason below the
            /// name of the rule, so whoever breaks the rule learns what it protects.
            pub fn because(self, reason: impl Into<String>) -> $type {
                let filter = crate::filters::Filter::Reason(reason.into());
                crate::filters::Filtered::filter(self, filter)
            }

            /// Makes this a warning: the violations are reported, and the check passes.
            /// `verify` and `check` write the report to stderr, which the test harness shows
            /// for a test that passes only with `cargo test -- --show-output`.
            ///
            /// A rule that cannot fail as written is still an error, and so is a stale
            /// baseline. [`Rule::evaluate`](crate::Rule::evaluate) gives the violations of a
            /// warning as of any rule.
            ///
            /// ```no_run
            /// use refpact::{all, any_of, rule, Crate};
            ///
            /// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
            ///
            /// rule("new code does not print")
            ///     .modules(all())
            ///     .must_not_use(any_of(["std::println!"]))
            ///     .as_warning()
            ///     .check(&app);
            /// ```
            pub fn as_warning(self) -> $type {
                crate::filters::Filtered::filter(self, crate::filters::Filter::Warning)
            }

            /// Leaves out code inside `#[cfg(test)]` and test functions.
            pub fn except_in_test_code(self) -> $type {
                let tag = crate::rust::TAG_TEST.to_owned();
                crate::filters::Filtered::filter(self, crate::filters::Filter::IgnoreTag(tag))
            }

            /// Leaves out what the expansions of macros hold. Only code that is loaded with
            /// [`Options::expanding_macros`](crate::Options::expanding_macros) has such code.
            pub fn ignoring_generated(self) -> $type {
                let tag = crate::rust::TAG_GENERATED.to_owned();
                crate::filters::Filtered::filter(self, crate::filters::Filter::IgnoreTag(tag))
            }

            /// Leaves out code under a `cfg` condition, named as the code writes it:
            /// `"windows"`, `"feature = \"tls\""` or `"not(unix)"`. Call this again for more
            /// conditions.
            ///
            /// Code is under a condition when a `#[cfg(..)]` around it requires it:
            /// `#[cfg(all(windows, feature = "tls"))]` is under `windows` and under
            /// `feature = "tls"`. A condition that no code is under is an error.
            ///
            /// To check code as one target builds it, see
            /// [`Options::target`](crate::Options::target).
            pub fn except_under_cfg(self, condition: &str) -> $type {
                let tag = crate::rust::cfg_tag(condition);
                crate::filters::Filtered::filter(self, crate::filters::Filter::IgnoreTag(tag))
            }

            /// Leaves out references that the analysis could not prove, such as a name that
            /// may come through a glob import of another crate.
            pub fn ignoring_candidates(self) -> $type {
                let filter = crate::filters::Filter::IgnoreCandidates;
                crate::filters::Filtered::filter(self, filter)
            }

            /// Counts only these kinds of reference. With another filter of kinds, both
            /// hold: `only_kinds` twice counts the kinds that both name.
            pub fn only_kinds(self, kinds: impl IntoIterator<Item = crate::Kind>) -> $type {
                let kinds = kinds.into_iter().map(|kind| kind.as_str().to_owned());
                let filter = crate::filters::Filter::OnlyKinds(kinds.collect());
                crate::filters::Filtered::filter(self, filter)
            }

            /// Counts all kinds of reference except these, also with another filter of
            /// kinds: `ignoring_kinds` twice leaves out the kinds of both.
            pub fn ignoring_kinds(self, kinds: impl IntoIterator<Item = crate::Kind>) -> $type {
                let kinds = kinds.into_iter().map(|kind| kind.as_str().to_owned());
                let filter = crate::filters::Filter::IgnoreKinds(kinds.collect());
                crate::filters::Filtered::filter(self, filter)
            }

            /// The references from these modules to these paths are not seen: one known
            /// dependency that is allowed. The modules are a selection as in
            /// [`modules`](crate::Named::modules): `any_of(..)`, `all()`, `all_except(..)`
            /// or `any_of(..).except(..)`. Paths match what is inside them, as targets do.
            /// For a cycle or a chain, the reference is no edge.
            ///
            /// An exemption that lets no reference pass is an error, so it goes when the
            /// code that it allows goes. Call this again for more exemptions.
            pub fn ignoring_use(self, from: impl Into<crate::Modules>, to: crate::AnyOf) -> $type {
                let exemption = crate::core::Exemption {
                    from: crate::subjects(from.into()),
                    to: to.split(),
                };
                let filter = crate::filters::Filter::Exempt(exemption);
                crate::filters::Filtered::filter(self, filter)
            }
        }
    };
}
pub(crate) use filter_methods;
