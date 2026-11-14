//! Keeps the Rust examples of the README and of the documents in `docs/` under test.
//!
//! `readme_examples.md` holds every Rust example of these documents with the lines that an
//! example needs to compile. The crate includes that file as documentation, so `cargo test`
//! compiles each example. This test fails when the file is not what the documents give.
//!
//! To write the file again after a change to a document:
//!
//! ```sh
//! REFPACT_UPDATE_README_EXAMPLES=1 cargo test -p refpact --test readme_examples
//! ```

use std::fs;
use std::path::Path;

/// The lines that come before each example. Rustdoc does not show a line that starts with `#`.
const SETUP: &str = "\
# #![allow(unused, dead_code)]
# use refpact::*;
# let app = Crate::load(env!(\"CARGO_MANIFEST_DIR\")).unwrap();
";

/// The Rust examples of a markdown text, in order.
fn rust_examples(markdown: &str) -> Vec<String> {
    let mut examples = Vec::new();
    let mut current: Option<String> = None;
    for line in markdown.lines() {
        match &mut current {
            None if line == "```rust" => current = Some(String::new()),
            None => {}
            Some(_) if line == "```" => examples.extend(current.take()),
            Some(example) => {
                example.push_str(line);
                example.push('\n');
            }
        }
    }
    assert!(current.is_none(), "a document ends inside a Rust example");
    examples
}

/// The documents with examples: the README, then the files of `docs/` in the order of their
/// names. Each is the path from the root of the repository and the text.
fn documents() -> Vec<(String, String)> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let mut names = vec![String::from("README.md")];
    let mut docs: Vec<String> = fs::read_dir(root.join("docs"))
        .expect("docs is read")
        .map(|entry| {
            entry
                .expect("readable entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| name.ends_with(".md"))
        .map(|name| format!("docs/{name}"))
        .collect();
    docs.sort();
    names.extend(docs);
    names
        .into_iter()
        .map(|name| {
            let text = fs::read_to_string(root.join(&name)).expect("the document is read");
            (name, text)
        })
        .collect()
}

/// The file that the crate includes: each example after the setup lines.
fn expected(documents: &[(String, String)]) -> String {
    let mut text = String::from(
        "The Rust examples of the README and of `docs/`. `tests/readme_examples.rs` makes this file. Do not edit it.\n",
    );
    for (name, document) in documents {
        examples_of(name, document, &mut text);
    }
    text
}

/// Adds the examples of one document.
fn examples_of(name: &str, document: &str, text: &mut String) {
    for (index, example) in rust_examples(document).iter().enumerate() {
        text.push_str(&format!(
            "\n{name}, example {}:\n\n```no_run\n{SETUP}",
            index + 1
        ));
        for line in example.lines() {
            // Without the test harness the compiler removes a `#[test]` function before it
            // checks it, so the attribute is left out.
            if line.trim() != "#[test]" {
                text.push_str(line);
                text.push('\n');
            }
        }
        text.push_str("```\n");
    }
}

#[test]
fn the_examples_file_is_what_the_readme_gives() {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let file = dir.join("tests/readme_examples.md");
    let expected = expected(&documents());
    if std::env::var_os("REFPACT_UPDATE_README_EXAMPLES").is_some() {
        fs::write(&file, &expected).expect("the examples file is written");
    }
    let found = fs::read_to_string(&file).unwrap_or_default();
    assert!(
        found == expected,
        "tests/readme_examples.md is not what the documents give. Write it again with\n\
         REFPACT_UPDATE_README_EXAMPLES=1 cargo test -p refpact --test readme_examples"
    );
}

#[test]
fn the_documents_have_rust_examples() {
    let counts: Vec<(String, usize)> = documents()
        .iter()
        .map(|(name, text)| (name.clone(), rust_examples(text).len()))
        .collect();
    let of = |name: &str| {
        counts
            .iter()
            .find(|(found, _)| found == name)
            .map_or(0, |(_, count)| *count)
    };
    assert!(
        of("README.md") >= 7,
        "the README lost Rust examples, or their fence changed: {counts:?}"
    );
    assert!(
        of("docs/rust.md") >= 9,
        "docs/rust.md lost Rust examples, or their fence changed: {counts:?}"
    );
}

#[test]
fn an_example_is_taken_from_its_fence_only() {
    let examples = rust_examples(
        "text\n```rust\nlet a = 1;\n```\n```ts\nlet b = 2;\n```\n```rust\nlet c = 3;\n```\n",
    );
    assert_eq!(examples, ["let a = 1;\n", "let c = 3;\n"]);
}
