//! Rules about items with a check of the user: the filters of the rule select the items,
//! and the subjects, the exemptions, the errors and the reports are those of every rule.

use std::sync::LazyLock;

use refpact::core::{CustomItems, NoReferences};
use refpact::{
    all, any_of, rule, Crate, Detail, ErrorCode, Failure, ItemCheck, ItemChecker, ItemKind,
    ItemScope, Role, Setup, Violation, Visibility,
};

static APP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            ("src/lib.rs", "pub mod a;\npub mod b;\n"),
            (
                "src/a.rs",
                "use std::fs;\n\
                 \n\
                 pub struct Order;\n\
                 \n\
                 pub fn get_order() -> Order {\n\
                     let _ = std::env::args();\n\
                     crate::b::make()\n\
                 }\n\
                 \n\
                 fn helper() {\n\
                     let _ = std::io::stdout();\n\
                 }\n\
                 \n\
                 #[cfg(test)]\n\
                 pub fn get_test_order() {}\n",
            ),
            (
                "src/b.rs",
                "pub fn make() -> crate::a::Order {\n    crate::a::Order\n}\n",
            ),
        ],
    )
    .unwrap()
});

/// One violation for each selected item whose name starts with `get_`.
fn no_getters(items: &ItemScope<'_>) -> Vec<Violation> {
    items
        .items()
        .filter(|item| item.name.starts_with("get_"))
        .map(|item| {
            let detail = Detail::custom_with_key(format!("has the getter {}", item.name), "getter");
            items.violation_of_item(item, detail)
        })
        .collect()
}

#[test]
fn a_check_of_items_gets_the_items_that_the_filters_select() {
    let failure = rule("no getters")
        .items(all())
        .of_kind([ItemKind::Fn])
        .must_satisfy(no_getters)
        .verify(&APP)
        .unwrap_err();
    assert_eq!(
        failure.to_string(),
        "rule `no getters` is violated in 2 place(s):\n  \
         src/a.rs:5:8: [no getters] crate::a has the getter get_order\n  \
         src/a.rs:15:8: [no getters] crate::a has the getter get_test_order"
    );
    // The tags, the subjects and each filter of the rule narrow what the check gets.
    let found = |made: refpact::Rule| made.evaluate(&APP).unwrap().len();
    let functions = || rule("no getters").items(all()).of_kind([ItemKind::Fn]);
    assert_eq!(
        found(functions().must_satisfy(no_getters).except_in_test_code()),
        1
    );
    assert_eq!(
        found(functions().named(["*_test_*"]).must_satisfy(no_getters)),
        1
    );
    assert_eq!(
        found(
            functions()
                .with_visibility([Visibility::Private])
                .must_satisfy(no_getters)
        ),
        0
    );
    let other_module = rule("no getters")
        .items(any_of(["crate::b"]))
        .must_satisfy(no_getters);
    assert_eq!(found(other_module), 0);
    let structs = rule("no getters")
        .items(all())
        .of_kind([ItemKind::Struct])
        .must_satisfy(|items: &ItemScope<'_>| -> Vec<Violation> {
            assert_eq!(
                items
                    .items()
                    .map(|item| item.name.as_str())
                    .collect::<Vec<_>>(),
                ["Order"]
            );
            Vec::new()
        });
    assert_eq!(found(structs), 0);
}

/// One violation for each reference inside a selected item.
fn no_references(items: &ItemScope<'_>) -> Vec<Violation> {
    items
        .item_references()
        .map(|seen| {
            let target = seen.reference.target.join("::");
            items.violation_at(seen.fact, Detail::custom(format!("refers to {target}")))
        })
        .collect()
}

