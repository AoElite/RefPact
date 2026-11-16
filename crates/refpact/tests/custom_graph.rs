//! What a check of the user can do with the graphs and the checks of arguments that the
//! built-in rules use: each built-in rule here is written again as a check of the user, and
//! must give the same violations.

use std::sync::{Arc, LazyLock, Mutex};

use refpact::core::{Constraint, End, RawPattern, Targets};
use refpact::{
    all, all_except, any_of, rule, Check, Checker, Crate, Detail, ErrorCode, Role, Scope, Setup,
    Violation,
};

static APP: LazyLock<Crate> = LazyLock::new(|| {
    Crate::from_memory(
        "src/lib.rs",
        &[
            (
                "src/lib.rs",
                "pub mod a;\npub mod b;\npub mod c;\npub mod d;\npub mod e;\n",
            ),
            (
                "src/a.rs",
                "pub mod deep;\nuse crate::b::B;\npub struct A;\n",
            ),
            ("src/a/deep.rs", "use crate::c::C;\n"),
            ("src/b.rs", "use crate::c::C;\npub struct B;\n"),
            (
                "src/c.rs",
                "use crate::a::A;\nuse crate::d;\npub struct C;\n",
            ),
            (
                "src/d.rs",
                "pub fn read() { let _ = std::fs::read(\"x\"); }\n",
            ),
            ("src/e.rs", "pub unsafe fn e() {}\n"),
        ],
    )
    .unwrap()
});

fn patterns(list: &[&str]) -> Vec<RawPattern> {
    list.iter()
        .map(|pattern| refpact::core::split(pattern, "::"))
        .collect()
}

/// `must_be_acyclic` and `must_be_acyclic_between`, from the graph of the subjects.
struct Acyclic(Option<Vec<RawPattern>>);

impl Check for Acyclic {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        let slices = self.0.as_ref().map(|slices| setup.slices(slices));
        if !setup.has_errors() && setup.nodes(slices.as_ref()).len() < 2 {
            setup.error(ErrorCode::TooFewNodes, None, None);
        }
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            let graph = scope.subject_graph(slices.as_ref());
            graph
                .tangles()
                .into_iter()
                .map(|tangle| {
                    let steps = tangle
                        .cycle
                        .iter()
                        .map(|&(from, edge)| graph.step(from, edge))
                        .collect();
                    let detail = Detail::Cycle {
                        steps,
                        size: tangle.members.len(),
                    };
                    let first = graph.fact(tangle.members[0], tangle.cycle[0].1);
                    Violation {
                        unit: graph.node(tangle.members[0]).clone(),
                        note: None,
                        ..scope.violation_at(first, detail)
                    }
                })
                .collect()
        })
    }
}

#[test]
fn the_graph_of_the_subjects_gives_the_cycles_of_the_built_in_rule() {
    let own = rule("no cycles")
        .modules(all())
        .must_meet(Acyclic(None))
        .evaluate(&APP)
        .unwrap();
    let built_in = rule("no cycles")
        .modules(all())
        .must_be_acyclic()
        .evaluate(&APP)
        .unwrap();
    assert_eq!(own, built_in);
    let Detail::Cycle { steps, size } = &own[0].detail else {
        panic!("not a cycle")
    };
    assert_eq!((steps.len(), *size), (3, 3));

    // A tangle gives its members: `d` and `e` are in no cycle, and `deep` only uses `c`.
    let members = Arc::new(Mutex::new(Vec::new()));
    let found = Arc::clone(&members);
    rule("the members of each tangle")
        .modules(all())
        .must_meet(move |scope: &Scope<'_>| -> Vec<Violation> {
            let graph = scope.subject_graph(None);
            for tangle in graph.tangles() {
                let paths = tangle
                    .members
                    .iter()
                    .map(|&node| graph.node(node).join("::"));
                found.lock().unwrap().push(paths.collect::<Vec<_>>());
            }
            Vec::new()
        })
        .evaluate(&APP)
        .unwrap();
    assert_eq!(
        *members.lock().unwrap(),
        [["crate::a", "crate::b", "crate::c"]]
    );

    // With slices, `crate::a::deep` is in the slice `crate::a`.
    let slices = || any_of(["crate::*"]);
    let own = rule("no cycles")
        .modules(all_except(["crate"]))
        .must_meet(Acyclic(Some(patterns(&["crate::*"]))))
        .evaluate(&APP)
        .unwrap();
    let built_in = rule("no cycles")
        .modules(all_except(["crate"]))
        .must_be_acyclic_between(slices())
        .evaluate(&APP)
        .unwrap();
    assert_eq!(own, built_in);
    assert_eq!(own.len(), 1);

    // The errors of the slices and of the nodes are those of the built-in rule.
    let errors = |modules: refpact::Modules, slices: &[&str]| {
        let own = rule("no cycles")
            .modules(modules.clone())
            .must_meet(Acyclic(Some(patterns(slices))))
            .evaluate(&APP)
            .unwrap_err();
        let built_in = rule("no cycles")
            .modules(modules)
            .must_be_acyclic_between(any_of(slices))
            .evaluate(&APP)
            .unwrap_err();
        assert_eq!(own, built_in);
        own
    };
    let nothing = errors(all(), &["nothing::*", "a::**::**x"]);
    assert_eq!(nothing[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(nothing[1].code, ErrorCode::InvalidPattern);
    assert_eq!(errors(all(), &[])[0].code, ErrorCode::EmptyList);
    let one_node = errors(any_of(["crate::a", "crate::a::deep"]).into(), &["crate::*"]);
    assert_eq!(one_node[0].code, ErrorCode::TooFewNodes);
}

/// `must_not_reach`, from the graph of all units.
struct NotReach(Targets);

impl Check for NotReach {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        let targets = setup.targets(&self.0);
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            let facts = scope.facts();
            let graph = scope.unit_graph();
            // The first reference of each unit that leads to a target.
            let mut ends = vec![None; graph.len()];
            for seen in scope.references() {
                if ends[seen.unit].is_none() {
                    ends[seen.unit] = scope.hit(&seen, &targets).map(|path| (seen, path));
                }
            }
            let mut search = graph.search(|node| ends[node].is_some());
            let mut found = Vec::new();
            for unit in scope.subjects() {
                let Some(chain) = search.chain(unit) else {
                    continue;
                };
                let (seen, path) = ends[chain.end].expect("an end");
                let place = chain
                    .steps
                    .first()
                    .map_or(seen.fact, |&(from, edge)| graph.fact(from, edge));
                let detail = Detail::Reaches {
                    steps: chain
                        .steps
                        .iter()
                        .map(|&(from, edge)| graph.step(from, edge))
                        .collect(),
                    end: End {
                        unit: seen.fact.unit.clone(),
                        file: seen.fact.file.clone(),
                        line: seen.fact.line,
                        column: seen.fact.column,
                        target: path.clone(),
                        kind: seen.reference.kind.clone(),
                    },
                };
                found.push(Violation {
                    unit: facts.units[unit].clone(),
                    ..scope.violation_at(place, detail)
                });
            }
            found
        })
    }
}

