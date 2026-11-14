The Rust examples of the README and of `docs/`. `tests/readme_examples.rs` makes this file. Do not edit it.

README.md, example 1:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the domain does not use storage")
    .modules(any_of(["crate::domain::**"]))
    .must_not_use(any_of(["crate::storage", "std::fs"]))
    .check(&app);
```

README.md, example 2:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
use std::sync::LazyLock;

use refpact::{all_except, any_of, rule, Crate};

static APP: LazyLock<Crate> = LazyLock::new(|| Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap());

fn files_are_opened_in_the_platform_layer_only() {
    rule("files are opened in the platform layer only")
        .modules(all_except(["crate::platform::**"]))
        .must_not_use(any_of(["std::fs"]))
        .except_in_test_code()
        .check(&APP);
}
```

README.md, example 3:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
layers("the layers of the shop")
    .layer("api", any_of(["crate::api::**"]))
    .layer("domain", any_of(["crate::domain::**"]))
    .layer("storage", any_of(["crate::storage::**"]))
    .check(&app);
```

README.md, example 4:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("no feature uses another feature")
    .modules(all_except(["crate::features::shared::**"]))
    .must_be_independent_between(any_of(["crate::features::*"]))
    .check(&app);
```

README.md, example 5:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the parts of the crate form no cycle")
    .modules(all_except(["crate"]))
    .must_be_acyclic_between(any_of(["crate::*"]))
    .check(&app);
```

README.md, example 6:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the api does not reach the file system")
    .modules(any_of(["crate::api::**"]))
    .must_not_reach(any_of(["std::fs"]))
    .check(&app);
```

README.md, example 7:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("every module is used")
    .modules(all_except(["crate", "**::tests"]))
    .must_be_used()
    .check(&app);
```

README.md, example 8:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("repositories live in the storage")
    .items(all())
    .implementing(any_of(["crate::domain::Repository"]))
    .must_reside_in(any_of(["crate::storage::**"]))
    .check(&app);
```

