//! The onion architecture, also called the hexagonal architecture or ports and adapters:
//! layers with the domain at the centre and the adapters outside.

use crate::filters::{filter_methods, Filter, Filtered};
use crate::group::rule_set_methods;
use crate::{layers, AnyOf, Crate, DiagramFormat, Failure, Layers, Modules, Rule, RuleSet};

/// Starts an onion architecture. From the inside out, its parts are:
///
/// - the **domain models**, which use no other part;
/// - the **domain services**, which may use the domain models;
/// - the **application services**, which may use the domain;
/// - the **adapters**, such as a web interface or a database, which may use the application
///   services and the domain, but not each other.
///
/// Give the parts that the code has; at least two of them. The rules are those of
/// [`layers`], one layer for each part, and each adapter a layer of its own. The filters of a
/// [`Rule`], such as `except_in_test_code()`, apply to every part.
///
/// ```no_run
/// use refpact::{any_of, onion_architecture, Crate};
///
/// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
///
/// onion_architecture("the shop")
///     .domain_models(any_of(["crate::domain::model::**"]))
///     .domain_services(any_of(["crate::domain::service::**"]))
///     .application_services(any_of(["crate::application::**"]))
///     .adapter("web", any_of(["crate::adapter::web::**"]))
///     .adapter("database", any_of(["crate::adapter::db::**"]))
///     .check(&app);
/// ```
pub fn onion_architecture(name: impl Into<String>) -> Onion {
    Onion {
        name: name.into(),
        domain_models: None,
        domain_services: None,
        application_services: None,
        adapters: Vec::new(),
        filters: Vec::new(),
        covered: None,
    }
}

/// The parts of an onion architecture. See [`onion_architecture`].
#[derive(Debug, Clone)]
pub struct Onion {
    name: String,
    domain_models: Option<AnyOf>,
    domain_services: Option<AnyOf>,
    application_services: Option<AnyOf>,
    adapters: Vec<(String, AnyOf)>,
    /// The filters of every rule of the architecture.
    filters: Vec<Filter>,
    covered: Option<Modules>,
}

const DOMAIN_MODELS: &str = "domain models";
const DOMAIN_SERVICES: &str = "domain services";
const APPLICATION_SERVICES: &str = "application services";

impl Onion {
    /// The modules of the domain models: the centre, which uses no other part.
    pub fn domain_models(mut self, modules: AnyOf) -> Onion {
        self.domain_models = Some(modules);
        self
    }

    /// The modules of the domain services, which may use the domain models.
    pub fn domain_services(mut self, modules: AnyOf) -> Onion {
        self.domain_services = Some(modules);
        self
    }

    /// The modules of the application services, which may use the domain.
    pub fn application_services(mut self, modules: AnyOf) -> Onion {
        self.application_services = Some(modules);
        self
    }

    /// Adds an adapter. It may use the application services and the domain, and no other
    /// adapter may use it. Its layer is named `adapter <name>`.
    pub fn adapter(mut self, name: impl Into<String>, modules: AnyOf) -> Onion {
        self.adapters.push((name.into(), modules));
        self
    }

    /// Each of these modules must be in a part. See [`Layers::every_module_in_a_layer`].
    pub fn every_module_in_a_layer(mut self, modules: impl Into<Modules>) -> Onion {
        self.covered = Some(modules.into());
        self
    }

    /// The parts as layers, from the outside in.
    pub fn as_layers(&self) -> Layers {
        let mut made = layers(self.name.clone());
        let adapter = |name: &str| format!("adapter {name}");
        for (name, modules) in &self.adapters {
            made = made.layer(adapter(name), modules.clone());
        }
        let inner = [
            (APPLICATION_SERVICES, &self.application_services),
            (DOMAIN_SERVICES, &self.domain_services),
            (DOMAIN_MODELS, &self.domain_models),
        ];
        let inner: Vec<(&str, &AnyOf)> = inner
            .into_iter()
            .filter_map(|(name, modules)| Some((name, modules.as_ref()?)))
            .collect();
        for (name, modules) in &inner {
            made = made.layer(*name, (*modules).clone());
        }
        // An adapter is above the other adapters that follow it, but may not use them.
        for (name, _) in &self.adapters {
            made = made.may_only_use(&adapter(name), inner.iter().map(|(name, _)| *name));
        }
        made = self.filters.iter().cloned().fold(made, Filtered::filter);
        if let Some(modules) = &self.covered {
            made = made.every_module_in_a_layer(modules.clone());
        }
        made
    }

    /// The rules of the architecture. See [`Layers::rules`].
    pub fn rules(&self) -> Result<Vec<Rule>, Failure> {
        self.as_layers().rules()
    }

    /// A diagram of the parts as layers. See [`Layers::diagram`].
    pub fn diagram(&self, krate: &Crate, format: DiagramFormat) -> Result<String, Failure> {
        self.as_layers().diagram(krate, format)
    }

    /// Like [`Onion::check_diagram`], but the failure is a value.
    pub fn verify_diagram(
        &self,
        krate: &Crate,
        file: impl AsRef<std::path::Path>,
    ) -> Result<(), Failure> {
        self.as_layers().verify_diagram(krate, file)
    }

    /// Fails the test when the file does not hold the [diagram](Onion::diagram) of the
    /// parts. See [`Rule::check_diagram`].
    #[track_caller]
    pub fn check_diagram(&self, krate: &Crate, file: impl AsRef<std::path::Path>) {
        self.as_layers().check_diagram(krate, file);
    }
}

impl RuleSet for Onion {
    fn name(&self) -> &str {
        &self.name
    }

    fn rules(&self) -> Result<Vec<Rule>, Failure> {
        Onion::rules(self)
    }
}

impl Filtered for Onion {
    fn filter(mut self, filter: Filter) -> Onion {
        self.filters.push(filter);
        self
    }
}

filter_methods!(Onion);
rule_set_methods!(Onion);
