//! Groups: several rules that are checked together and report every failure.

use crate::{Baseline, Crate, Failure, Rule};

/// Groups rules, so that one check reports every rule that fails, not only the first.
///
/// ```no_run
/// use refpact::{all_except, any_of, rule, rules, Crate};
///
/// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
///
/// rules(
///     "the platform rules",
///     [
///         rule("files are opened in the platform layer only")
///             .modules(all_except(["crate::platform::**"]))
///             .must_not_use(any_of(["std::fs"])),
///         rule("the platform layer forms no cycle")
///             .modules(any_of(["crate::platform::**"]))
///             .must_be_acyclic(),
///     ],
/// )
/// .check(&app);
/// ```
pub fn rules(name: impl Into<String>, rules: impl IntoIterator<Item = Rule>) -> Rules {
    Rules {
        name: name.into(),
        rules: rules.into_iter().collect(),
    }
}

/// Rules that are checked together. See [`rules`].
#[derive(Debug, Clone)]
pub struct Rules {
    name: String,
    rules: Vec<Rule>,
}

impl RuleSet for Rules {
    fn name(&self) -> &str {
        &self.name
    }

    fn rules(&self) -> Result<Vec<Rule>, Failure> {
        Ok(self.rules.clone())
    }
}

/// Rules that are checked together, so that one check reports every rule that fails.
/// [`rules`], [`layers`](crate::layers) and [`onion_architecture`](crate::onion_architecture)
/// give sets of rules, and a type of your own can be one: say which rules it stands for, and
/// it is checked as they are.
///
/// ```no_run
/// use refpact::{any_of, rule, Crate, Failure, Rule, RuleSet};
///
/// /// Each part keeps away from the database.
/// struct NoDatabase(&'static [&'static str]);
///
/// impl RuleSet for NoDatabase {
///     fn name(&self) -> &str {
///         "no part uses the database"
///     }
///
///     fn rules(&self) -> Result<Vec<Rule>, Failure> {
///         let rule_of = |part: &&str| {
///             rule(format!("{part} does not use the database"))
///                 .modules(any_of([format!("crate::{part}::**")]))
///                 .must_not_use(any_of(["crate::db"]))
///         };
///         Ok(self.0.iter().map(rule_of).collect())
///     }
/// }
///
/// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
/// NoDatabase(&["api", "domain"]).check(&app);
/// ```
pub trait RuleSet {
    /// The name of the set. It names the failure of a set without rules.
    fn name(&self) -> &str;

    /// The rules of the set. The error is [`Failure::Malformed`] when the set cannot fail
    /// as written.
    fn rules(&self) -> Result<Vec<Rule>, Failure>;

    /// Checks all rules and returns what failed: the failure of one rule, or
    /// [`Failure::Several`] in the order of the rules. A set without rules cannot fail, so
    /// it is [`Failure::Malformed`].
    fn verify(&self, krate: &Crate) -> Result<(), Failure> {
        all(self, |rule| rule.verify(krate))
    }

    /// Fails the test when a rule is violated or cannot be evaluated. The report has every
    /// rule that fails.
    #[track_caller]
    fn check(&self, krate: &Crate) {
        if let Err(failure) = self.verify(krate) {
            panic!("{failure}");
        }
    }

    /// Like [`RuleSet::verify`], with a baseline. See [`Rule::verify_with_baseline`].
    fn verify_with_baseline(&self, krate: &Crate, baseline: &Baseline) -> Result<(), Failure> {
        all(self, |rule| rule.verify_with_baseline(krate, baseline))
    }

    /// Like [`RuleSet::check`], with a baseline. See [`Rule::check_with_baseline`].
    #[track_caller]
    fn check_with_baseline(&self, krate: &Crate, baseline: &Baseline) {
        if let Err(failure) = self.verify_with_baseline(krate, baseline) {
            panic!("{failure}");
        }
    }
}

/// What `verify` gives for each rule of a set, as one result.
fn all<S: RuleSet + ?Sized>(
    set: &S,
    verify: impl Fn(&Rule) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let rules = set.rules()?;
    if rules.is_empty() {
        return Err(Failure::Malformed {
            rule: set.name().to_owned(),
            message: "the group has no rules, so it cannot fail".to_owned(),
        });
    }
    let mut failures: Vec<Failure> = rules.iter().filter_map(|rule| verify(rule).err()).collect();
    match failures.len() {
        0 => Ok(()),
        1 => Err(failures.remove(0)),
        _ => Err(Failure::Several(failures)),
    }
}

/// The methods of [`RuleSet`] as methods of a type of this crate, so that a test calls them
/// without the trait in scope.
macro_rules! rule_set_methods {
    ($set:ident) => {
        impl $set {
            /// Checks all rules and returns what failed: the failure of one rule, or
            /// [`Failure::Several`](crate::Failure::Several) in the order of the rules.
            pub fn verify(&self, krate: &crate::Crate) -> Result<(), crate::Failure> {
                crate::RuleSet::verify(self, krate)
            }

            /// Fails the test when a rule is violated or cannot be evaluated. The report has
            /// every rule that fails.
            #[track_caller]
            pub fn check(&self, krate: &crate::Crate) {
                crate::RuleSet::check(self, krate)
            }

            /// Like `verify`, with a baseline. See
            /// [`Rule::verify_with_baseline`](crate::Rule::verify_with_baseline).
            pub fn verify_with_baseline(
                &self,
                krate: &crate::Crate,
                baseline: &crate::Baseline,
            ) -> Result<(), crate::Failure> {
                crate::RuleSet::verify_with_baseline(self, krate, baseline)
            }

            /// Like `check`, with a baseline. See
            /// [`Rule::check_with_baseline`](crate::Rule::check_with_baseline).
            #[track_caller]
            pub fn check_with_baseline(&self, krate: &crate::Crate, baseline: &crate::Baseline) {
                crate::RuleSet::check_with_baseline(self, krate, baseline)
            }
        }
    };
}
pub(crate) use rule_set_methods;

rule_set_methods!(Rules);