#[test]
fn a_search_in_the_graph_of_all_units_gives_the_chains_of_the_built_in_rule() {
    let targets = Targets {
        any_of: patterns(&["std::fs"]),
        unverified: Vec::new(),
    };
    let own = rule("no files")
        .modules(all())
        .must_meet(NotReach(targets))
        .evaluate(&APP)
        .unwrap();
    let built_in = rule("no files")
        .modules(all())
        .must_not_reach(any_of(["std::fs"]))
        .evaluate(&APP)
        .unwrap();
    assert_eq!(own, built_in);
    // `a::deep` reaches the file system through `c` and `d`; `d` itself has no step.
    let steps = |unit: &str| {
        let violation = own
            .iter()
            .find(|violation| violation.unit.join("::") == unit)
            .unwrap();
        let Detail::Reaches { steps, .. } = &violation.detail else {
            panic!("not a chain")
        };
        steps.len()
    };
    assert_eq!(steps("crate::a::deep"), 2);
    assert_eq!(steps("crate::d"), 0);
    assert!(own
        .iter()
        .all(|violation| violation.unit.join("::") != "crate::e"));
}

#[test]
fn the_units_that_a_root_reaches() {
    let found = rule("reached from b")
        .modules(all())
        .must_satisfy(|scope: &Scope<'_>| -> Vec<Violation> {
            let units = &scope.facts().units;
            let root = scope.unit(&refpact::core::split("crate::b", "::")).unwrap();
            let reached = scope.unit_graph().reached_from(|node| node == root);
            scope
                .subjects()
                .filter(|&unit| !reached[unit])
                .map(|unit| Violation {
                    unit: units[unit].clone(),
                    ..scope.violation_of_unit(unit, Detail::Unreachable)
                })
                .collect()
        })
        .evaluate(&APP)
        .unwrap();
    let units: Vec<String> = found.iter().map(|found| found.unit.join("::")).collect();
    // `b` uses `c`, which uses `a` and `d`. Nothing uses the root, `a::deep` or `e`.
    assert_eq!(units, ["crate", "crate::a::deep", "crate::e"]);
}

/// A built-in constraint inside a check of the user: `must_not_use`, without the uses in
/// `use` declarations of one line.
struct NotUseBelow {
    constraint: Constraint,
    line: u32,
}

impl Check for NotUseBelow {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        let built_in = self.constraint.prepare(setup);
        let line = self.line;
        Box::new(move |scope: &Scope<'_>| -> Vec<Violation> {
            let mut found = built_in.violations(scope);
            found.retain(|violation| violation.line > line);
            found
        })
    }
}

