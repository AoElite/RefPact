//! Architecture tests for Rust: rules about what code may depend on, checked as ordinary
//! tests on stable Rust.
//!
//! ```no_run
//! use refpact::{all_except, any_of, rule, Crate};
//!
//! let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
//!
//! rule("the interpreter does not use the code generator")
//!     .modules(any_of(["crate::interpreter::**"]))
//!     .must_not_use(any_of(["crate::codegen"]))
//!     .check(&app);
//!
//! rule("files are opened in the platform layer only")
//!     .modules(all_except(["crate::platform::**"]))
//!     .must_not_use(any_of(["std::fs"]))
//!     .check(&app);
//! ```
//!
//! A pattern is a module path or an item path. `*` matches one segment or a part of one
//! segment, `**` matches zero or more segments. A module pattern selects exactly the modules
//! it matches: `crate::interpreter` is one module, `crate::interpreter::**` is that module
//! and all modules below it. A target covers the path and everything inside it: `std::fs`
//! covers `std::fs::read`.
//!
//! A rule that cannot fail is an error. Every pattern must match something by itself, also
//! each pattern of a list and each exception.

#![warn(missing_docs)]

/// The language-neutral core: the rule model, the evaluation, reports and baselines. It is the
/// crate `refpact-core`, which the TypeScript package also runs, compiled to WebAssembly.
pub use refpact_core as core;

mod baseline;
mod diagram;
mod filters;
mod group;
mod items;
mod layers;
mod onion;
mod rust;

use std::fmt;

use crate::core::{Constraint, DiagramNodes, Include, Subjects, Targets};
use crate::filters::{filter_methods, Filter, Filtered};

pub use crate::core::{
    Check, Checker, Detail, DiagramFormat, End, ErrorCode, ItemCheck, ItemChecker, ItemScope, Role,
    RuleError, Scope, Seen, Setup, StaleEntry, Step, Violation,
};
pub use crate::rust::{
    cfg_tag, Crate, LoadError, Member, Options, Workspace, WorkspaceOptions,
    EXPRESSION_LIST_MACROS, OTHER_PARSED_MACROS, PROPERTY_LICENSE, PROPERTY_SOURCE,
    PROPERTY_VERSION, TAG_GENERATED, TAG_TEST,
};
pub use baseline::Baseline;
pub use group::{rules, RuleSet, Rules};
pub use items::{ItemKind, ItemSelection, Modifier, Visibility};
pub use layers::{layers, Layers};
pub use onion::{onion_architecture, Onion};

/// The Rust examples of the README and of `docs/`. They are documentation of this item only
/// when the documentation tests are built, so `cargo test` compiles each one.
/// `tests/readme_examples.rs` keeps the file equal to the documents.
#[cfg(doctest)]
#[doc = include_str!("../tests/readme_examples.md")]
#[allow(dead_code)]
struct ReadmeExamples;

/// Starts a rule. The name is printed with every violation.
pub fn rule(name: impl Into<String>) -> Named {
    Named { name: name.into() }
}

/// The union of some patterns: modules for [`Named::modules`], targets for
/// [`Scoped::must_not_use`].
pub fn any_of<I, S>(patterns: I) -> AnyOf
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    AnyOf {
        patterns: texts(patterns),
    }
}

/// All modules of the crate.
pub fn all() -> Modules {
    Modules {
        include: None,
        except: None,
    }
}

/// All modules of the crate except those that match one of the patterns.
pub fn all_except<I, S>(patterns: I) -> Modules
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    Modules {
        include: None,
        except: Some(texts(patterns)),
    }
}

/// Targets that are not checked for existence. Use this only for names inside the crate
/// that the source does not show, such as items that a macro generates.
pub fn unverified<I, S>(patterns: I) -> Uses
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    Uses {
        any_of: Vec::new(),
        unverified: texts(patterns),
    }
}

/// All unsafe constructs.
pub fn unsafe_code() -> [Marker; 6] {
    [
        Marker::UnsafeFn,
        Marker::UnsafeBlock,
        Marker::UnsafeImpl,
        Marker::UnsafeTrait,
        Marker::UnsafeExtern,
        Marker::UnsafeAttr,
    ]
}