README.md, example 9:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let baseline = Baseline::file(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/architecture.baseline"));

rule("the domain does not use storage")
    .modules(any_of(["crate::domain::**"]))
    .must_not_use(any_of(["crate::storage"]))
    .check_with_baseline(&app, &baseline);
```

docs/rust.md, example 1:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
use std::sync::LazyLock;

use refpact::{all_except, any_of, rule, unsafe_code, Crate};

static APP: LazyLock<Crate> = LazyLock::new(|| Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap());

fn frontend_passes_do_not_use_the_back_ends() {
    rule("frontend passes do not use the back ends")
        .modules(any_of(["crate::frontend::passes::**"]))
        .must_not_use(any_of(["crate::codegen", "crate::interpreter", "crate::cache"]))
        .check(&APP);
}

fn files_are_opened_in_the_platform_layer_only() {
    rule("files are opened in the platform layer only")
        .modules(all_except(["crate::platform::**"]))
        .must_not_use(any_of(["std::fs", "std::env"]))
        .except_in_test_code()
        .check(&APP);
}

fn no_unsafe_code_in_the_interpreter() {
    rule("no unsafe code in the interpreter")
        .modules(any_of(["crate::interpreter::**"]))
        .must_not_contain(unsafe_code())
        .check(&APP);
}
```

docs/rust.md, example 2:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the domain uses only itself and the standard library")
    .modules(any_of(["crate::domain::**"]))
    .must_only_use(any_of(["std", "core", "alloc", "crate::domain"]))
    .check(&app);
```

docs/rust.md, example 3:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the parts of the crate form no cycle")
    .modules(all_except(["crate"]))
    .must_be_acyclic_between(any_of(["crate::*"]))
    .except_in_test_code()
    .check(&app);
```

docs/rust.md, example 4:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
layers("the layers of the shop")
    .layer("api", any_of(["crate::api::**"]))          // the top
    .layer("domain", any_of(["crate::domain::**"]))
    .layer("storage", any_of(["crate::storage::**"]))  // the bottom
    .check(&app);
```

docs/rust.md, example 5:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
layers("the layers of the shop")
    .layer("api", any_of(["crate::api::**"]))
    .layer("jobs", any_of(["crate::jobs::**"]))
    .layer("domain", any_of(["crate::domain::**"]))
    .layer("storage", any_of(["crate::storage::**"]))
    .may_only_use("jobs", ["domain"])                 // not the storage
    .may_only_be_used_by("storage", ["domain"])       // not the api, not the jobs
    .every_module_in_a_layer(all_except(["crate"]))
    .check(&app);
```

docs/rust.md, example 6:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("only the domain uses the storage")
    .modules(any_of(["crate::storage::**"]))
    .must_only_be_used_by(any_of(["crate::domain::**"]))
    .check(&app);
```

docs/rust.md, example 7:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("repositories live in the storage")
    .items(all())
    .implementing(any_of(["crate::domain::Repository"]))
    .must_reside_in(any_of(["crate::storage::**"]))
    .check(&app);

rule("nothing in internal is public")
    .items(any_of(["crate::internal::**"]))
    .with_visibility([Visibility::Pub])
    .must_not_exist()
    .check(&app);

rule("internal items are not exported")
    .items(any_of(["crate::internal::**"]))
    .exported()
    .must_not_exist()
    .check(&app);

rule("only the api derives Serialize")
    .items(all_except(["crate::api::**"]))
    .deriving(any_of(["serde::Serialize"]))
    .must_not_exist()
    .check(&app);

rule("repositories are named so")
    .items(all())
    .implementing(any_of(["crate::domain::Repository"]))
    .must_be_named(["*Repository"])
    .check(&app);
```

docs/rust.md, example 8:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the public API is documented")
    .items(all())
    .of_kind([ItemKind::Fn, ItemKind::Struct, ItemKind::Method])
    .with_visibility([Visibility::Pub])
    .must_have_modifier([Modifier::Documented, Modifier::Hidden])
    .check(&app);

rule("no async in the domain")
    .items(any_of(["crate::domain::**"]))
    .of_kind([ItemKind::Fn, ItemKind::Method])
    .with_modifier([Modifier::Async])
    .must_not_exist()
    .check(&app);
```

docs/rust.md, example 9:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the domain has no public fields")
    .items(any_of(["crate::domain::**"]))
    .of_kind([ItemKind::Field])
    .with_visibility([Visibility::Pub])
    .must_not_exist()
    .check(&app);

rule("public functions do not return anyhow errors")
    .items(all())
    .of_kind([ItemKind::Fn, ItemKind::Method])
    .with_visibility([Visibility::Pub])
    .returning(any_of(["anyhow::Error"]))
    .must_not_exist()
    .check(&app);
```

docs/rust.md, example 10:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("what is sent over the wire knows no database")
    .items(all())
    .deriving(any_of(["serde::Serialize"]))
    .must_not_use(any_of(["crate::db"]))
    .check(&app);

rule("the handlers use only the services")
    .items(any_of(["crate::api"]))
    .named(["*Handler"])
    .must_only_use(any_of(["std", "crate::services"]))
    .check(&app);
```

docs/rust.md, example 11:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let mut platform = vec![
    rule("files are opened in the platform layer only")
        .modules(all_except(["crate::platform::**"]))
        .must_not_use(any_of(["std::fs"])),
    rule("the platform layer forms no cycle")
        .modules(any_of(["crate::platform::**"]))
        .must_be_acyclic(),
];
platform.extend(
    layers("the layers of the shop")
        .layer("api", any_of(["crate::api::**"]))
        .layer("domain", any_of(["crate::domain::**"]))
        .rules()
        .unwrap(),
);
rules("the platform rules", platform).check(&app);
```

docs/rust.md, example 12:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
onion_architecture("the shop")
    .domain_models(any_of(["crate::domain::model::**"]))
    .domain_services(any_of(["crate::domain::service::**"]))
    .application_services(any_of(["crate::application::**"]))
    .adapter("web", any_of(["crate::adapter::web::**"]))
    .adapter("database", any_of(["crate::adapter::db::**"]))
    .check(&app);
```

docs/rust.md, example 13:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("only the command line prints")
    .modules(all_except(["crate::cli::**"]))
    .must_not_use(any_of(["std::print!", "std::println!", "std::eprint!", "std::eprintln!", "std::dbg!"]))
    .except_in_test_code()
    .check(&app);

rule("only the command line ends the process")
    .modules(all_except(["crate::cli::**"]))
    .must_not_use(any_of(["std::process::exit", "std::process::abort"]))
    .check(&app);

rule("unsafe code is in the allocator only")
    .modules(all_except(["crate::alloc::**"]))
    .must_not_contain(unsafe_code())
    .check(&app);
```

docs/rust.md, example 14:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the domain knows no database")
    .modules(any_of(["crate::domain::**"]))
    .must_not_use(any_of(["crate::db"]))
    .because("the domain runs in tests without a database")
    .check(&app);
```

docs/rust.md, example 15:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("new code does not print")
    .modules(all())
    .must_not_use(any_of(["std::println!"]))
    .as_warning()
    .check(&app);
```

docs/rust.md, example 16:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let text = rule("the parts of the crate")
    .modules(all())
    .must_be_acyclic_between(any_of(["crate::*"]))
    .diagram(&app, DiagramFormat::Mermaid)
    .unwrap();
std::fs::write("docs/architecture.md", format!("```mermaid\n{text}```\n")).unwrap();
```

docs/rust.md, example 17:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let text = layers("the layers of the shop")
    .layer("api", any_of(["crate::api::**"]))
    .layer("domain", any_of(["crate::domain::**"]))
    .layer("storage", any_of(["crate::storage::**"]))
    .diagram(&app, DiagramFormat::Mermaid)
    .unwrap();
```

docs/rust.md, example 18:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("a module uses at most ten other modules")
    .modules(all_except(["crate"]))
    .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
        let units = &scope.facts().units;
        scope
            .subjects()
            .filter_map(|unit| {
                let mut used: Vec<usize> = scope
                    .references()
                    .filter(|seen| seen.unit == unit)
                    .flat_map(|seen| scope.owners(&seen).filter_map(|(_, owner)| owner))
                    .filter(|&owner| owner != unit)
                    .collect();
                used.sort_unstable();
                used.dedup();
                (used.len() > 10).then(|| {
                    let message = format!("uses {} modules", used.len());
                    scope.violation_of_unit(unit, Detail::custom_with_key(message, "uses too many modules"))
                })
            })
            .collect()
    })
    .except_in_test_code()
    .check(&app);
