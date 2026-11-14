//! The lines of a `Cargo.toml` file that name its dependencies.
//!
//! Cargo says what a manifest means (`cargo metadata`): the packages, their crates and what
//! each depends on. It does not say where in the file a dependency is written. This reads
//! that, and nothing else: it is not a TOML parser.

/// The table of a manifest that names a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Table {
    Normal,
    Dev,
    Build,
}

/// The lines of a manifest that name a dependency: the table, the name as the code writes
/// it, and the line. `rand-core = "0.6"` under `[dependencies]` and the header
/// `[dependencies.rand-core]` both name `rand_core`.
pub(crate) fn dependency_lines(text: &str) -> Vec<(Table, String, u32)> {
    let table_of = |name: &str| match name.rsplit('.').next().unwrap_or(name) {
        "dependencies" => Some(Table::Normal),
        "dev-dependencies" | "dev_dependencies" => Some(Table::Dev),
        "build-dependencies" | "build_dependencies" => Some(Table::Build),
        _ => None,
    };
    let key = |text: &str| {
        let unquoted = text.trim().trim_matches(|c| c == '"' || c == '\'');
        unquoted.replace('-', "_")
    };
    let mut found = Vec::new();
    let mut table = None;
    for (line, text) in (1u32..).zip(text.lines()) {
        let trimmed = text.trim();
        if let Some(header) = trimmed.strip_prefix('[') {
            let name = header.split(']').next().unwrap_or("").trim();
            table = table_of(name);
            // `[dependencies.name]` is one dependency.
            if let Some((tables, dependency)) = name.rsplit_once('.') {
                if let (None, Some(table)) = (table, table_of(tables)) {
                    found.push((table, key(dependency), line));
                }
            }
            continue;
        }
        let (Some(table), false) = (table, trimmed.starts_with('#')) else {
            continue;
        };
        // `name = ..`, or `name.workspace = true`.
        if let Some((before, _)) = trimmed.split_once('=') {
            let name = key(before.split('.').next().unwrap_or(before));
            if !name.is_empty() {
                found.push((table, name, line));
            }
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_the_dependencies() {
        let text = "[package]\nname = \"a\"\n\n[dependencies]\n# rand = \"1\"\nrand-core = \"0.6\"\n\"serde\" = { version = \"1\" }\nlog.workspace = true\n\n[dev-dependencies]\nrand-core = \"0.6\"\n\n[target.'cfg(windows)'.build-dependencies.win-api]\nversion = \"1\"\n\n[features]\ntls = []\n";
        let found = dependency_lines(text);
        let expected = [
            (Table::Normal, "rand_core", 6),
            (Table::Normal, "serde", 7),
            (Table::Normal, "log", 8),
            (Table::Dev, "rand_core", 11),
            (Table::Build, "win_api", 13),
        ];
        let expected = expected.map(|(table, name, line)| (table, name.to_owned(), line));
        assert_eq!(found, expected);
    }
}