fn texts<I, S>(patterns: I) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    patterns
        .into_iter()
        .map(|pattern| pattern.as_ref().to_owned())
        .collect()
}

/// A list of patterns that form a union.
#[derive(Debug, Clone)]
pub struct AnyOf {
    patterns: Vec<String>,
}

impl AnyOf {
    /// The patterns as the core sees them.
    pub(crate) fn split(&self) -> Vec<Vec<String>> {
        self.patterns.iter().map(|pattern| split(pattern)).collect()
    }

    /// Removes the modules that match one of `patterns` from the selection.
    pub fn except<I, S>(self, patterns: I) -> Modules
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Modules {
            include: Some(self.patterns),
            except: Some(texts(patterns)),
        }
    }

    /// Adds targets that are not checked for existence. See [`unverified`].
    pub fn and_unverified<I, S>(self, patterns: I) -> Uses
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        Uses {
            any_of: self.patterns,
            unverified: texts(patterns),
        }
    }
}

/// A selection of modules.
#[derive(Debug, Clone)]
pub struct Modules {
    include: Option<Vec<String>>,
    except: Option<Vec<String>>,
}

impl From<AnyOf> for Modules {
    fn from(any_of: AnyOf) -> Modules {
        Modules {
            include: Some(any_of.patterns),
            except: None,
        }
    }
}

/// A set of forbidden targets.
#[derive(Debug, Clone)]
pub struct Uses {
    any_of: Vec<String>,
    unverified: Vec<String>,
}

impl Uses {
    /// The targets as the core sees them.
    pub(crate) fn targets(self) -> Targets {
        let patterns = |list: Vec<String>| list.iter().map(|pattern| split(pattern)).collect();
        Targets {
            any_of: patterns(self.any_of),
            unverified: patterns(self.unverified),
        }
    }
}

impl From<AnyOf> for Uses {
    fn from(any_of: AnyOf) -> Uses {
        Uses {
            any_of: any_of.patterns,
            unverified: Vec::new(),
        }
    }
}

/// How the code refers to a path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// `use a::b;`
    Use,
    /// `use a::b::*;`
    UseGlob,
    /// `extern crate a;`
    ExternCrate,
    /// A path in an expression, also inside the arguments of the macros that are parsed.
    Expr,
    /// A path in a type.
    Type,
    /// A path in a pattern.
    Pattern,
    /// A trait in a bound, in `impl Trait` or in `dyn Trait`.
    Bound,
    /// The trait of `impl Trait for Type`.
    Impl,
    /// The path of an attribute or of a derive, and paths in the arguments of attributes.
    Attribute,
    /// The macro of a macro call. The path ends in `!`.
    Macro,
    /// A path found in the tokens of a macro call that is not parsed.
    MacroTokens,
    /// A call of a method: `file.read_to_end(..)` is a use of `std::fs::File::read_to_end`.
    /// Only code that is loaded with
    /// [`Options::resolving_members`](crate::Options::resolving_members) has this kind; for
    /// other code, a rule that names it is an error.
    MethodCall,
    /// An access to a field: `order.total` is a use of `crate::domain::Order::total`. As
    /// [`Kind::MethodCall`], it needs [`Options::resolving_members`](crate::Options::resolving_members).
    Field,
    /// A dependency that `[dependencies]` of the manifest names. The kinds of the
    /// dependencies are only in code that is loaded with
    /// [`Options::reading_dependencies`](crate::Options::reading_dependencies).
    Dependency,
    /// A dependency that `[dev-dependencies]` names. It has the tag of test code.
    DevDependency,
    /// A dependency that `[build-dependencies]` names.
    BuildDependency,
    /// A package that a dependency needs and that the manifest does not name.
    TransitiveDependency,
}

impl Kind {
    fn as_str(self) -> &'static str {
        use crate::rust::kind;
        match self {
            Kind::Use => kind::USE,
            Kind::UseGlob => kind::USE_GLOB,
            Kind::ExternCrate => kind::EXTERN_CRATE,
            Kind::Expr => kind::EXPR,
            Kind::Type => kind::TYPE,
            Kind::Pattern => kind::PATTERN,
            Kind::Bound => kind::BOUND,
            Kind::Impl => kind::IMPL,
            Kind::Attribute => kind::ATTRIBUTE,
            Kind::Macro => kind::MACRO,
            Kind::MacroTokens => kind::MACRO_TOKENS,
            Kind::MethodCall => kind::METHOD_CALL,
            Kind::Field => kind::FIELD,
            Kind::Dependency => kind::DEPENDENCY,
            Kind::DevDependency => kind::DEV_DEPENDENCY,
            Kind::BuildDependency => kind::BUILD_DEPENDENCY,
            Kind::TransitiveDependency => kind::TRANSITIVE_DEPENDENCY,
        }
    }
}