```

docs/rust.md, example 19:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
/// The modules use the targets in `use` declarations only.
struct OnlyImported(core::Targets);

impl Check for OnlyImported {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        let targets = setup.targets(&self.0);
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            scope
                .references()
                .filter(|seen| scope.selects(seen.unit) && seen.reference.kind != "use")
                .filter(|seen| scope.hit(seen, &targets).is_some())
                .map(|seen| scope.violation_at(seen.fact, Detail::custom("names the path in full")))
                .collect()
        })
    }
}

let targets = core::Targets {
    any_of: vec![core::split("std::collections", "::")],
    unverified: Vec::new(),
};
rule("collections are imported")
    .modules(all())
    .must_meet(OnlyImported(targets))
    .check(&app);
```

docs/rust.md, example 20:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("a public function has at most five uses")
    .items(all())
    .of_kind([ItemKind::Fn])
    .with_visibility([Visibility::Pub])
    .must_satisfy(|items: &ItemScope<'_>| -> Vec<Violation> {
        items
            .items()
            .filter(|item| {
                let inside = |seen: &Seen<'_>| seen.fact.item.as_ref().is_some_and(|path| path.starts_with(&item.path));
                items.item_references().filter(inside).count() > 5
            })
            .map(|item| items.violation_of_item(item, Detail::custom("has more than five uses")))
            .collect()
    })
    .check(&app);
