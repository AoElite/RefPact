//! Layers: rules about which layers of an architecture may use which.

use crate::core::{DiagramLayer, DiagramNodes};
use crate::filters::{filter_methods, Filter, Filtered};
use crate::group::rule_set_methods;
use crate::{all, rule, AnyOf, Crate, DiagramFormat, Failure, Modules, Rule, RuleSet};

/// Starts a set of layers. Add the layers from the top to the bottom.
///
/// A layer may use the layers below it. It must not use a layer above it.
///
/// The layers have the filters of a [`Rule`], such as `except_in_test_code()`,
/// `ignoring_kinds(..)` and `as_warning()`. A filter applies to every rule of the layers and
/// to their diagram.
///
/// ```no_run
/// use refpact::{all_except, any_of, layers, Crate};
///
/// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
///
/// layers("the layers of the shop")
///     .layer("api", any_of(["crate::api::**"]))
///     .layer("domain", any_of(["crate::domain::**"]))
///     .layer("storage", any_of(["crate::storage::**"]))
///     .may_only_be_used_by("storage", ["domain"])
///     .every_module_in_a_layer(all_except(["crate"]))
///     .check(&app);
/// ```
pub fn layers(name: impl Into<String>) -> Layers {
    Layers {
        name: name.into(),
        layers: Vec::new(),
        filters: Vec::new(),
        strict: false,
        may_only_use: Vec::new(),
        may_only_be_used_by: Vec::new(),
        covered: None,
    }
}

/// Layers from the top to the bottom. See [`layers`].
#[derive(Debug, Clone)]
pub struct Layers {
    name: String,
    layers: Vec<(String, AnyOf)>,
    /// The filters of every rule of the layers.
    filters: Vec<Filter>,
    strict: bool,
    /// For a layer, the only layers it may use.
    may_only_use: Vec<(String, Vec<String>)>,
    /// For a layer, the only layers that may use it.
    may_only_be_used_by: Vec<(String, Vec<String>)>,
    /// The modules that must each be in a layer.
    covered: Option<Modules>,
}

impl Layers {
    /// Adds a layer below the layers that are there.
    pub fn layer(mut self, name: impl Into<String>, modules: AnyOf) -> Layers {
        self.layers.push((name.into(), modules));
        self
    }

    /// The rule with the filters of the layers. A rule that looks at no reference gets only
    /// the filters that are not about references.
    fn filtered(&self, made: Rule, sees_references: bool) -> Rule {
        let filters = self.filters.iter().cloned();
        filters
            .filter(|filter| sees_references || !filter.is_about_references())
            .fold(made, Filtered::filter)
    }

    /// Makes the layers strict: a layer may use only the layer right below it, not the
    /// layers further down.
    pub fn strict(mut self) -> Layers {
        self.strict = true;
        self
    }