/// Something a module contains that has no path.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Marker {
    /// `unsafe fn`
    UnsafeFn,
    /// `unsafe { .. }`
    UnsafeBlock,
    /// `unsafe impl`
    UnsafeImpl,
    /// `unsafe trait`
    UnsafeTrait,
    /// `unsafe extern`
    UnsafeExtern,
    /// `#[unsafe(..)]`
    UnsafeAttr,
}

impl Marker {
    fn as_str(self) -> &'static str {
        use crate::rust::marker;
        match self {
            Marker::UnsafeFn => marker::UNSAFE_FN,
            Marker::UnsafeBlock => marker::UNSAFE_BLOCK,
            Marker::UnsafeImpl => marker::UNSAFE_IMPL,
            Marker::UnsafeTrait => marker::UNSAFE_TRAIT,
            Marker::UnsafeExtern => marker::UNSAFE_EXTERN,
            Marker::UnsafeAttr => marker::UNSAFE_ATTR,
        }
    }
}

/// A rule with a name. Say which modules it applies to.
#[derive(Debug, Clone)]
pub struct Named {
    name: String,
}

impl Named {
    /// The modules that the rule constrains: [`any_of`], [`all`], [`all_except`], or
    /// `any_of(..).except(..)`.
    pub fn modules(self, modules: impl Into<Modules>) -> Scoped {
        Scoped {
            name: self.name,
            modules: modules.into(),
        }
    }

    /// The items that these modules declare: functions, types, traits, constants, statics
    /// and `macro_rules!` macros. Narrow them with the filters of [`ItemSelection`], then
    /// say what they must meet, for example with [`ItemSelection::must_reside_in`].
    pub fn items(self, modules: impl Into<Modules>) -> ItemSelection {
        ItemSelection::new(self.name, modules.into())
    }
}

/// A rule with a name and modules. Say what the modules must or must not do, for example
/// with `must_not_use`, `must_only_use` or `must_be_acyclic`.
#[derive(Debug, Clone)]
pub struct Scoped {
    name: String,
    modules: Modules,
}

impl Scoped {
    /// The modules must not refer to any of the targets, in any form: `use`, a full path in
    /// an expression, a type, a pattern, a bound, an attribute or a macro call.
    pub fn must_not_use(self, targets: impl Into<Uses>) -> Rule {
        self.rule(Constraint::MustNotUse(targets.into().targets()))
    }

    /// Every reference of the modules must lead to one of the targets. A module may always
    /// use its own items. A reference is allowed when the path in the code is allowed, or
    /// when the definition behind a re-export is.
    ///
    /// The list must name everything the modules use, also `std` and `core`.
    pub fn must_only_use(self, targets: impl Into<Uses>) -> Rule {
        self.rule(Constraint::MustOnlyUse(targets.into().targets()))
    }

    /// The modules must not form a cycle: no module may reach itself through the references
    /// between them. Each tangle of modules is reported once, with one of its cycles.
    ///
    /// In Rust, a module and its children often use each other. For cycles between larger
    /// parts, use [`Scoped::must_be_acyclic_between`].
    pub fn must_be_acyclic(self) -> Rule {
        self.rule(Constraint::MustBeAcyclic(None))
    }

    /// Like [`Scoped::must_be_acyclic`], between slices. The slice of a module is the
    /// shortest start of its path that matches one of the patterns: with `crate::*`, the
    /// module `crate::frontend::passes` is in the slice `crate::frontend`. References inside a
    /// slice do not count, and a module without a slice is left out.
    pub fn must_be_acyclic_between(self, slices: AnyOf) -> Rule {
        self.rule(Constraint::MustBeAcyclic(Some(slices.split())))
    }