```

docs/rust.md, example 21:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("no module is more than three uses away from the errors")
    .modules(all_except(["crate"]))
    .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
        let errors = scope.unit(&core::split("crate::error", "::"));
        let graph = scope.unit_graph();
        let mut search = graph.search(|node| Some(node) == errors);
        scope
            .subjects()
            .filter(|&unit| search.chain(unit).is_some_and(|chain| chain.steps.len() > 3))
            .map(|unit| scope.violation_of_unit(unit, Detail::custom("is far from the errors")))
            .collect()
    })
    .check(&app);
```

docs/rust.md, example 22:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("a module that many modules use is stable")
    .modules(all_except(["crate"]))
    .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
        let graph = scope.unit_graph();
        let mut users = vec![0usize; graph.len()];
        for unit in 0..graph.len() {
            for edge in graph.edges(unit) {
                users[edge.to] += 1;
            }
        }
        scope
            .subjects()
            .filter_map(|unit| {
                let (uses, users) = (graph.edges(unit).len(), users[unit]);
                let instability = uses as f64 / (uses + users).max(1) as f64;
                (users > 20 && instability >= 0.5).then(|| {
                    let message = format!("is used by {users} modules and uses {uses}: instability {instability:.2}");
                    scope.violation_of_unit(unit, Detail::custom_with_key(message, "is not stable"))
                })
            })
            .collect()
    })
    .except_in_test_code()
    .check(&app);
```

docs/rust.md, example 23:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
/// Each part keeps away from the database.
struct NoDatabase(&'static [&'static str]);

impl RuleSet for NoDatabase {
    fn name(&self) -> &str {
        "no part uses the database"
    }

    fn rules(&self) -> Result<Vec<Rule>, Failure> {
        let rule_of = |part: &&str| {
            rule(format!("{part} does not use the database"))
                .modules(any_of([format!("crate::{part}::**")]))
                .must_not_use(any_of(["crate::db"]))
        };
        Ok(self.0.iter().map(rule_of).collect())
    }
}

NoDatabase(&["api", "domain"]).check(&app);
```

docs/rust.md, example 24:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("no feature uses another feature")
    .modules(all_except(["crate::features::shared::**"]))
    .must_be_independent_between(any_of(["crate::features::*"]))
    .check(&app);
```

docs/rust.md, example 25:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("every module is used")
    .modules(all_except(["crate", "**::tests"]))
    .must_be_used()
    .check(&app);

rule("every module is reachable from the crate root")
    .modules(all_except(["**::tests"]))
    .must_be_reachable_from(any_of(["crate"]))
    .check(&app);
```

docs/rust.md, example 26:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("the api does not reach the file system")
    .modules(any_of(["crate::api::**"]))
    .must_not_reach(any_of(["std::fs"]))
    .check(&app);
```

docs/rust.md, example 27:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("no module is longer than 500 lines")
    .modules(all_except(["**::tests"]))
    .must_not_exceed_lines(500)
    .check(&app);

rule("no function is longer than 60 lines")
    .items(all())
    .of_kind([ItemKind::Fn, ItemKind::Method])
    .must_not_exceed_lines(60)
    .except_in_test_code()
    .check(&app);
```

docs/rust.md, example 28:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let app = Crate::options(env!("CARGO_MANIFEST_DIR"))
    .resolving_members()
    .load()
    .unwrap();

rule("only the storage reads files")
    .modules(all_except(["crate::storage::**"]))
    .must_not_use(any_of(["std::fs"]))
    .check(&app);

rule("the api does not change the total of an order")
    .modules(any_of(["crate::api::**"]))
    .must_not_use(any_of(["crate::domain::Order::set_total"]))
    .only_kinds([Kind::MethodCall])
    .check(&app);
```

docs/rust.md, example 29:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let shop = Workspace::load(concat!(env!("CARGO_MANIFEST_DIR"), "/../..")).unwrap();

rule("the domain does not use storage")
    .modules(any_of(["shop_domain::**"]))
    .must_not_use(any_of(["shop_storage"]))
    .check(&shop);

rule("the crates form no cycle")
    .modules(all())
    .must_be_acyclic_between(any_of(["*"]))     // a slice is a crate
    .check(&shop);
```

