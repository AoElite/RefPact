//! The interface of every check of items (SPEC 4.12 and 4.15). A rule about items selects
//! declarations with its filters, and then asks the check of its demand for the violations.
//! The built-in demands are checks of this kind, and a user can write more.
//!
//! An [`ItemCheck`] runs in two steps, as a [`crate::Check`] does: it reads its arguments
//! with a [`Setup`], and the [`ItemChecker`] that it returns finds the violations in an
//! [`ItemScope`].

use std::cell::OnceCell;
use std::fmt;
use std::ops::Deref;
use std::sync::Arc;

use crate::check::{NoReferences, Scope, Seen, Setup};
use crate::index::FastSet;
use crate::model::Declaration;
use crate::violation::Violation;

/// What a rule about items demands: the arguments of a check, read before the evaluation.
///
/// A closure `Fn(&ItemScope) -> Vec<Violation>` is a check without arguments.
pub trait ItemCheck {
    /// Checks the arguments with `setup`, and returns what finds the violations. An error
    /// that `setup` records stops the evaluation before the checker runs.
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn ItemChecker + 'c>;
}

/// What finds the violations of a rule about items, after [`ItemCheck::prepare`].
///
/// A closure `Fn(&ItemScope) -> Vec<Violation>` is a checker.
pub trait ItemChecker {
    /// Whether the checker reads references. An exemption of a rule whose checker does not
    /// read them lets nothing pass, so it is an error (SPEC 4.13). Default: `true`.
    fn reads_references(&self) -> bool {
        true
    }

    /// The violations, in a stable order.
    fn violations(&self, items: &ItemScope<'_>) -> Vec<Violation>;
}

impl<F> ItemChecker for F
where
    F: Fn(&ItemScope<'_>) -> Vec<Violation>,
{
    fn violations(&self, items: &ItemScope<'_>) -> Vec<Violation> {
        self(items)
    }
}

impl<F> ItemCheck for F
where
    F: Fn(&ItemScope<'_>) -> Vec<Violation>,
{
    fn prepare<'c>(&'c self, _: &mut Setup<'_>) -> Box<dyn ItemChecker + 'c> {
        Box::new(self)
    }
}

impl<F> ItemChecker for NoReferences<F>
where
    F: Fn(&ItemScope<'_>) -> Vec<Violation>,
{
    fn reads_references(&self) -> bool {
        false
    }

    fn violations(&self, items: &ItemScope<'_>) -> Vec<Violation> {
        (self.0)(items)
    }
}

/// The check of [`crate::Demand::Custom`]. Two values are equal when they are clones of one
/// value.
#[derive(Clone)]
pub struct CustomItems {
    check: Arc<dyn ItemCheck + Send + Sync>,
}

impl CustomItems {
    /// Wraps a check, or a closure `Fn(&ItemScope) -> Vec<Violation>`.
    pub fn new(check: impl ItemCheck + Send + Sync + 'static) -> CustomItems {
        CustomItems {
            check: Arc::new(check),
        }
    }
}

impl ItemCheck for CustomItems {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn ItemChecker + 'c> {
        self.check.prepare(setup)
    }
}

impl fmt::Debug for CustomItems {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CustomItems(..)")
    }
}

impl PartialEq for CustomItems {
    fn eq(&self, other: &CustomItems) -> bool {
        Arc::ptr_eq(&self.check, &other.check)
    }
}

impl Eq for CustomItems {}

// As for `Custom`: a check reads the facts and returns violations.
impl std::panic::UnwindSafe for CustomItems {}
impl std::panic::RefUnwindSafe for CustomItems {}

/// What an [`ItemChecker`] gets: the [`Scope`] of the rule, and the declarations that the
/// filters of the rule select. Every method of the scope can be called on it.
pub struct ItemScope<'a> {
    scope: Scope<'a>,
    items: Vec<&'a Declaration>,
    /// The paths of the items, made when the references inside them are asked for.
    paths: OnceCell<FastSet<&'a [String]>>,
}

impl<'a> ItemScope<'a> {
    /// The declarations of the subjects that the rule sees and that `selects` keeps.
    pub(crate) fn new(scope: &Scope<'a>, selects: impl Fn(&Declaration) -> bool) -> Self {
        ItemScope {
            scope: *scope,
            items: scope
                .declarations()
                .filter(|declaration| selects(declaration))
                .collect(),
            paths: OnceCell::new(),
        }
    }

    /// The declarations that the rule selects, in order.
    pub fn items(&self) -> impl Iterator<Item = &'a Declaration> + '_ {
        self.items.iter().copied()
    }

    /// The references that the rule sees inside the selected declarations, in the order of
    /// the facts. The innermost item of a reference is the `item` of its fact. A reference
    /// outside every item, such as an import at the top of a unit, is not one of them.
    pub fn item_references(&self) -> impl Iterator<Item = Seen<'a>> + '_ {
        let paths = self.paths.get_or_init(|| {
            self.items
                .iter()
                .map(|declaration| declaration.path.as_slice())
                .collect()
        });
        self.scope.references().filter(move |seen| {
            seen.fact
                .item
                .as_ref()
                .is_some_and(|item| (1..=item.len()).any(|length| paths.contains(&item[..length])))
        })
    }
}

impl<'a> Deref for ItemScope<'a> {
    type Target = Scope<'a>;

    fn deref(&self) -> &Scope<'a> {
        &self.scope
    }
}