    /// No module of one slice may refer to a module of another slice. Slices are as in
    /// [`Scoped::must_be_acyclic_between`]: with `crate::features::*`, no feature may use
    /// another feature.
    ///
    /// A module that the rule does not select is in no slice. Leave a shared part out with
    /// `all_except(["crate::features::shared::**"])`.
    pub fn must_be_independent_between(self, slices: AnyOf) -> Rule {
        self.rule(Constraint::MustBeIndependent(slices.split()))
    }

    /// Each module must be used: code outside the module and outside the modules below it
    /// must refer to it, or to a module below it. A `mod` declaration is not a use.
    ///
    /// The crate root is never used, so leave it out: `all_except(["crate"])`.
    pub fn must_be_used(self) -> Rule {
        self.rule(Constraint::MustBeUsed)
    }

    /// No chain of references may lead from the modules to one of the targets: not the
    /// modules themselves, and not a module that they use, at any distance. The report
    /// shows the shortest chain.
    pub fn must_not_reach(self, targets: impl Into<Uses>) -> Rule {
        self.rule(Constraint::MustNotReach(targets.into().targets()))
    }

    /// A chain of references must lead from one of the root modules to each module, or to
    /// a module below it. Give the entry points as roots: `any_of(["crate"])`.
    pub fn must_be_reachable_from(self, roots: AnyOf) -> Rule {
        self.rule(Constraint::MustBeReachableFrom(roots.split()))
    }

    /// Only the users, and the modules of the rule themselves, may refer to the modules of
    /// the rule. Every other module that refers to one of them breaks the rule, also through
    /// a re-export.
    ///
    /// Users match modules exactly, as the modules of a rule do: `crate::api::**` is the api
    /// and all modules below it.
    pub fn must_only_be_used_by(self, users: AnyOf) -> Rule {
        self.rule(Constraint::MustOnlyBeUsedBy(users.split()))
    }

    /// Each module must be one that a place matches exactly. The layers use it for
    /// [`Layers::every_module_in_a_layer`].
    pub(crate) fn must_be_in<'a>(self, places: impl IntoIterator<Item = &'a String>) -> Rule {
        let places = places.into_iter().map(|pattern| split(pattern)).collect();
        self.rule(Constraint::MustBeIn(places))
    }

    /// Each module must have at most this number of lines.
    ///
    /// The lines of a module are the lines of its file, with the blank lines, the comments
    /// and the inline modules of the file. The lines of an inline module are those of its
    /// declaration. The files of the modules below it do not count.
    ///
    /// ```
    /// use refpact::{all, rule};
    /// rule("no module is longer than 500 lines")
    ///     .modules(all())
    ///     .must_not_exceed_lines(500);
    /// ```
    pub fn must_not_exceed_lines(self, lines: u32) -> Rule {
        self.rule(Constraint::MustNotExceedLines(lines))
    }

    /// The modules must not contain any of the markers, for example [`unsafe_code`].
    pub fn must_not_contain(self, markers: impl IntoIterator<Item = Marker>) -> Rule {
        let markers = markers
            .into_iter()
            .map(|marker| marker.as_str().to_owned())
            .collect();
        self.rule(Constraint::MustNotContain(markers))
    }

    /// The modules must pass a check of your own: a closure that gets the [`Scope`] of the
    /// rule and returns the violations. The rule selects the modules, applies its filters and
    /// its exemptions, and reports and baselines the violations, as for the rules above.
    ///
    /// ```
    /// use refpact::{all, rule, Crate, Detail, Scope, Violation};
    ///
    /// let app = Crate::from_memory("src/lib.rs", &[("src/lib.rs", "use std::fs;\nuse std::io;\n")]).unwrap();
    /// let failure = rule("a module refers to at most one path")
    ///     .modules(all())
    ///     .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
    ///         scope
    ///             .subjects()
    ///             .filter(|&unit| scope.references().filter(|seen| seen.unit == unit).count() > 1)
    ///             .map(|unit| scope.violation_of_unit(unit, Detail::custom("refers to more than one path")))
    ///             .collect()
    ///     })
    ///     .verify(&app)
    ///     .unwrap_err();
    /// assert!(failure.to_string().contains("crate refers to more than one path"));
    /// ```
    ///
    /// A violation with [`Detail::custom`] prints its message after the module. Any other
    /// [`Detail`] prints as the rules above print it.
    pub fn must_satisfy<F>(self, check: F) -> Rule
    where
        F: Fn(&Scope<'_>) -> Vec<Violation> + Send + Sync + 'static,
    {
        self.must_meet(check)
    }

    /// Like [`Scoped::must_satisfy`], with a [`Check`] that reads its own arguments first,
    /// so that a pattern that matches nothing is an error, as it is for the rules above.
    pub fn must_meet(self, check: impl Check + Send + Sync + 'static) -> Rule {
        self.rule(Constraint::Custom(crate::core::Custom::new(check)))
    }

    fn rule(self, constraint: Constraint) -> Rule {
        Rule {
            rule: crate::core::Rule::new(self.name, subjects(self.modules), constraint),
        }
    }
}

