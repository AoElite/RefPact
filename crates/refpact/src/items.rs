//! Rules about items: where they are, what they are named, and what they implement or derive.

use crate::core::{Constraint, CustomItems, Demand, Items, Related};
use crate::rust::items::{item_kind, modifier, relation, visibility};
use crate::{split, subjects, AnyOf, ItemCheck, ItemScope, Modules, Rule, Uses, Violation};

/// A kind of item, for [`ItemSelection::of_kind`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemKind {
    /// `fn`
    Fn,
    /// `struct`
    Struct,
    /// `enum`
    Enum,
    /// `union`
    Union,
    /// `trait`
    Trait,
    /// `type`, an alias
    Type,
    /// `const`
    Const,
    /// `static`
    Static,
    /// `macro_rules!`
    Macro,
    /// A field of a struct or a union, at `Type::field`, or `Type::0` for a tuple struct.
    Field,
    /// A function of an inherent `impl` or of a trait, at `Type::name` or `Trait::name`. A
    /// function of a trait is as visible as the trait. The functions of an
    /// `impl Trait for Type` are no methods: the trait fixes them.
    Method,
}

impl ItemKind {
    fn as_str(self) -> &'static str {
        match self {
            ItemKind::Fn => item_kind::FN,
            ItemKind::Struct => item_kind::STRUCT,
            ItemKind::Enum => item_kind::ENUM,
            ItemKind::Union => item_kind::UNION,
            ItemKind::Trait => item_kind::TRAIT,
            ItemKind::Type => item_kind::TYPE,
            ItemKind::Const => item_kind::CONST,
            ItemKind::Static => item_kind::STATIC,
            ItemKind::Macro => item_kind::MACRO,
            ItemKind::Field => item_kind::FIELD,
            ItemKind::Method => item_kind::METHOD,
        }
    }
}

/// The visibility of an item, for [`ItemSelection::with_visibility`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
    /// `pub`, and a `macro_rules!` macro with `#[macro_export]`.
    Pub,
    /// `pub(crate)`
    PubCrate,
    /// `pub(super)` and `pub(in path)`
    PubRestricted,
    /// No `pub`, and `pub(self)`.
    Private,
}

impl Visibility {
    fn as_str(self) -> &'static str {
        match self {
            Visibility::Pub => visibility::PUB,
            Visibility::PubCrate => visibility::PUB_CRATE,
            Visibility::PubRestricted => visibility::PUB_RESTRICTED,
            Visibility::Private => visibility::PRIVATE,
        }
    }
}

/// A modifier of an item, for [`ItemSelection::with_modifier`] and
/// [`ItemSelection::must_have_modifier`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Modifier {
    /// `async fn`
    Async,
    /// `const fn`
    Const,
    /// `unsafe fn` and `unsafe trait`
    Unsafe,
    /// A function with an ABI: `extern "C" fn`.
    Extern,
    /// `static mut`
    Mut,
    /// A doc comment (`///`) or a `#[doc = ".."]` on the item. `//!` inside a module file
    /// documents the module, which is no item.
    Documented,
    /// `#[doc(hidden)]`
    Hidden,
}

impl Modifier {
    fn as_str(self) -> &'static str {
        match self {
            Modifier::Async => modifier::ASYNC,
            Modifier::Const => modifier::CONST,
            Modifier::Unsafe => modifier::UNSAFE,
            Modifier::Extern => modifier::EXTERN,
            Modifier::Mut => modifier::MUT,
            Modifier::Documented => modifier::DOCUMENTED,
            Modifier::Hidden => modifier::HIDDEN,
        }
    }
}

fn modifier_names(modifiers: impl IntoIterator<Item = Modifier>) -> Vec<String> {
    modifiers
        .into_iter()
        .map(|modifier| modifier.as_str().to_owned())
        .collect()
}