docs/rust.md, example 30:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let shop = Workspace::options(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    .reading_dependencies()
    .load()
    .unwrap();

rule("the domain is built without a database")
    .modules(any_of(["shop_domain"]))
    .must_not_use(any_of(["sqlx", "shop_storage"]))
    .only_kinds([Kind::Dependency, Kind::TransitiveDependency])
    .check(&shop);
```

docs/rust.md, example 31:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let shop = Workspace::options(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    .reading_dependencies()
    .load()
    .unwrap();

rule("every package has a licence that we accept")
    .modules(all())
    .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
        let accepted = ["MIT", "Apache-2.0", "MIT OR Apache-2.0"];
        scope
            .references()
            .filter(|seen| scope.selects(seen.unit))
            // A member of the workspace is no package of others.
            .filter(|seen| scope.owners(seen).all(|(_, owner)| owner.is_none()))
            .filter_map(|seen| {
                let licence = seen.reference.property(PROPERTY_LICENSE);
                let name = seen.reference.target.join("::");
                (!licence.is_some_and(|name| accepted.contains(&name))).then(|| {
                    let licence = licence.map_or("no licence".to_owned(), |name| format!("the licence {name}"));
                    scope.violation_at(seen.fact, Detail::custom(format!("is built with {name}, which has {licence}")))
                })
            })
            .collect()
    })
    .reading_properties([PROPERTY_LICENSE])
    .only_kinds([Kind::Dependency, Kind::TransitiveDependency])
    .check(&shop);
```

docs/rust.md, example 32:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
use std::collections::{BTreeSet, HashMap};

let shop = Workspace::options(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
    .reading_dependencies()
    .load()
    .unwrap();

rule("a build has one version of each package")
    .modules(all())
    .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
        let mut versions: HashMap<&[String], BTreeSet<&str>> = HashMap::new();
        for seen in scope.references() {
            if let Some(version) = seen.reference.property(PROPERTY_VERSION) {
                versions.entry(&seen.reference.target).or_default().insert(version);
            }
        }
        scope
            .references()
            .filter(|seen| scope.selects(seen.unit))
            .filter_map(|seen| {
                let all = versions.get(seen.reference.target.as_slice())?;
                let version = seen.reference.property(PROPERTY_VERSION)?;
                let name = seen.reference.target.join("::");
                (all.len() > 1).then(|| {
                    let message = format!("is built with {name} {version}, one of {all:?}");
                    let key = format!("is built with several versions of {name}");
                    scope.violation_at(seen.fact, Detail::custom_with_key(message, key))
                })
            })
            .collect()
    })
    .reading_properties([PROPERTY_VERSION])
    .check(&shop);
```

docs/rust.md, example 33:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let baseline = Baseline::file(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/architecture.baseline"));

rule("the domain does not use storage")
    .modules(any_of(["crate::domain::**"]))
    .must_not_use(any_of(["crate::storage"]))
    .check_with_baseline(&app, &baseline);
```

docs/rust.md, example 34:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("files are opened in the platform layer only")
    .modules(all_except(["crate::platform::**"]))
    .must_not_use(any_of(["std::fs"]))
    .ignoring_use(any_of(["crate::config"]), any_of(["std::fs::read_to_string"]))
    .check(&app);
```

docs/rust.md, example 35:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let app = Crate::options(env!("CARGO_MANIFEST_DIR"))
    .expanding_macros()
    .load()
    .unwrap();

rule("the model reads no file")
    .modules(any_of(["crate::model::**"]))
    .must_not_use(any_of(["std::fs"]))
    .check(&app);
```

docs/rust.md, example 36:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
rule("only the Windows code opens files")
    .modules(any_of(["crate::**"]))
    .must_not_use(any_of(["std::fs"]))
    .except_under_cfg("windows")
    .check(&app);
```

docs/rust.md, example 37:

```no_run
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!("CARGO_MANIFEST_DIR")).unwrap();
let linux = Crate::options(env!("CARGO_MANIFEST_DIR"))
    .target("x86_64-unknown-linux-gnu")
    .features(["tls"])
    .load()
    .unwrap();
```
