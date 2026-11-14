//! Paths and patterns (SPEC sections 1 and 2).

/// A path: a non-empty list of non-empty segments.
pub type Path = Vec<String>;

/// Splits `text` at `separator` into a path or raw pattern.
pub fn split(text: &str, separator: &str) -> Vec<String> {
    text.split(separator).map(str::to_owned).collect()
}

/// Prints a path or raw pattern with `separator`.
pub fn join(segments: &[String], separator: &str) -> String {
    segments.join(separator)
}

/// True when `prefix` is a prefix of `path`. A path is a prefix of itself.
pub fn is_prefix(prefix: &[String], path: &[String]) -> bool {
    prefix.len() <= path.len() && prefix.iter().zip(path).all(|(a, b)| a == b)
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Segment {
    /// `**`: zero or more whole segments.
    Any,
    /// A glob for one segment. `*` matches any run of characters.
    Glob(String),
}

/// A validated pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
    segments: Vec<Segment>,
}

impl Pattern {
    /// Validates raw segments. Returns `None` when the pattern is invalid.
    pub fn new(raw: &[String]) -> Option<Pattern> {
        if raw.is_empty() {
            return None;
        }
        let mut segments = Vec::with_capacity(raw.len());
        for segment in raw {
            if segment.is_empty() {
                return None;
            }
            if segment == "**" {
                segments.push(Segment::Any);
            } else if segment.contains("**") {
                return None;
            } else {
                segments.push(Segment::Glob(segment.clone()));
            }
        }
        Some(Pattern { segments })
    }

    /// Exact match: the pattern matches the whole path.
    pub fn matches(&self, path: &[String]) -> bool {
        matches_from(&self.segments, path)
    }

    /// Match within: the pattern matches some non-empty prefix of the path.
    pub fn matches_within(&self, path: &[String]) -> bool {
        (1..=path.len()).any(|len| self.matches(&path[..len]))
    }

    /// The first segment, when it has no wildcard.
    pub fn literal_head(&self) -> Option<&str> {
        match self.segments.first() {
            Some(Segment::Glob(glob)) if !glob.contains('*') => Some(glob),
            _ => None,
        }
    }

    /// The number of pattern segments.
    pub fn len(&self) -> usize {
        self.segments.len()
    }

    /// Always false. A valid pattern has at least one segment.
    pub fn is_empty(&self) -> bool {
        false
    }

    /// True when the pattern starts with the segments of `path` and these have no wildcard.
    pub fn literal_prefix_is(&self, path: &[String]) -> bool {
        path.len() <= self.segments.len()
            && self.segments.iter().zip(path).all(|(segment, wanted)| {
                matches!(segment, Segment::Glob(glob) if !glob.contains('*') && glob == wanted)
            })
    }
}

fn matches_from(pattern: &[Segment], path: &[String]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((Segment::Any, rest)) => {
            (0..=path.len()).any(|skip| matches_from(rest, &path[skip..]))
        }
        Some((Segment::Glob(glob), rest)) => match path.split_first() {
            Some((head, tail)) => glob_matches(glob, head) && matches_from(rest, tail),
            None => false,
        },
    }
}

/// Matches one segment against a glob in which `*` is any run of characters.
///
/// The match is on bytes, without a copy of the texts. That gives the same answer as a match
/// on characters: `*` is one byte, and in UTF-8 a character cannot begin inside another one.
fn glob_matches(glob: &str, text: &str) -> bool {
    let (glob, text) = (glob.as_bytes(), text.as_bytes());
    if !glob.contains(&b'*') {
        return glob == text;
    }
    let (mut g, mut t) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while t < text.len() {
        if g < glob.len() && glob[g] == b'*' {
            backtrack = Some((g, t));
            g += 1;
        } else if g < glob.len() && glob[g] == text[t] {
            g += 1;
            t += 1;
        } else if let Some((star, start)) = backtrack {
            backtrack = Some((star, start + 1));
            g = star + 1;
            t = start + 1;
        } else {
            return false;
        }
    }
    glob[g..].iter().all(|c| *c == b'*')
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pattern(text: &str) -> Pattern {
        Pattern::new(&split(text, "::")).expect("valid pattern")
    }

    fn path(text: &str) -> Path {
        split(text, "::")
    }

    #[test]
    fn globs_stay_inside_one_segment() {
        assert!(glob_matches("*", "abc"));
        assert!(glob_matches("a*c", "abc"));
        assert!(glob_matches("a*c", "ac"));
        assert!(glob_matches("*_pass", "inline_pass"));
        assert!(glob_matches("a*b*c", "aXbYbZc"));
        assert!(!glob_matches("a*c", "abd"));
        assert!(!glob_matches("abc", "abcd"));
        assert!(!pattern("crate::*").matches(&path("crate::a::b")));
    }

    #[test]
    fn globs_match_text_that_is_not_ascii() {
        assert!(pattern("crate::grüße").matches(&path("crate::grüße")));
        assert!(!pattern("crate::grüße").matches(&path("crate::grüsse")));
        assert!(pattern("crate::*ße").matches(&path("crate::grüße")));
        assert!(pattern("crate::gr*e").matches(&path("crate::grüße")));
        assert!(pattern("crate::日*語").matches(&path("crate::日本語")));
        assert!(!pattern("crate::日*語").matches(&path("crate::日本")));
        // `é` and `è` have the same first byte. A star must not match a part of a character.
        assert!(!pattern("crate::*è").matches(&path("crate::é")));
    }

    #[test]
    fn double_star_is_zero_or_more_segments() {
        let p = pattern("crate::a::**");
        assert!(p.matches(&path("crate::a")));
        assert!(p.matches(&path("crate::a::b::c")));
        assert!(!p.matches(&path("crate::ab")));
        assert!(pattern("**::tests").matches(&path("crate::x::tests")));
        assert!(pattern("**").matches(&path("x")));
    }

    #[test]
    fn within_matches_a_prefix() {
        assert!(pattern("std::fs").matches_within(&path("std::fs::read")));
        assert!(pattern("std::fs").matches_within(&path("std::fs")));
        assert!(!pattern("std::fs").matches_within(&path("std")));
        assert!(!pattern("std::fs").matches_within(&path("std::fsx::read")));
    }

    #[test]
    fn invalid_patterns_are_rejected() {
        assert!(Pattern::new(&[]).is_none());
        assert!(Pattern::new(&split("a::::b", "::")).is_none());
        assert!(Pattern::new(&split("a::b**", "::")).is_none());
        assert!(Pattern::new(&split("", "::")).is_none());
    }
}