/// Items of some modules, narrowed by filters. Each filter narrows the selection; the values
/// that one filter takes form a union. End with a `must_..` method.
///
/// ```no_run
/// use refpact::{all, any_of, rule, Crate};
///
/// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
///
/// rule("repositories live in the storage")
///     .items(all())
///     .implementing(any_of(["crate::domain::Repository"]))
///     .must_reside_in(any_of(["crate::storage::**"]))
///     .check(&app);
/// ```
#[derive(Debug, Clone)]
pub struct ItemSelection {
    name: String,
    modules: Modules,
    named: Option<Vec<String>>,
    kinds: Option<Vec<String>>,
    visibilities: Option<Vec<String>>,
    related: Vec<Related>,
    modifiers: Option<Vec<String>>,
}

impl ItemSelection {
    pub(crate) fn new(name: String, modules: Modules) -> ItemSelection {
        ItemSelection {
            name,
            modules,
            named: None,
            kinds: None,
            visibilities: None,
            related: Vec::new(),
            modifiers: None,
        }
    }

    /// Only items whose name one of the globs matches. `*` matches any run of characters:
    /// `*Repository`, `test_*`. A glob that matches no item of the crate is an error.
    pub fn named<'a>(mut self, globs: impl IntoIterator<Item = &'a str>) -> ItemSelection {
        self.named
            .get_or_insert_with(Vec::new)
            .extend(globs.into_iter().map(str::to_owned));
        self
    }

    /// Only items of these kinds. Without this filter, a selection holds every kind except
    /// [`ItemKind::Field`] and [`ItemKind::Method`]: the members of the items are selected
    /// only when this filter names them.
    pub fn of_kind(mut self, kinds: impl IntoIterator<Item = ItemKind>) -> ItemSelection {
        self.kinds
            .get_or_insert_with(Vec::new)
            .extend(kinds.into_iter().map(|kind| kind.as_str().to_owned()));
        self
    }

    /// Only items with one of these visibilities, as written: a `pub` item in a private
    /// module is `pub`.
    pub fn with_visibility(
        mut self,
        visibilities: impl IntoIterator<Item = Visibility>,
    ) -> ItemSelection {
        self.visibilities.get_or_insert_with(Vec::new).extend(
            visibilities
                .into_iter()
                .map(|visibility| visibility.as_str().to_owned()),
        );
        self
    }

    /// Only types that implement one of these traits in an `impl Trait for Type` of the
    /// crate. A trait pattern that no `impl` of the crate names is an error.
    pub fn implementing(self, traits: AnyOf) -> ItemSelection {
        self.related(relation::IMPLEMENTS, traits)
    }

    /// Only items that derive one of these: `#[derive(Serialize)]` derives
    /// `serde::Serialize`, and `#[derive(Debug)]` derives `std::fmt::Debug`.
    pub fn deriving(self, derives: AnyOf) -> ItemSelection {
        self.related(relation::DERIVES, derives)
    }

    /// Only items with one of these attributes: a built-in attribute by its name, such as
    /// `non_exhaustive`, and any other by its path, such as `tokio::main`.
    pub fn with_attribute(self, attributes: AnyOf) -> ItemSelection {
        self.related(relation::ATTRIBUTE, attributes)
    }

    /// Only items that other crates can name: `pub` items reached from the crate root
    /// through `pub mod` and `pub use`. A `pub` item in a private module is not exported
    /// unless a `pub use` names it.
    pub fn exported(mut self) -> ItemSelection {
        self.related.push(anywhere(relation::EXPORTED));
        self
    }

    /// Only functions and methods with one of these paths in their return type:
    /// `-> Result<Order, anyhow::Error>` returns `std::result::Result`, `crate::Order` and
    /// `anyhow::Error`. A pattern covers the paths inside it, as a target does.
    pub fn returning(self, types: AnyOf) -> ItemSelection {
        self.related(relation::RETURNS, types)
    }

    /// Only functions and methods with one of these paths in the type of a parameter. The
    /// `self` parameter does not count.
    pub fn taking(self, types: AnyOf) -> ItemSelection {
        self.related(relation::TAKES, types)
    }

    /// Only fields with one of these paths in their type.
    pub fn of_type(self, types: AnyOf) -> ItemSelection {
        self.related(relation::HAS_TYPE, types)
    }

    /// Only items with one of these modifiers: `with_modifier([Modifier::Async])` selects the
    /// `async` functions and methods.
    pub fn with_modifier(mut self, modifiers: impl IntoIterator<Item = Modifier>) -> ItemSelection {
        self.modifiers
            .get_or_insert_with(Vec::new)
            .extend(modifier_names(modifiers));
        self
    }

    fn related(mut self, kind: &str, targets: AnyOf) -> ItemSelection {
        self.related.push(relate(kind, targets));
        self
    }

    /// Each item must have one of these modifiers: "every `pub` item is documented".
    ///
    /// ```no_run
    /// use refpact::{all, rule, Crate, Modifier, Visibility};
    ///
    /// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
    ///
    /// rule("the public API is documented")
    ///     .items(all())
    ///     .with_visibility([Visibility::Pub])
    ///     .must_have_modifier([Modifier::Documented])
    ///     .check(&app);
    /// ```
    pub fn must_have_modifier(self, modifiers: impl IntoIterator<Item = Modifier>) -> Rule {
        let modifiers = modifier_names(modifiers);
        self.rule(Demand::HaveModifier(modifiers))
    }

    /// No item may be selected. Use this to forbid items: "nothing in `crate::internal` is
    /// `pub`".
    pub fn must_not_exist(self) -> Rule {
        self.rule(Demand::NotExist)
    }

    /// Each item must be declared in one of these modules. They match exactly, as the
    /// modules of a rule do.
    pub fn must_reside_in(self, modules: AnyOf) -> Rule {
        let places = modules
            .patterns
            .iter()
            .map(|pattern| split(pattern))
            .collect();
        self.rule(Demand::ResideIn(places))
    }

    /// Each item must have at most this number of lines, from the line of its name to the
    /// line of its last token. The attributes and the comments above the name do not count.
    ///
    /// ```
    /// use refpact::{all, rule, ItemKind};
    /// rule("no function is longer than 60 lines")
    ///     .items(all())
    ///     .of_kind([ItemKind::Fn, ItemKind::Method])
    ///     .must_not_exceed_lines(60);
    /// ```
    pub fn must_not_exceed_lines(self, lines: u32) -> Rule {
        self.rule(Demand::NotExceedLines(lines))
    }

    /// The name of each item must match one of the globs.
    pub fn must_be_named<'a>(self, globs: impl IntoIterator<Item = &'a str>) -> Rule {
        let globs = globs.into_iter().map(str::to_owned).collect();
        self.rule(Demand::BeNamed(globs))
    }

    /// Each item must implement one of the traits.
    pub fn must_implement(self, traits: AnyOf) -> Rule {
        self.rule(Demand::Have(relate(relation::IMPLEMENTS, traits)))
    }

    /// Each item must derive one of these.
    pub fn must_derive(self, derives: AnyOf) -> Rule {
        self.rule(Demand::Have(relate(relation::DERIVES, derives)))
    }

    /// Each item must have one of these attributes.
    pub fn must_have_attribute(self, attributes: AnyOf) -> Rule {
        self.rule(Demand::Have(relate(relation::ATTRIBUTE, attributes)))
    }

    /// Each item must be exported: other crates can name it. See [`ItemSelection::exported`].
    pub fn must_be_exported(self) -> Rule {
        self.rule(Demand::Have(anywhere(relation::EXPORTED)))
    }

    /// The code of each item must not refer to any of the targets. The code of an item is
    /// its signature, its body, its fields and the `impl` blocks of a type, with their
    /// methods. A `use` at the top of a module is in no item, so the rule sees the paths
    /// that the code inside the items writes.
    ///
    /// ```no_run
    /// use refpact::{all, any_of, rule, Crate};
    ///
    /// let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
    ///
    /// rule("what is sent over the wire knows no database")
    ///     .items(all())
    ///     .deriving(any_of(["serde::Serialize"]))
    ///     .must_not_use(any_of(["crate::db"]))
    ///     .check(&app);
    /// ```
    pub fn must_not_use(self, targets: impl Into<Uses>) -> Rule {
        self.rule(Demand::NotUse(targets.into().targets()))
    }

    /// Every reference in the code of each item must lead to one of the targets, or stay
    /// in the module of the reference. See [`ItemSelection::must_not_use`] for what the code
    /// of an item is.
    pub fn must_only_use(self, targets: impl Into<Uses>) -> Rule {
        self.rule(Demand::OnlyUse(targets.into().targets()))
    }

    /// The items must pass a check of your own: a closure that gets the [`ItemScope`] of
    /// the rule and returns the violations. The scope has the items that the filters above
    /// select, the references inside them, and all that the [`Scope`](crate::Scope) of a
    /// rule about modules has. The rule applies its filters and its exemptions, and
    /// reports and baselines the violations, as for the demands above.
    ///
    /// ```
    /// use refpact::{all, rule, Crate, Detail, ItemKind, ItemScope, Violation};
    ///
    /// let app = Crate::from_memory("src/lib.rs", &[("src/lib.rs", "pub fn a() {}\npub fn get_b() {}\n")]).unwrap();
    /// let failure = rule("no getter has a prefix")
    ///     .items(all())
    ///     .of_kind([ItemKind::Fn])
    ///     .must_satisfy(|items: &ItemScope<'_>| -> Vec<Violation> {
    ///         items
    ///             .items()
    ///             .filter(|item| item.name.starts_with("get_"))
    ///             .map(|item| items.violation_of_item(item, Detail::custom(format!("has the getter {}", item.name))))
    ///             .collect()
    ///     })
    ///     .verify(&app)
    ///     .unwrap_err();
    /// assert!(failure.to_string().contains("crate has the getter get_b"));
    /// ```
    pub fn must_satisfy<F>(self, check: F) -> Rule
    where
        F: Fn(&ItemScope<'_>) -> Vec<Violation> + Send + Sync + 'static,
    {
        self.must_meet(check)
    }

    /// Like [`ItemSelection::must_satisfy`], with an [`ItemCheck`] that reads its own
    /// arguments first, so that a pattern that matches nothing is an error, as it is for
    /// the demands above.
    pub fn must_meet(self, check: impl ItemCheck + Send + Sync + 'static) -> Rule {
        self.rule(Demand::Custom(CustomItems::new(check)))
    }

    fn rule(self, must: Demand) -> Rule {
        // Members are selected only when the kinds name them.
        let kinds = self.kinds.unwrap_or_else(|| {
            item_kind::ALL
                .iter()
                .filter(|kind| ![item_kind::FIELD, item_kind::METHOD].contains(kind))
                .map(|kind| (*kind).to_owned())
                .collect()
        });
        let items = Items {
            named: self.named,
            kinds: Some(kinds),
            visibilities: self.visibilities,
            related: self.related,
            modifiers: self.modifiers,
            must,
        };
        Rule {
            rule: crate::core::Rule::new(
                self.name,
                subjects(self.modules),
                Constraint::Items(items),
            ),
        }
    }
}

/// A relation to any path.
fn anywhere(kind: &str) -> Related {
    Related {
        relation: kind.to_owned(),
        any_of: vec![vec!["**".to_owned()]],
    }
}

fn relate(kind: &str, targets: AnyOf) -> Related {
    Related {
        relation: kind.to_owned(),
        any_of: targets
            .patterns
            .iter()
            .map(|pattern| split(pattern))
            .collect(),
    }
}