    /// The layer may use only these layers. Each of them must be below it, and right below
    /// it when the layers are [strict](Layers::strict). An empty list forbids the layer to
    /// use any other layer.
    pub fn may_only_use<'a>(
        mut self,
        layer: &str,
        layers: impl IntoIterator<Item = &'a str>,
    ) -> Layers {
        let layers = layers.into_iter().map(str::to_owned).collect();
        self.may_only_use.push((layer.to_owned(), layers));
        self
    }

    /// Only these layers may use the layer. Each of them must be above it, and right above
    /// it when the layers are [strict](Layers::strict). An empty list forbids every other
    /// layer to use it.
    ///
    /// This is about the layers only. To keep every other module away as well, add a rule
    /// with [`Scoped::must_only_be_used_by`](crate::Scoped::must_only_be_used_by).
    pub fn may_only_be_used_by<'a>(
        mut self,
        layer: &str,
        layers: impl IntoIterator<Item = &'a str>,
    ) -> Layers {
        let layers = layers.into_iter().map(str::to_owned).collect();
        self.may_only_be_used_by.push((layer.to_owned(), layers));
        self
    }

    /// Each of these modules must be in a layer. A new module then cannot stay out of the
    /// architecture unseen. Leave out the modules that belong to no layer, such as the crate
    /// root: `all_except(["crate"])`.
    pub fn every_module_in_a_layer(mut self, modules: impl Into<Modules>) -> Layers {
        self.covered = Some(modules.into());
        self
    }

    /// The rules of the layers: one `must_not_use` rule for each layer that must not use
    /// some other layers, and the rule of [`Layers::every_module_in_a_layer`] when it is
    /// given.
    ///
    /// The rule of a layer has the name of the layers and the name of the layer. It is
    /// `<layers>: <layer> does not use the layers above it` when only the order of the layers
    /// applies to the layer, and `<layers>: <layer> uses only the layers it may use` when
    /// more applies.
    ///
    /// The error is [`Failure::Malformed`] when the layers cannot fail as written or name a
    /// layer that does not exist.
    pub fn rules(&self) -> Result<Vec<Rule>, Failure> {
        let allowed = self.allowed().map_err(|problems| Failure::Malformed {
            rule: self.name.clone(),
            message: problems.join("\n  "),
        })?;
        let mut rules = Vec::new();
        for (from, (layer, modules)) in self.layers.iter().enumerate() {
            let forbidden = forbidden(&allowed, from);
            if forbidden.is_empty() {
                continue;
            }
            let name = if forbidden.iter().copied().eq(0..from) {
                format!("{}: {layer} does not use the layers above it", self.name)
            } else {
                format!("{}: {layer} uses only the layers it may use", self.name)
            };
            let targets = forbidden
                .iter()
                .flat_map(|&to| self.layers[to].1.patterns.iter());
            let made = rule(name)
                .modules(modules.clone())
                .must_not_use(crate::any_of(targets));
            rules.push(self.filtered(made, true));
        }
        if let Some(modules) = &self.covered {
            let places = self.layers.iter().flat_map(|(_, layer)| &layer.patterns);
            let made = rule(format!("{}: every module is in a layer", self.name))
                .modules(modules.clone())
                .must_be_in(places);
            rules.push(self.filtered(made, false));
        }
        Ok(rules)
    }

    /// A diagram of the layers, in Mermaid, DOT or Markdown, from the top to the bottom. Each layer is
    /// a node, and an edge is a use between two layers. A use that the layers forbid is red.
    /// A module is in the first layer that matches it, and a module in no layer is not drawn.
    /// The modules of [`Layers::every_module_in_a_layer`] are not checked.
    ///
    /// A Markdown viewer that knows Mermaid, such as GitHub's, draws the text in a block
    /// ` ```mermaid `.
    ///
    /// ```
    /// use refpact::{any_of, layers, Crate, DiagramFormat};
    ///
    /// let shop = Crate::from_memory(
    ///     "src/lib.rs",
    ///     &[
    ///         ("src/lib.rs", "pub mod api;\npub mod db;\n"),
    ///         ("src/api.rs", "pub fn get() {}\n"),
    ///         ("src/db.rs", "pub fn query() { crate::api::get(); }\n"),
    ///     ],
    /// )
    /// .unwrap();
    /// let diagram = layers("the layers")
    ///     .layer("api", any_of(["crate::api"]))
    ///     .layer("db", any_of(["crate::db"]))
    ///     .diagram(&shop, DiagramFormat::Mermaid)
    ///     .unwrap();
    /// assert!(diagram.contains("n1 --> n0"));
    /// assert!(diagram.contains("linkStyle 0 stroke:#d62728"));
    /// ```
    ///
    /// The error is [`Failure::Malformed`] as for [`Layers::rules`], or [`Failure::Invalid`]
    /// when a pattern of a layer is not valid or matches no module.
    pub fn diagram(&self, krate: &Crate, format: DiagramFormat) -> Result<String, Failure> {
        let allowed = self.allowed().map_err(|problems| Failure::Malformed {
            rule: self.name.clone(),
            message: problems.join("\n  "),
        })?;
        let list: Vec<DiagramLayer> = self
            .layers
            .iter()
            .enumerate()
            .map(|(from, (name, modules))| DiagramLayer {
                name: name.clone(),
                units: modules.split(),
                must_not_use: forbidden(&allowed, from),
            })
            .collect();
        let made = rule(self.name.clone()).modules(all()).must_be_used();
        self.filtered(made, true)
            .draw(krate, DiagramNodes::Layers(&list), format)
    }

    /// Like [`Layers::check_diagram`], but the failure is a value.
    pub fn verify_diagram(
        &self,
        krate: &Crate,
        file: impl AsRef<std::path::Path>,
    ) -> Result<(), Failure> {
        let draw = |format| self.diagram(krate, format);
        crate::diagram::verify(&self.name, file.as_ref(), draw)
    }

    /// Fails the test when the file does not hold the [diagram](Layers::diagram) of the
    /// layers. See [`Rule::check_diagram`].
    #[track_caller]
    pub fn check_diagram(&self, krate: &Crate, file: impl AsRef<std::path::Path>) {
        if let Err(failure) = self.verify_diagram(krate, file) {
            panic!("{failure}");
        }
    }

    /// For each layer, by position from the top, the layers it may use. The error lists
    /// every problem of the layers.
    fn allowed(&self) -> Result<Vec<Vec<bool>>, Vec<String>> {
        let count = self.layers.len();
        if count < 2 {
            return Err(vec![
                "fewer than two layers are given, so no layer can use a layer above it".to_owned(),
            ]);
        }
        let mut problems = Vec::new();
        for (position, (name, _)) in self.layers.iter().enumerate() {
            if self.layers[..position]
                .iter()
                .any(|(other, _)| other == name)
            {
                problems.push(format!("two layers have the name `{name}`"));
            }
        }
        // What the order allows: the layers below, or only the next one when strict.
        let ordered = |from: usize, to: usize| to > from && (!self.strict || to == from + 1);
        let mut allowed: Vec<Vec<bool>> = (0..count)
            .map(|from| (0..count).map(|to| ordered(from, to)).collect())
            .collect();
        let position = |name: &str, problems: &mut Vec<String>| {
            let found = self.layers.iter().position(|(layer, _)| layer == name);
            if found.is_none() {
                problems.push(format!("`{name}` is not a layer"));
            }
            found
        };
        let mut seen = Vec::new();
        for (layer, list) in &self.may_only_use {
            let Some(from) = position(layer, &mut problems) else {
                continue;
            };
            if seen.contains(&from) {
                problems.push(format!("`{layer}` has two lists of the layers it may use"));
            }
            seen.push(from);
            let mut keep = vec![false; count];
            for name in list {
                match position(name, &mut problems) {
                    Some(to) if ordered(from, to) => keep[to] = true,
                    Some(_) => problems.push(format!(
                        "`{layer}` may only use `{name}`, which the order of the layers forbids"
                    )),
                    None => {}
                }
            }
            for (to, keep) in keep.into_iter().enumerate() {
                allowed[from][to] &= keep;
            }
        }
        seen.clear();
        for (layer, list) in &self.may_only_be_used_by {
            let Some(to) = position(layer, &mut problems) else {
                continue;
            };
            if seen.contains(&to) {
                problems.push(format!(
                    "`{layer}` has two lists of the layers that may use it"
                ));
            }
            seen.push(to);
            let mut keep = vec![false; count];
            for name in list {
                match position(name, &mut problems) {
                    Some(from) if ordered(from, to) => keep[from] = true,
                    Some(_) => problems.push(format!(
                        "`{layer}` may only be used by `{name}`, which the order of the layers forbids"
                    )),
                    None => {}
                }
            }
            for (from, keep) in keep.into_iter().enumerate() {
                allowed[from][to] &= keep;
            }
        }
        if problems.is_empty() {
            Ok(allowed)
        } else {
            Err(problems)
        }
    }
}

/// The layers that a layer must not use, by position from the top, from what each layer may
/// use.
fn forbidden(allowed: &[Vec<bool>], from: usize) -> Vec<usize> {
    (0..allowed.len())
        .filter(|&to| to != from && !allowed[from][to])
        .collect()
}

impl RuleSet for Layers {
    fn name(&self) -> &str {
        &self.name
    }

    fn rules(&self) -> Result<Vec<Rule>, Failure> {
        Layers::rules(self)
    }
}

impl Filtered for Layers {
    fn filter(mut self, filter: Filter) -> Layers {
        self.filters.push(filter);
        self
    }
}

filter_methods!(Layers);
rule_set_methods!(Layers);