/// These modules as a selection of the core: the subjects of a rule, or the modules of an
/// exemption.
pub(crate) fn subjects(modules: Modules) -> Subjects {
    let patterns = |list: Vec<String>| list.iter().map(|pattern| split(pattern)).collect();
    Subjects {
        include: match modules.include {
            None => Include::All,
            Some(list) => Include::AnyOf(patterns(list)),
        },
        except: modules.except.map(patterns),
    }
}

fn split(pattern: &str) -> Vec<String> {
    crate::core::split(pattern, "::")
}

/// A complete rule: a plain value that can be checked against a crate.
#[derive(Debug, Clone)]
pub struct Rule {
    rule: crate::core::Rule,
}

impl Filtered for Rule {
    fn filter(mut self, filter: Filter) -> Rule {
        filter.apply(&mut self.rule);
        self
    }
}

filter_methods!(Rule);

impl Rule {
    /// Whether the violations of the rule fail the check.
    fn is_warning(&self) -> bool {
        self.rule.severity == crate::core::Severity::Warning
    }

    /// Writes the report of the violations of a warning to stderr.
    fn warn(&self, violations: &[Violation]) {
        eprintln!(
            "{}",
            crate::core::render_violations(
                &self.rule.name,
                self.rule.reason.as_deref(),
                crate::core::Severity::Warning,
                violations,
                &crate::core::PathFormat::plain("::"),
            )
        );
    }

    /// Evaluates the rule. `Ok` holds the violations, which may be none.
    pub fn evaluate(&self, krate: &Crate) -> Result<Vec<Violation>, Vec<RuleError>> {
        krate.prepared().evaluate(&self.rule)
    }

    /// A diagram of what the rule sees, in Mermaid, DOT or Markdown. The nodes are the modules of the
    /// rule, or its slices when the rule is about slices, such as
    /// [`Scoped::must_be_acyclic_between`]. An edge is a reference between two nodes that
    /// the filters and the exemptions of the rule let pass. The modules and references of a
    /// cycle are red. The constraint of the rule is not evaluated, so a rule of any kind
    /// draws its modules.
    ///
    /// A Markdown viewer that knows Mermaid, such as GitHub's, draws the text in a block
    /// ` ```mermaid `.
    ///
    /// ```
    /// use refpact::{all, rule, Crate, DiagramFormat};
    ///
    /// let shop = Crate::from_memory(
    ///     "src/lib.rs",
    ///     &[
    ///         ("src/lib.rs", "pub mod api;\npub mod db;\n"),
    ///         ("src/api.rs", "pub fn get() { crate::db::query(); }\n"),
    ///         ("src/db.rs", "pub fn query() {}\n"),
    ///     ],
    /// )
    /// .unwrap();
    /// let diagram = rule("the modules")
    ///     .modules(all())
    ///     .must_be_acyclic()
    ///     .diagram(&shop, DiagramFormat::Mermaid)
    ///     .unwrap();
    /// assert!(diagram.contains("n1 --> n2"));
    /// ```
    pub fn diagram(&self, krate: &Crate, format: DiagramFormat) -> Result<String, Failure> {
        let nodes = match &self.rule.constraint {
            Constraint::MustBeAcyclic(Some(slices)) | Constraint::MustBeIndependent(slices) => {
                DiagramNodes::Slices(slices)
            }
            _ => DiagramNodes::Subjects,
        };
        self.draw(krate, nodes, format)
    }