#[test]
fn a_check_of_items_gets_the_references_inside_the_items() {
    let public = || {
        rule("public functions refer to nothing")
            .items(any_of(["crate::a"]))
            .of_kind([ItemKind::Fn])
            .with_visibility([Visibility::Pub])
            .must_satisfy(no_references)
    };
    // Not the `use` at the top of the module, and not the references of `helper`.
    let lines = |made: refpact::Rule| -> Vec<String> {
        let found = made.evaluate(&APP).unwrap();
        let line = |violation: &Violation| {
            let Detail::Custom { message, .. } = &violation.detail else {
                panic!("not a custom detail")
            };
            format!("{}: {message}", violation.line)
        };
        found.iter().map(line).collect()
    };
    assert_eq!(
        lines(public()),
        [
            "5: refers to crate::a::Order",
            "6: refers to std::env::args",
            "7: refers to crate::b::make"
        ]
    );
    // An exemption hides a reference, and the kinds of the rule filter them.
    let exempt = public().ignoring_use(any_of(["crate::a"]), any_of(["std::env"]));
    assert_eq!(lines(exempt).len(), 2);
    assert_eq!(lines(public().only_kinds([refpact::Kind::Type])).len(), 1);
    // An exemption that lets nothing pass is an error, as for every rule.
    let unused = public().ignoring_use(any_of(["crate::b"]), any_of(["std::env"]));
    let errors = unused.evaluate(&APP).unwrap_err();
    assert_eq!(errors[0].code, ErrorCode::UnusedExemption);
}

/// A check with an argument: the globs that the names must not match.
struct NotNamed(Vec<String>);

impl ItemCheck for NotNamed {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn ItemChecker + 'c> {
        let globs = setup.names(&self.0, Role::RequiredName, true);
        Box::new(NoReferences(
            move |items: &ItemScope<'_>| -> Vec<Violation> {
                items
                    .items()
                    .filter(|item| {
                        globs
                            .iter()
                            .any(|glob| glob.matches(std::slice::from_ref(&item.name)))
                    })
                    .map(|item| {
                        items.violation_of_item(item, Detail::custom("has a forbidden name"))
                    })
                    .collect()
            },
        ))
    }
}

#[test]
fn a_check_of_items_reads_its_arguments_and_reports_their_errors() {
    let not_named =
        |globs: &[&str]| NotNamed(globs.iter().map(|glob| (*glob).to_owned()).collect());
    let found = rule("no helpers")
        .items(all())
        .must_meet(not_named(&["help*"]))
        .evaluate(&APP)
        .unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].line, 10);

    // The errors of the filters come first, then those of the check.
    let errors = rule("no helpers")
        .items(all())
        .named(["nothing_*"])
        .must_meet(not_named(&["absent*", "a**"]))
        .evaluate(&APP)
        .unwrap_err();
    let codes: Vec<_> = errors
        .iter()
        .map(|error| (error.code, error.role, error.value.as_deref()))
        .collect();
    assert_eq!(
        codes,
        [
            (
                ErrorCode::PatternMatchesNothing,
                Some(Role::Name),
                Some("nothing_*")
            ),
            (
                ErrorCode::PatternMatchesNothing,
                Some(Role::RequiredName),
                Some("absent*")
            ),
            (
                ErrorCode::InvalidPattern,
                Some(Role::RequiredName),
                Some("a**")
            ),
        ]
    );
    let empty = rule("no helpers")
        .items(all())
        .must_meet(not_named(&[]))
        .evaluate(&APP)
        .unwrap_err();
    assert_eq!(empty[0].code, ErrorCode::EmptyList);

    // The check reads no references, so an exemption of its rule lets nothing pass.
    let exempt = rule("no helpers")
        .items(all())
        .must_meet(not_named(&["help*"]))
        .ignoring_use(any_of(["crate::a"]), any_of(["std::fs"]))
        .evaluate(&APP)
        .unwrap_err();
    assert_eq!(exempt[0].code, ErrorCode::UnusedExemption);
    // A closure may read references, so its rule may have an exemption.
    let closure = rule("no getters")
        .items(all())
        .must_satisfy(no_getters)
        .ignoring_use(any_of(["crate::a"]), any_of(["std::fs"]));
    assert_eq!(closure.evaluate(&APP).unwrap().len(), 2);
}

#[test]
fn a_rule_about_items_with_a_check_is_a_rule_like_the_others() {
    let failure = rule("no getters")
        .items(any_of(["crate::nothing"]))
        .must_satisfy(no_getters)
        .verify(&APP)
        .unwrap_err();
    assert!(matches!(failure, Failure::Invalid { .. }), "{failure}");

    let check = CustomItems::new(no_getters);
    assert_eq!(check, check.clone());
    assert_ne!(check, CustomItems::new(no_getters));
    assert_eq!(format!("{check:?}"), "CustomItems(..)");
    fn plain<T: Send + Sync + std::panic::UnwindSafe + std::panic::RefUnwindSafe>() {}
    plain::<refpact::Rule>();
}