#[test]
fn a_check_of_the_user_can_run_a_built_in_constraint() {
    let not_use = |targets: &[&str], line: u32| NotUseBelow {
        constraint: Constraint::MustNotUse(Targets {
            any_of: patterns(targets),
            unverified: Vec::new(),
        }),
        line,
    };
    let lines = |line: u32| -> Vec<u32> {
        rule("c is used below the first line only")
            .modules(any_of(["crate::c"]))
            .must_meet(not_use(&["crate::a", "crate::d"], line))
            .evaluate(&APP)
            .unwrap()
            .iter()
            .map(|violation| violation.line)
            .collect()
    };
    assert_eq!(lines(0), [1, 2]);
    assert_eq!(lines(1), [2]);
    // The errors of the built-in constraint are errors of the rule.
    let errors = rule("r")
        .modules(all())
        .must_meet(not_use(&["crate::nothing"], 0))
        .evaluate(&APP)
        .unwrap_err();
    assert_eq!(errors[0].code, ErrorCode::PatternMatchesNothing);
    assert_eq!(errors[0].role, Some(Role::Target));
}

/// A check that reads one argument of each kind, as the built-in rules do.
struct Arguments {
    markers: Vec<String>,
    kinds: Vec<String>,
    visibilities: Vec<String>,
    relation: String,
    related: Vec<RawPattern>,
}

impl Check for Arguments {
    fn prepare<'c>(&'c self, setup: &mut Setup<'_>) -> Box<dyn Checker + 'c> {
        setup.markers(&self.markers);
        setup.item_kinds(&self.kinds);
        setup.visibilities(&self.visibilities);
        setup.related(&self.relation, &self.related);
        Box::new(|_: &Scope<'_>| -> Vec<Violation> { Vec::new() })
    }
}

#[test]
fn a_check_of_the_user_checks_its_arguments_as_the_built_in_rules_do() {
    let texts = |list: &[&str]| list.iter().map(|text| (*text).to_owned()).collect();
    let errors = |arguments: Arguments| -> Vec<(ErrorCode, Option<Role>, Option<String>)> {
        match rule("r").modules(all()).must_meet(arguments).evaluate(&APP) {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .into_iter()
                .map(|error| (error.code, error.role, error.value))
                .collect(),
        }
    };
    let valid = || Arguments {
        markers: texts(&["unsafe-fn"]),
        kinds: texts(&["fn"]),
        visibilities: texts(&["pub"]),
        relation: "exported".to_owned(),
        related: patterns(&["**"]),
    };
    assert_eq!(errors(valid()), []);
    let invalid = Arguments {
        markers: texts(&["goto"]),
        kinds: texts(&["class"]),
        visibilities: texts(&["protected"]),
        relation: "extends".to_owned(),
        related: patterns(&["crate::Nothing", ""]),
    };
    let text = |text: &str| Some(text.to_owned());
    assert_eq!(
        errors(invalid),
        [
            (ErrorCode::UnknownMarker, Some(Role::Marker), text("goto")),
            (
                ErrorCode::UnknownItemKind,
                Some(Role::ItemKind),
                text("class")
            ),
            (
                ErrorCode::UnknownVisibility,
                Some(Role::Visibility),
                text("protected")
            ),
            (
                ErrorCode::UnknownRelation,
                Some(Role::Relation),
                text("extends")
            ),
            (
                ErrorCode::PatternMatchesNothing,
                Some(Role::Related),
                text("crate::Nothing")
            ),
            (ErrorCode::InvalidPattern, Some(Role::Related), text("")),
        ]
    );
    let empty = Arguments {
        markers: Vec::new(),
        kinds: Vec::new(),
        visibilities: Vec::new(),
        relation: "exported".to_owned(),
        related: Vec::new(),
    };
    assert_eq!(
        errors(empty),
        [
            (ErrorCode::EmptyList, Some(Role::Marker), None),
            (ErrorCode::EmptyList, Some(Role::ItemKind), None),
            (ErrorCode::EmptyList, Some(Role::Visibility), None),
            (ErrorCode::EmptyList, Some(Role::Related), None),
        ]
    );
}

/// The metrics of a module, from the graph of all modules: the number of modules that it
/// uses, the number of modules that use it, and its instability. This is the example of
/// `docs/rust.md` with the limits of the small crate.
#[test]
fn a_check_counts_the_users_of_a_module_and_its_instability() {
    let stable = |most_users: usize| {
        rule("a module that many modules use is stable")
            .modules(all_except(["crate"]))
            .must_satisfy(move |scope: &Scope<'_>| -> Vec<Violation> {
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
                        // The share of the uses of other modules among all uses of the module.
                        let instability = uses as f64 / (uses + users).max(1) as f64;
                        (users > most_users && instability >= 0.5).then(|| {
                            let message = format!(
                                "is used by {users} modules and uses {uses}: instability {instability:.2}"
                            );
                            scope.violation_of_unit(
                                unit,
                                Detail::custom_with_key(message, "is not stable"),
                            )
                        })
                    })
                    .collect()
            })
    };
    // `c` is used by `a::deep` and `b`, and uses `a` and `d`.
    let failure = stable(1).verify(&APP).unwrap_err().to_string();
    assert_eq!(
        failure.lines().skip(1).collect::<Vec<_>>(),
        ["  src/c.rs: [a module that many modules use is stable] crate::c is used by 2 modules and uses 2: instability 0.50"]
    );
    stable(2).check(&APP);
    // The filters of the rule decide what the graph has: without `use`, no module uses another.
    stable(1).ignoring_kinds([refpact::Kind::Use]).check(&APP);
}