    /// Like [`Rule::check_diagram`], but the failure is a value.
    pub fn verify_diagram(
        &self,
        krate: &Crate,
        file: impl AsRef<std::path::Path>,
    ) -> Result<(), Failure> {
        let draw = |format| self.diagram(krate, format);
        diagram::verify(&self.rule.name, file.as_ref(), draw)
    }

    /// Fails the test when the file does not hold the [diagram](Rule::diagram) of the rule,
    /// so a diagram that is committed with the code stays current.
    ///
    /// The name of the file gives the format: `.md` is a Markdown file with the Mermaid
    /// diagram in a block ` ```mermaid `, `.mmd` and `.mermaid` are Mermaid, `.dot` and `.gv`
    /// are DOT. The diagram is the whole file. When the environment variable
    /// `REFPACT_DIAGRAM` is `update`, this writes the diagram to the file and passes.
    ///
    /// In a test, use `concat!(env!("CARGO_MANIFEST_DIR"), "/docs/modules.md")`.
    #[track_caller]
    pub fn check_diagram(&self, krate: &Crate, file: impl AsRef<std::path::Path>) {
        if let Err(failure) = self.verify_diagram(krate, file) {
            panic!("{failure}");
        }
    }

    /// The diagram of the rule with these nodes.
    fn draw(
        &self,
        krate: &Crate,
        nodes: DiagramNodes<'_>,
        format: DiagramFormat,
    ) -> Result<String, Failure> {
        krate
            .prepared()
            .diagram(&self.rule, nodes, format)
            .map_err(|errors| Failure::Invalid {
                rule: self.rule.name.clone(),
                errors,
            })
    }

    /// Names the properties of references that a check of your own reads, such as
    /// [`PROPERTY_LICENSE`]. A name that the load does not produce is then an error, as a
    /// kind that it does not produce is: a check that reads `licence` for `license`, or the
    /// properties of a load without
    /// [`reading_dependencies`](crate::Options::reading_dependencies), cannot pass because
    /// it finds nothing.
    pub fn reading_properties<I, S>(mut self, names: I) -> Rule
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let names = names.into_iter().map(Into::into);
        self.rule.properties.extend(names);
        self
    }

    /// Evaluates the rule and returns the failure, if any, as a value that prints the same
    /// report as [`Rule::check`]. The violations of a warning are written to stderr and are
    /// no failure: see [`Rule::as_warning`].
    pub fn verify(&self, krate: &Crate) -> Result<(), Failure> {
        match self.evaluate(krate) {
            Ok(violations) if violations.is_empty() => Ok(()),
            Ok(violations) if self.is_warning() => {
                self.warn(&violations);
                Ok(())
            }
            Ok(violations) => Err(Failure::Violated {
                rule: self.rule.name.clone(),
                reason: self.rule.reason.clone(),
                violations,
            }),
            Err(errors) => Err(Failure::Invalid {
                rule: self.rule.name.clone(),
                errors,
            }),
        }
    }

    /// Fails the test when the rule is violated or cannot be evaluated.
    #[track_caller]
    pub fn check(&self, krate: &Crate) {
        if let Err(failure) = self.verify(krate) {
            panic!("{failure}");
        }
    }

    /// Like [`Rule::verify`], but violations that the baseline file accepts do not fail.
    ///
    /// A baseline is used only through this method and [`Rule::check_with_baseline`]. It can
    /// only shrink: an entry that accepts more violations than the code has is a failure.
    /// When the environment variable `REFPACT_BASELINE` is `update`, this writes the
    /// violations of the rule to the file and passes.
    pub fn verify_with_baseline(&self, krate: &Crate, baseline: &Baseline) -> Result<(), Failure> {
        if baseline::update_requested() {
            return self.write_baseline(krate, baseline);
        }
        let violations = self.violations_for_baseline(krate)?;
        let applied = baseline.apply(&self.rule.name, violations)?;
        if !applied.remaining.is_empty() && self.is_warning() {
            self.warn(&applied.remaining);
        } else if !applied.remaining.is_empty() {
            return Err(Failure::Violated {
                rule: self.rule.name.clone(),
                reason: self.rule.reason.clone(),
                violations: applied.remaining,
            });
        }
        if !applied.stale.is_empty() {
            return Err(Failure::StaleBaseline {
                rule: self.rule.name.clone(),
                file: baseline.display(),
                entries: applied.stale,
            });
        }
        Ok(())
    }

    /// Fails the test when the rule has violations that the baseline file does not accept,
    /// when the baseline has stale entries, or when the rule cannot be evaluated.
    #[track_caller]
    pub fn check_with_baseline(&self, krate: &Crate, baseline: &Baseline) {
        if let Err(failure) = self.verify_with_baseline(krate, baseline) {
            panic!("{failure}");
        }
    }

    /// Writes the current violations of this rule to the baseline file. Entries of other
    /// rules in the file are kept.
    pub fn write_baseline(&self, krate: &Crate, baseline: &Baseline) -> Result<(), Failure> {
        let violations = self.violations_for_baseline(krate)?;
        baseline.write(&self.rule.name, &violations)
    }

    fn violations_for_baseline(&self, krate: &Crate) -> Result<Vec<Violation>, Failure> {
        if self.rule.name.contains(['\t', '\n', '\r']) {
            return Err(Failure::Baseline {
                message: format!(
                    "the name of rule `{}` has a tab or a line break, so it cannot be in a baseline",
                    self.rule.name.escape_debug()
                ),
            });
        }
        self.evaluate(krate).map_err(|errors| Failure::Invalid {
            rule: self.rule.name.clone(),
            errors,
        })
    }

    /// The rule as the core sees it.
    pub fn as_core(&self) -> &crate::core::Rule {
        &self.rule
    }
}

