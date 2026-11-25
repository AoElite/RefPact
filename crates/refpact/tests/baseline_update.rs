//! `REFPACT_BASELINE=update` and `REFPACT_DIAGRAM=update`. This file holds one test, because
//! the variables belong to the whole process and other tests must not see them.

use refpact::{any_of, layers, onion_architecture, rule, Baseline, Crate, Failure};

mod support;
use support::Removed;

#[test]
fn the_environment_variables_make_the_checks_write_their_files() {
    let planted = Crate::options(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../fixtures/rust/planted"
    ))
    .resolving_members()
    .load()
    .unwrap();
    let file = std::env::temp_dir().join(format!("refpact-{}-update.baseline", std::process::id()));
    let _removed = Removed(file.clone());
    let _ = std::fs::remove_file(&file);
    let baseline = Baseline::file(&file);
    let leaf = rule("strict does not use leaf")
        .modules(any_of(["crate::rings::strict"]))
        .must_not_use(any_of(["crate::rings::leaf"]));

    // Any other value does not write.
    std::env::set_var("REFPACT_BASELINE", "yes");
    assert!(leaf.verify_with_baseline(&planted, &baseline).is_err());
    assert!(!file.exists());

    std::env::set_var("REFPACT_BASELINE", "update");
    leaf.check_with_baseline(&planted, &baseline);
    // The plain check does not read the variable or the file.
    assert!(leaf.verify(&planted).is_err());
    std::env::remove_var("REFPACT_BASELINE");

    let text = std::fs::read_to_string(&file).unwrap();
    assert_eq!(
        text.lines().filter(|line| !line.starts_with('#')).count(),
        2
    );
    leaf.check_with_baseline(&planted, &baseline);
    // A diagram file is checked against the diagram of the rule, and written on request.
    let dir = std::env::temp_dir().join(format!("refpact-{}-diagrams", std::process::id()));
    let _removed = Removed(dir.clone());
    let rings = rule("the rings")
        .modules(any_of(["crate::rings::*"]))
        .must_be_acyclic();
    let stack = || {
        layers("the stack")
            .layer("strict", any_of(["crate::rings::strict"]))
            .layer("leaf", any_of(["crate::rings::leaf"]))
    };
    let onion = onion_architecture("the onion")
        .domain_models(any_of(["crate::rings::leaf"]))
        .application_services(any_of(["crate::rings::strict"]));
    let stale = |file: &std::path::Path| {
        matches!(
            rings.verify_diagram(&planted, file),
            Err(Failure::StaleDiagram { .. })
        )
    };
    let markdown = dir.join("rings.md");
    // A file that is not there is not current, and any other value does not write.
    std::env::set_var("REFPACT_DIAGRAM", "yes");
    assert!(stale(&markdown));
    assert!(!markdown.exists());
    std::env::set_var("REFPACT_DIAGRAM", "update");
    for name in ["rings.md", "rings.mmd", "rings.dot"] {
        rings.check_diagram(&planted, dir.join(name));
    }
    stack().check_diagram(&planted, dir.join("stack.mermaid"));
    onion.check_diagram(&planted, dir.join("onion.gv"));
    // The name of the file must give a format, also for an update.
    let unnamed = rings.verify_diagram(&planted, dir.join("rings.txt"));
    assert_eq!(
        unnamed.unwrap_err().to_string(),
        format!(
            "the name of the diagram file {} must end in .md, .mmd, .mermaid, .dot or .gv",
            dir.join("rings.txt").display()
        )
    );
    assert!(!dir.join("rings.txt").exists());
    std::env::remove_var("REFPACT_DIAGRAM");

    let read = |name: &str| std::fs::read_to_string(dir.join(name)).unwrap();
    let mermaid = read("rings.mmd");
    assert!(mermaid.starts_with("---\ntitle: \"the rings\"\n---\nflowchart LR\n"));
    assert_eq!(read("rings.md"), format!("```mermaid\n{mermaid}```\n"));
    assert!(read("rings.dot").starts_with("digraph {\n    label=\"the rings\";"));
    assert!(read("stack.mermaid").contains("flowchart TB\n    n0[\"strict\"]"));
    assert!(read("onion.gv").contains("label=\"the onion\";"));

    // The written files are current, also with the line ends of Windows.
    rings.check_diagram(&planted, &markdown);
    stack().check_diagram(&planted, dir.join("stack.mermaid"));
    onion.check_diagram(&planted, dir.join("onion.gv"));
    std::fs::write(&markdown, read("rings.md").replace('\n', "\r\n")).unwrap();
    rings.check_diagram(&planted, &markdown);
    // An update does not write a file that is current.
    std::env::set_var("REFPACT_DIAGRAM", "update");
    rings.check_diagram(&planted, &markdown);
    // A name with the line ends of Windows is in the diagram as it is.
    let two_lines = rule("two\r\nlines")
        .modules(any_of(["crate::rings::*"]))
        .must_be_acyclic();
    two_lines.check_diagram(&planted, dir.join("lines.mmd"));
    std::env::remove_var("REFPACT_DIAGRAM");
    assert!(read("rings.md").contains("\r\n"));
    assert!(read("lines.mmd").contains("two\r\nlines"));
    two_lines.check_diagram(&planted, dir.join("lines.mmd"));

    // A file with another diagram is not current, and the check does not write it.
    assert!(stack().verify_diagram(&planted, &markdown).is_err());
    std::fs::write(&markdown, "an old diagram\n").unwrap();
    assert!(stale(&markdown));
    assert_eq!(read("rings.md"), "an old diagram\n");
    let failure = rings.verify_diagram(&planted, &markdown).unwrap_err();
    assert_eq!(
        failure.to_string(),
        format!(
            "rule `the rings`: the diagram {} is not the diagram of the code.\n\
             Run the tests with REFPACT_DIAGRAM=update to write the diagram again.",
            markdown.display()
        )
    );
    // A rule that cannot be evaluated has no diagram.
    let nowhere = rule("nowhere")
        .modules(any_of(["crate::nowhere"]))
        .must_be_acyclic();
    assert!(matches!(
        nowhere.verify_diagram(&planted, &markdown),
        Err(Failure::Invalid { .. })
    ));
}
