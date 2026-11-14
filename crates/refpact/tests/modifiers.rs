//! The modifiers of items: what a declaration says besides its kind and its visibility.

use std::sync::LazyLock;

use refpact::{all, rule, Crate, ErrorCode, ItemKind, Modifier, Violation, Visibility};

static SHOP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[(
            "src/lib.rs",
            "/// Serves one request.\n\
             pub async fn serve() {}\n\
             pub const fn limit() -> u32 { 3 }\n\
             pub unsafe fn raw() {}\n\
             pub extern \"C\" fn callback() {}\n\
             #[doc(hidden)]\n\
             pub fn internal() {}\n\
             static mut COUNT: u32 = 0;\n\
             static NAME: &str = \"shop\";\n\
             pub unsafe trait Zeroed {}\n\
             #[doc = \"An order.\"]\n\
             pub struct Order {\n\
                 /// The total.\n\
                 pub total: u32,\n\
                 pub count: u32,\n\
             }\n\
             impl Order {\n\
                 /// Saves the order.\n\
                 pub async fn save(&self) {}\n\
                 pub fn plain(&self) {}\n\
             }\n",
        )],
    )
    .unwrap()
});

#[test]
fn the_modifiers_of_the_declarations() {
    let declared: Vec<String> = SHOP
        .facts()
        .declarations
        .iter()
        .map(|declaration| {
            let path = declaration.path.join("::");
            format!("{path} [{}]", declaration.modifiers.join(", "))
        })
        .collect();
    assert_eq!(
        declared,
        [
            "crate::serve [async, documented]",
            "crate::limit [const]",
            "crate::raw [unsafe]",
            "crate::callback [extern]",
            "crate::internal [hidden]",
            "crate::COUNT [mut]",
            "crate::NAME []",
            "crate::Zeroed [unsafe]",
            "crate::Order [documented]",
            "crate::Order::total [documented]",
            "crate::Order::count []",
            "crate::Order::save [async, documented]",
            "crate::Order::plain []",
        ]
    );
}

fn items(violations: &[Violation]) -> Vec<String> {
    violations
        .iter()
        .map(|violation| match &violation.detail {
            refpact::Detail::Item { item, .. } => item.join("::"),
            other => panic!("not an item: {other:?}"),
        })
        .collect()
}

#[test]
fn rules_on_the_modifiers_of_items() {
    let no_async = rule("no async")
        .items(all())
        .of_kind([ItemKind::Fn, ItemKind::Method])
        .with_modifier([Modifier::Async])
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(items(&no_async), ["crate::serve", "crate::Order::save"]);
    let unsafe_code = rule("no unsafe items")
        .items(all())
        .with_modifier([Modifier::Unsafe, Modifier::Mut])
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        items(&unsafe_code),
        ["crate::raw", "crate::COUNT", "crate::Zeroed"]
    );
    let documented = rule("the public API is documented")
        .items(all())
        .of_kind([
            ItemKind::Fn,
            ItemKind::Struct,
            ItemKind::Field,
            ItemKind::Method,
        ])
        .with_visibility([Visibility::Pub])
        .must_have_modifier([Modifier::Documented, Modifier::Hidden])
        .evaluate(&SHOP)
        .unwrap();
    assert_eq!(
        items(&documented),
        [
            "crate::limit",
            "crate::raw",
            "crate::callback",
            "crate::Order::count",
            "crate::Order::plain",
        ]
    );
}

#[test]
fn a_modifier_list_must_not_be_empty() {
    let errors = rule("r")
        .items(all())
        .with_modifier([])
        .must_not_exist()
        .evaluate(&SHOP)
        .unwrap_err();
    assert_eq!(errors[0].code, ErrorCode::EmptyList);
}