/// Why a rule did not pass.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// The code breaks the rule.
    Violated {
        /// The name of the rule.
        rule: String,
        /// Why the rule exists, when it says so. See [`Rule::because`].
        reason: Option<String>,
        /// The violations that no baseline accepts.
        violations: Vec<Violation>,
    },
    /// The rule cannot fail as written, for example because a pattern matches nothing.
    Invalid {
        /// The name of the rule.
        rule: String,
        /// Every reason, in the order of the specification.
        errors: Vec<RuleError>,
    },
    /// The baseline accepts violations that the code no longer has.
    StaleBaseline {
        /// The name of the rule.
        rule: String,
        /// The baseline file.
        file: String,
        /// The entries that accept too much.
        entries: Vec<StaleEntry>,
    },
    /// The baseline file cannot be read or written.
    Baseline {
        /// What went wrong.
        message: String,
    },
    /// The diagram file does not hold the diagram of the rule. See [`Rule::check_diagram`].
    StaleDiagram {
        /// The name of the rule.
        rule: String,
        /// The diagram file.
        file: String,
    },
    /// The diagram file cannot be read or written, or its name gives no format.
    Diagram {
        /// What went wrong.
        message: String,
    },
    /// A group of rules cannot fail as written, for example layers with one layer only.
    Malformed {
        /// The name of the group.
        rule: String,
        /// What is wrong with it.
        message: String,
    },
    /// Several rules of one group did not pass.
    Several(Vec<Failure>),
}

impl fmt::Display for Failure {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Failure::Violated {
                rule,
                reason,
                violations,
            } => f.write_str(&crate::core::render_violations(
                rule,
                reason.as_deref(),
                crate::core::Severity::Error,
                violations,
                &crate::core::PathFormat::plain("::"),
            )),
            Failure::Invalid { rule, errors } => {
                f.write_str(&crate::core::render_errors(rule, errors))
            }
            Failure::StaleBaseline {
                rule,
                file,
                entries,
            } => f.write_str(&crate::core::render_stale(rule, file, entries)),
            Failure::StaleDiagram { rule, file } => {
                f.write_str(&crate::core::render_stale_diagram(rule, file))
            }
            Failure::Baseline { message } | Failure::Diagram { message } => f.write_str(message),
            Failure::Malformed { rule, message } => {
                write!(f, "rule `{rule}` cannot be checked:\n  {message}")
            }
            Failure::Several(failures) => {
                let texts: Vec<String> = failures.iter().map(Failure::to_string).collect();
                f.write_str(&texts.join("\n"))
            }
        }
    }
}

impl std::error::Error for Failure {}
