//! Mutant schemata: many mutations built into one copy of the code, each behind a switch.
//!
//! Building the crate once for each mutation is slow, and there are more than a hundred
//! mutations. With schemata, all mutations are built into the code at once and the test
//! binaries choose one at run time. Each function body that has mutations becomes
//!
//! ```text
//! { if __mutant!(1) { <body with mutation 1> } else if __mutant!(2) { ... } else { <body> } }
//! ```
//!
//! `__mutant!(n)` is true when the environment variable `REFPACT_MUTANT` is `n`. The macro is
//! in `mutant.rs` beside `src`, read with `include!`, so that refpact's analysis of its own
//! source does not see it.
//!
//! A mutation that is not inside the body of a `fn` cannot be switched like this. The caller
//! builds it alone, in a copy of its own. So does a mutation whose function does not compile
//! as a schema, for example one that returns `impl Trait` and so cannot return two different
//! closures.
//!
//! The TypeScript mutations use the same rewriting, with a switch on
//! `globalThis.__refpactMutant`, in blocks that a parser finds (`plan_blocks`).
//!
//! All positions are byte offsets into the text.

use std::collections::{BTreeMap, BTreeSet};

pub const MACRO: &str = r#"// Mutant schemata of `cargo xtask mutants`. `__mutant!(n)` is true when the
// environment variable REFPACT_MUTANT is `n`.
#[allow(unused_macros)]
macro_rules! __mutant {
    ($n:literal) => {{
        static ACTIVE: ::std::sync::OnceLock<u32> = ::std::sync::OnceLock::new();
        *ACTIVE.get_or_init(|| {
            ::std::env::var("REFPACT_MUTANT").ok().and_then(|value| value.parse().ok()).unwrap_or(0)
        }) == $n
    }};
}
"#;

/// A mutation to put into a schema: its number, the span of the text that it replaces, and
/// the new text.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Change {
    pub number: usize,
    pub start: usize,
    pub end: usize,
    pub replace: String,
}

/// The `{` and `}` of a body or block.
pub type Body = (usize, usize);

/// The bodies that get a schema, each with its changes.
pub type Bodies = BTreeMap<Body, Vec<Change>>;

/// The span of a schema in the new text, and the numbers of its mutations.
pub type Span = (usize, usize, Vec<usize>);

/// A byte that can be part of an identifier. A byte of a character that is not ASCII counts,
/// as such a character is a letter in every case that matters here.
fn is_ident_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_' || byte >= 0x80
}

fn find(bytes: &[u8], pattern: &[u8], from: usize) -> Option<usize> {
    bytes
        .get(from..)?
        .windows(pattern.len())
        .position(|window| window == pattern)
        .map(|at| from + at)
}

/// The positions of the bytes that are code: not in a comment, a string or a char literal.
pub fn code_positions(text: &str) -> Vec<usize> {
    let bytes = text.as_bytes();
    let n = bytes.len();
    let after_ident = |i: usize| i > 0 && is_ident_byte(bytes[i - 1]);
    let mut code = Vec::new();
    let mut i = 0;
    while i < n {
        let c = bytes[i];
        if bytes[i..].starts_with(b"//") {
            i = find(bytes, b"\n", i).unwrap_or(n);
        } else if bytes[i..].starts_with(b"/*") {
            let mut depth = 1;
            i += 2;
            while i < n && depth > 0 {
                if bytes[i..].starts_with(b"/*") {
                    depth += 1;
                    i += 2;
                } else if bytes[i..].starts_with(b"*/") {
                    depth -= 1;
                    i += 2;
                } else {
                    i += 1;
                }
            }
        } else if let Some((open_end, hashes)) =
            raw_string_start(bytes, i).filter(|_| !after_ident(i))
        {
            // `r#"`: the closing quote is followed by as many `#`.
            let mut close = vec![b'"'];
            close.extend(std::iter::repeat_n(b'#', hashes));
            i = find(bytes, &close, open_end).map_or(n, |end| end + close.len());
        } else if c == b'"' || (bytes[i..].starts_with(b"b\"") && !after_ident(i)) {
            i = find(bytes, b"\"", i).unwrap_or(n) + 1;
            while i < n && bytes[i] != b'"' {
                i += if bytes[i] == b'\\' { 2 } else { 1 };
            }
            i += 1;
        } else if c == b'\'' {
            // A char literal, or a lifetime or a label, which is code.
            if bytes.get(i + 1) == Some(&b'\\') {
                let mut end = find(bytes, b"'", i + 2);
                while let Some(at) = end {
                    if bytes[at - 1] == b'\\' && bytes[at - 2] != b'\\' {
                        end = find(bytes, b"'", at + 1);
                    } else {
                        break;
                    }
                }
                i = end.map_or(n, |at| at + 1);
            } else if let Some(next) = text[i + 1..].chars().next() {
                let close = i + 1 + next.len_utf8();
                if bytes.get(close) == Some(&b'\'') {
                    i = close + 1;
                } else {
                    code.push(i);
                    i += 1;
                }
            } else {
                code.push(i);
                i += 1;
            }
        } else {
            code.push(i);
            i += 1;
        }
    }
    code
}

