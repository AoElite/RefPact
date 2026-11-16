//! Times the stages of a run: `cargo run --example timing -- <workspace dir> [members]`.
use std::time::Instant;

use refpact::{all, any_of, rule, Workspace};

fn main() {
    let mut args = std::env::args().skip(1);
    let dir = args.next().expect("a directory");
    let resolving = std::env::var("RESOLVE").is_ok();
    let members: Vec<String> = args.collect();
    let timed = |name: &str, run: &mut dyn FnMut() -> String| {
        let start = Instant::now();
        let note = run();
        println!(
            "{:>9.1} ms  {name}  {note}",
            start.elapsed().as_secs_f64() * 1000.0
        );
    };
    let load = || {
        let mut options = Workspace::options(&dir);
        if resolving {
            options = options.resolving_members();
        }
        if !members.is_empty() {
            options = options.members(members.iter().map(String::as_str));
        }
        if let Ok(allowed) = std::env::var("ALLOW") {
            options = options.allow_unreached_files(allowed.split(','));
        }
        options.load().unwrap_or_else(|error| panic!("{error}"))
    };
    let mut app = None;
    let mut best = f64::MAX;
    for _ in 0..7 {
        let start = Instant::now();
        app = Some(load());
        best = best.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    println!("{best:>9.1} ms  load, the best of 7");
    let app = app.unwrap();
    let copy = app.facts().clone();
    let start = Instant::now();
    let prepared = refpact::core::Prepared::new(copy);
    println!(
        "{:>9.1} ms  the lookups of the facts",
        start.elapsed().as_secs_f64() * 1000.0
    );
    drop(prepared);
    let facts = app.facts();
    println!(
        "             {} units, {} facts, {} declarations",
        facts.units.len(),
        facts.facts.len(),
        facts.declarations.len()
    );
    let count = |found: Result<Vec<refpact::Violation>, Vec<refpact::RuleError>>| match found {
        Ok(violations) => format!("{} violations", violations.len()),
        Err(errors) => format!("{} errors", errors.len()),
    };
    timed("must_not_use", &mut || {
        count(
            rule("r")
                .modules(all())
                .must_not_use(any_of(["std::fs"]))
                .evaluate(&app),
        )
    });
    timed("must_not_use again", &mut || {
        count(
            rule("r")
                .modules(all())
                .must_not_use(any_of(["std::fs"]))
                .evaluate(&app),
        )
    });
    timed("must_only_use", &mut || {
        count(
            rule("r")
                .modules(all())
                .must_only_use(any_of(["std", "core"]))
                .evaluate(&app),
        )
    });
    timed("must_be_acyclic", &mut || {
        count(rule("r").modules(all()).must_be_acyclic().evaluate(&app))
    });
    timed("must_be_acyclic_between", &mut || {
        count(
            rule("r")
                .modules(all())
                .must_be_acyclic_between(any_of(["*::*"]))
                .evaluate(&app),
        )
    });
    timed("must_not_reach", &mut || {
        count(
            rule("r")
                .modules(all())
                .must_not_reach(any_of(["std::process"]))
                .evaluate(&app),
        )
    });
    timed("must_be_used", &mut || {
        count(rule("r").modules(all()).must_be_used().evaluate(&app))
    });
    timed("must_be_reachable_from", &mut || {
        count(
            rule("r")
                .modules(all())
                .must_be_reachable_from(any_of(["*"]))
                .evaluate(&app),
        )
    });
    timed("items must_be_exported", &mut || {
        count(
            rule("r")
                .items(all())
                .with_visibility([refpact::Visibility::Pub])
                .must_be_exported()
                .evaluate(&app),
        )
    });
    timed("diagram", &mut || {
        rule("r")
            .modules(all())
            .must_be_acyclic()
            .diagram(&app, refpact::DiagramFormat::Mermaid)
            .map_or_else(|e| e.to_string(), |text| format!("{} bytes", text.len()))
    });
}