/// If a raw string starts at `i` (`r"`, `r#"`, `br"`, ...), the position after its `"` and
/// the number of its `#`.
fn raw_string_start(bytes: &[u8], i: usize) -> Option<(usize, usize)> {
    let mut j = i;
    if bytes.get(j) == Some(&b'b') {
        j += 1;
    }
    if bytes.get(j) != Some(&b'r') {
        return None;
    }
    let hashes_start = j + 1;
    j = hashes_start;
    while bytes.get(j) == Some(&b'#') {
        j += 1;
    }
    (bytes.get(j) == Some(&b'"')).then_some((j + 1, j - hashes_start))
}

/// The bodies of the `fn` items.
pub fn function_bodies(text: &str) -> Vec<Body> {
    let bytes = text.as_bytes();
    let code = code_positions(text);
    let code_set: BTreeSet<usize> = code.iter().copied().collect();
    let mut matching = BTreeMap::new();
    let mut stack = Vec::new();
    for &position in &code {
        match bytes[position] {
            b'{' => stack.push(position),
            b'}' => {
                if let Some(open) = stack.pop() {
                    matching.insert(open, position);
                }
            }
            _ => {}
        }
    }

    let mut bodies = Vec::new();
    let mut at = 0;
    while let Some(found) = find(bytes, b"fn", at) {
        at = found + 2;
        let Some(name_end) = fn_name_end(bytes, found) else {
            continue;
        };
        at = name_end;
        if !code_set.contains(&found) {
            continue;
        }
        let mut depth = 0i32;
        let from = code.partition_point(|&position| position < name_end);
        for &position in &code[from..] {
            match bytes[position] {
                b'(' | b'[' => depth += 1,
                b')' | b']' => depth -= 1,
                b';' if depth == 0 => break,
                b'{' if depth == 0 => {
                    if let Some(&close) = matching.get(&position) {
                        bodies.push((position, close));
                    }
                    break;
                }
                _ => {}
            }
        }
    }
    bodies
}

/// If `fn` at `at` begins `fn <name>` as a word, the end of the name.
fn fn_name_end(bytes: &[u8], at: usize) -> Option<usize> {
    if at > 0 && is_ident_byte(bytes[at - 1]) {
        return None;
    }
    let mut i = at + 2;
    let spaces = i;
    while bytes.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    let first = *bytes.get(i)?;
    if i == spaces || !(first.is_ascii_alphabetic() || first == b'_') {
        return None;
    }
    while bytes
        .get(i)
        .is_some_and(|&byte| byte.is_ascii_alphanumeric() || byte == b'_')
    {
        i += 1;
    }
    Some(i)
}

/// Which function body each mutation goes to.
///
/// `mutations` are (number, find, replace); `find` occurs once in `text`. Returns the bodies
/// with their changes, and the numbers that cannot be put into a body: not inside one, or
/// inside a body that is inside another body with mutations.
pub fn plan(text: &str, mutations: &[(usize, String, String)]) -> (Bodies, Vec<usize>) {
    let bodies = function_bodies(text);
    let mut by_body = Bodies::new();
    let mut outside = Vec::new();
    for (number, find, replace) in mutations {
        let start = text
            .find(find.as_str())
            .expect("the text occurs in the file");
        let end = start + find.len();
        let around = bodies
            .iter()
            .filter(|body| body.0 < start && end <= body.1)
            .max();
        match around {
            None => outside.push(*number),
            Some(&body) => by_body.entry(body).or_default().push(Change {
                number: *number,
                start,
                end,
                replace: replace.clone(),
            }),
        }
    }
    let keys: Vec<Body> = by_body.keys().copied().collect();
    for body in &keys {
        let inside_another = keys
            .iter()
            .any(|other| other != body && other.0 < body.0 && body.1 < other.1);
        if inside_another {
            let changes = by_body.remove(body).unwrap_or_default();
            outside.extend(changes.iter().map(|change| change.number));
        }
    }
    (by_body, outside)
}

/// `plan` for TypeScript, where a parser has found the block of each mutation.
///
/// `blocks` gives the block of each change, or `None`. A block inside another block with
/// mutations gives its mutations to the outer one, so that no schema is inside another.
pub fn plan_blocks(blocks: &[Option<Body>], changes: &[Change]) -> (Bodies, Vec<usize>) {
    let mut by_body = Bodies::new();
    let mut outside = Vec::new();
    for (change, block) in changes.iter().zip(blocks) {
        match block {
            None => outside.push(change.number),
            Some(block) => by_body.entry(*block).or_default().push(change.clone()),
        }
    }
    let size = |block: &Body| block.1 - block.0;
    let mut keys: Vec<Body> = by_body.keys().copied().collect();
    keys.sort_by_key(size);
    for block in keys {
        let outer = by_body
            .keys()
            .filter(|other| **other != block && other.0 < block.0 && block.1 < other.1)
            .max_by_key(|other| size(other))
            .copied();
        if let Some(target) = outer {
            let moved = by_body.remove(&block).unwrap_or_default();
            by_body.entry(target).or_default().extend(moved);
        }
    }
    for changes in by_body.values_mut() {
        changes.sort();
    }
    (by_body, outside)
}

pub fn rust_branch(number: usize) -> String {
    format!("if __mutant!({number})")
}

pub fn typescript_branch(number: usize) -> String {
    format!("if ((globalThis as {{ __refpactMutant?: number }}).__refpactMutant === {number})")
}

/// The text with each body of `by_body` made a schema, and the span of each schema in the new
/// text. `branch` gives the condition of a mutation in the syntax of the language.
pub fn rewrite(text: &str, by_body: &Bodies, branch: fn(usize) -> String) -> (String, Vec<Span>) {
    let mut result = String::new();
    let mut spans = Vec::new();
    let mut last = 0;
    for (&(open, close), changes) in by_body {
        let inner = &text[open + 1..close];
        let branches: Vec<String> = changes
            .iter()
            .map(|change| {
                let mutated = format!(
                    "{}{}{}",
                    &text[open + 1..change.start],
                    change.replace,
                    &text[change.end..close]
                );
                format!("{} {{{mutated}}}", branch(change.number))
            })
            .collect();
        let schema = format!("{{ {} else {{{inner}}} }}", branches.join(" else "));
        result.push_str(&text[last..open]);
        let start = result.len();
        result.push_str(&schema);
        let numbers = changes.iter().map(|change| change.number).collect();
        spans.push((start, result.len(), numbers));
        last = close + 1;
    }
    result.push_str(&text[last..]);
    (result, spans)
}

/// The text of a crate root with `include!("../mutant.rs");` after its inner attributes and
/// doc comments, where the macro is in scope for every module below.
pub fn include_macro(lib: &str) -> String {
    let lines: Vec<&str> = lib.split('\n').collect();
    let index = lines
        .iter()
        .position(|line| {
            let line = line.trim();
            !(line.starts_with("//!") || line.starts_with("#![") || line.is_empty())
        })
        .unwrap_or(lines.len());
    let mut lines = lines;
    lines.insert(index, "include!(\"../mutant.rs\");");
    lines.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn code_leaves_out_comments_strings_and_chars() {
        let text = "a // {\nb /* { /* } */ */ c \"{\\\"\" d r#\"{\"# e '{' f 'a g '\\'' h b\"{\" i";
        let code: String = code_positions(text)
            .into_iter()
            .map(|at| text.as_bytes()[at] as char)
            .filter(|c| !c.is_whitespace())
            .collect();
        assert_eq!(code, "abcdef'aghi");
    }

    #[test]
    fn bodies_of_functions() {
        let text =
            "fn a(x: [u8; 2]) -> u8 { let s = \"}\"; { x[0] } }\ntrait T { fn b(); }\nfn c() {}";
        let bodies = function_bodies(text);
        assert_eq!(bodies.len(), 2);
        assert_eq!(
            &text[bodies[0].0..=bodies[0].1],
            "{ let s = \"}\"; { x[0] } }"
        );
        assert_eq!(&text[bodies[1].0..=bodies[1].1], "{}");
    }

    #[test]
    fn a_schema_switches_each_mutation() {
        let text = "fn f() -> u8 { 1 + 2 }\nconst X: u8 = 3;";
        let mutations = vec![
            (1, "1 +".to_string(), "0 +".to_string()),
            (2, "+ 2".to_string(), "+ 5".to_string()),
            (3, "= 3".to_string(), "= 4".to_string()),
        ];
        let (bodies, outside) = plan(text, &mutations);
        assert_eq!(outside, vec![3]);
        let (new, spans) = rewrite(text, &bodies, rust_branch);
        assert_eq!(
            new,
            "fn f() -> u8 { if __mutant!(1) { 0 + 2 } else if __mutant!(2) { 1 + 5 } else { 1 + 2 } }\nconst X: u8 = 3;"
        );
        assert_eq!(spans.len(), 1);
        assert_eq!(&new[spans[0].0..spans[0].0 + 1], "{");
        assert_eq!(spans[0].2, vec![1, 2]);
    }

    #[test]
    fn inner_blocks_give_their_mutations_to_the_outer_one() {
        let change = |number| Change {
            number,
            start: 0,
            end: 0,
            replace: String::new(),
        };
        let blocks = [Some((10, 50)), Some((0, 100)), None, Some((20, 30))];
        let changes = [change(1), change(2), change(3), change(4)];
        let (bodies, outside) = plan_blocks(&blocks, &changes);
        assert_eq!(outside, vec![3]);
        let numbers: Vec<usize> = bodies[&(0, 100)].iter().map(|c| c.number).collect();
        assert_eq!(bodies.len(), 1);
        assert_eq!(numbers, vec![1, 2, 4]);
    }

    #[test]
    fn the_macro_goes_after_the_inner_attributes() {
        let lib = "//! Docs.\n\n#![warn(missing_docs)]\nmod a;\n";
        assert_eq!(
            include_macro(lib),
            "//! Docs.\n\n#![warn(missing_docs)]\ninclude!(\"../mutant.rs\");\nmod a;\n"
        );
    }
}
