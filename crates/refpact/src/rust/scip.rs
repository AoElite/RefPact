//! Reads the index that `rust-analyzer scip` writes: for each place in the source, the
//! symbol that the code there refers to, as the type checker of rust-analyzer resolves it.
//!
//! The index is a protocol buffer of the SCIP format. Only the fields that are used here are
//! read: the documents, and in each document the occurrences with their range, their symbol
//! and their roles. No library is needed for that.

use std::collections::{HashMap, HashSet};

/// A place in the source: the file, the line and the column, both counted from 1.
pub(crate) type Place = (String, u32, u32);

/// What an index says about a project.
#[derive(Debug, Default)]
pub(crate) struct Index {
    /// The symbols, each one time.
    symbols: Vec<String>,
    /// For each place where the code refers to a symbol: the symbol.
    references: HashMap<Place, usize>,
    /// For each symbol that the indexed code defines: the place of its name.
    definitions: HashMap<usize, Place>,
    /// The places where the code names something that has no path, such as a local variable.
    locals: HashSet<Place>,
}

impl Index {
    /// Reads an index. Each file is named relative to the directory that was indexed.
    pub fn read(bytes: &[u8]) -> Result<Index, String> {
        let mut index = Index::default();
        let mut numbers: HashMap<String, usize> = HashMap::new();
        let mut documents = 0;
        for field in Fields::new(bytes) {
            let (number, value) = field?;
            // Index.documents
            if let (2, Value::Bytes(document)) = (number, value) {
                documents += 1;
                index.document(document, &mut numbers)?;
            }
        }
        if documents == 0 {
            return Err("the index has no documents".to_owned());
        }
        Ok(index)
    }

    fn document(
        &mut self,
        bytes: &[u8],
        numbers: &mut HashMap<String, usize>,
    ) -> Result<(), String> {
        let mut path = "";
        let mut occurrences = Vec::new();
        for field in Fields::new(bytes) {
            match field? {
                // Document.relative_path
                (1, Value::Bytes(text)) => path = text_of(text)?,
                // Document.occurrences
                (2, Value::Bytes(occurrence)) => occurrences.push(occurrence),
                _ => {}
            }
        }
        for occurrence in occurrences {
            let mut range = None;
            let mut symbol = "";
            let mut roles = 0;
            for field in Fields::new(occurrence) {
                match field? {
                    // Occurrence.range: the line and the column of the start, from 0.
                    (1, Value::Bytes(packed)) => {
                        let mut numbers = Packed(packed);
                        range = match (numbers.next(), numbers.next()) {
                            (Some(Ok(line)), Some(Ok(column))) => Some((line, column)),
                            _ => return Err("an occurrence has no range".to_owned()),
                        };
                    }
                    // Occurrence.symbol
                    (2, Value::Bytes(text)) => symbol = text_of(text)?,
                    // Occurrence.symbol_roles
                    (3, Value::Number(bits)) => roles = bits,
                    _ => {}
                }
            }
            let Some((line, column)) = range else {
                continue;
            };
            if symbol.is_empty() {
                continue;
            }
            // A local variable has no path.
            if symbol.starts_with("local ") {
                let place = (path.to_owned(), line as u32 + 1, column as u32 + 1);
                self.locals.insert(place);
                continue;
            }
            let number = match numbers.get(symbol) {
                Some(number) => *number,
                None => {
                    self.symbols.push(symbol.to_owned());
                    numbers.insert(symbol.to_owned(), self.symbols.len() - 1);
                    self.symbols.len() - 1
                }
            };
            let place = (path.to_owned(), line as u32 + 1, column as u32 + 1);
            // The first bit of the roles says that the occurrence is the definition.
            if roles & 1 == 1 {
                self.definitions.insert(number, place);
            } else {
                self.references.insert(place, number);
            }
        }
        Ok(())
    }

    /// The symbols that the indexed code defines, each with the place of its name: those that
    /// [`Symbol`] reads, and the types and traits.
    pub fn definitions(&self) -> impl Iterator<Item = (Symbol<'_>, &Place)> + '_ {
        self.definitions.iter().filter_map(|(number, place)| {
            Symbol::parse_definition(&self.symbols[*number]).map(|symbol| (symbol, place))
        })
    }

    /// Whether the code at a place names something that has no path, such as a local
    /// variable or a parameter.
    pub fn is_local(&self, place: &Place) -> bool {
        self.locals.contains(place)
    }

    /// The item that the code at a place names: where it is defined, not how the code
    /// reaches it.
    pub fn named_at(&self, place: &Place) -> Option<Defined<'_>> {
        let number = *self.references.get(place)?;
        Defined::parse(&self.symbols[number])
    }

    /// The symbol that the code at a place refers to, and where the indexed code defines it.
    pub fn at(&self, place: &Place) -> Option<(Symbol<'_>, Option<&Place>)> {
        let number = *self.references.get(place)?;
        let symbol = Symbol::parse(&self.symbols[number])?;
        Some((symbol, self.definitions.get(&number)))
    }
}

/// What a symbol names at its end.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Form {
    /// A function or a method.
    Method,
    /// A field, a constant or a static.
    Term,
    /// A type or a trait. Only a definition has this form.
    Type,
}

/// A symbol of rust-analyzer, such as
/// `rust-analyzer cargo std <version> fs/impl#[File][Read]read_to_end().`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Symbol<'a> {
    /// The package that defines the symbol, as its manifest names it.
    pub package: &'a str,
    /// The modules down to the definition. For a function of an `impl`, they are the modules
    /// of the `impl`, which need not be those of the type.
    pub modules: Vec<&'a str>,
    /// The type or the trait that has the function or the field. `None` for a free function.
    pub owner: Option<&'a str>,
    /// The trait of the `impl` that has the function, by its name only.
    pub of_trait: Option<&'a str>,
    /// The name of the function or of the field.
    pub name: &'a str,
    /// What the symbol names.
    pub form: Form,
}

impl<'a> Symbol<'a> {
    /// Reads a symbol that names a function, a method, a field or a constant. `None` for
    /// other symbols, and for a method of a type that has no name, such as `&T` or `[T]`.
    fn parse(symbol: &'a str) -> Option<Symbol<'a>> {
        // `<scheme> <manager> <package> <version> <descriptors>`
        let mut parts = symbol.splitn(5, ' ');
        let (_scheme, _manager) = (parts.next()?, parts.next()?);
        let (package, _version, descriptors) = (parts.next()?, parts.next()?, parts.next()?);
        let mut rest = descriptors;
        let mut modules = Vec::new();
        let mut owner = None;
        let mut of_trait = None;
        let mut in_impl = false;
        let mut brackets = 0;
        loop {
            if let Some(inner) = rest.strip_prefix('[') {
                // `[Type]` and `[Trait]` after `impl#`. A name with other characters than
                // those of a name is between backquotes.
                let (name, after) = match inner.strip_prefix('`') {
                    Some(quoted) => {
                        let end = quoted.find('`')?;
                        (&quoted[..end], quoted[end + 1..].strip_prefix(']')?)
                    }
                    None => {
                        let end = inner.find(']')?;
                        (&inner[..end], &inner[end + 1..])
                    }
                };
                // `Vec<T, A>` is the type `Vec`.
                let name = name.split('<').next().unwrap_or(name);
                match brackets {
                    0 => owner = Some(name),
                    1 => of_trait = Some(name),
                    _ => return None,
                }
                brackets += 1;
                rest = after;
                continue;
            }
            let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
            let (name, after) = rest.split_at(end);
            if let Some(after) = after.strip_prefix('/') {
                modules.push(name);
                rest = after;
            } else if let Some(after) = after.strip_prefix('#') {
                if after.is_empty() {
                    return None;
                }
                if name == "impl" && after.starts_with('[') {
                    in_impl = true;
                } else if owner.is_some() || in_impl {
                    return None;
                } else {
                    owner = Some(name);
                }
                rest = after;
            } else {
                let form = match after {
                    "()." => Form::Method,
                    "." => Form::Term,
                    _ => return None,
                };
                let named = |name: &str| {
                    !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_')
                };
                if name.is_empty() || !owner.map_or(true, named) {
                    return None;
                }
                return Some(Symbol {
                    package,
                    modules,
                    owner,
                    of_trait,
                    name,
                    form,
                });
            }
        }
    }
}

/// The path of the definition of a symbol of any form: a module, a type, a variant, a
/// function, a field or a macro.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Defined<'a> {
    /// The package that defines the symbol, as its manifest names it.
    pub package: &'a str,
    /// The names from the root of the crate down to the symbol. For a function of an `impl`,
    /// the modules are those of the `impl`. Empty for the root of the crate.
    pub names: Vec<&'a str>,
    /// Whether the symbol is a macro.
    pub is_macro: bool,
}

impl<'a> Defined<'a> {
    /// `None` for a symbol whose path has a part without a name, such as a method of `[T]`
    /// or a type parameter.
    fn parse(symbol: &'a str) -> Option<Defined<'a>> {
        let mut parts = symbol.splitn(5, ' ');
        let (_scheme, _manager) = (parts.next()?, parts.next()?);
        let (package, _version, mut rest) = (parts.next()?, parts.next()?, parts.next()?);
        let mut names = Vec::new();
        let mut is_macro = false;
        if rest == "crate/" {
            rest = "";
        }
        while !rest.is_empty() {
            let end = rest.find(|c: char| !(c.is_alphanumeric() || c == '_'))?;
            let (name, after) = rest.split_at(end);
            if name.is_empty() {
                return None;
            }
            if let ("impl", Some(inner)) = (name, after.strip_prefix("#[")) {
                // `impl#[Type]` or `impl#[Type][Trait]`: the type is the name.
                // A type with parameters is between backquotes: `Vec<T, A>` is `Vec`.
                let (owner, after) = match inner.strip_prefix('`') {
                    Some(quoted) => {
                        let close = quoted.find('`')?;
                        (&quoted[..close], quoted[close + 1..].strip_prefix(']')?)
                    }
                    None => {
                        let close = inner.find(']')?;
                        (&inner[..close], &inner[close + 1..])
                    }
                };
                let owner = owner.split('<').next().unwrap_or(owner);
                if owner.is_empty() || !owner.chars().all(|c| c.is_alphanumeric() || c == '_') {
                    return None;
                }
                names.push(owner);
                rest = after;
                if let Some(of_trait) = rest.strip_prefix('[') {
                    rest = match of_trait.strip_prefix('`') {
                        Some(quoted) => quoted[quoted.find('`')? + 1..].strip_prefix(']')?,
                        None => &of_trait[of_trait.find(']')? + 1..],
                    };
                }
                continue;
            }
            names.push(name);
            rest = if let Some(after) = after.strip_prefix("().") {
                after
            } else if let Some(after) = after.strip_prefix(['/', '#', '.']) {
                after
            } else if after == "!" {
                is_macro = true;
                ""
            } else {
                return None;
            };
        }
        Some(Defined {
            package,
            names,
            is_macro,
        })
    }
}

impl<'a> Symbol<'a> {
    /// Reads a symbol that the indexed code defines: what [`Symbol::parse`] reads, and a type
    /// or a trait, `<modules>/Name#`, whose name is that of the type.
    fn parse_definition(symbol: &'a str) -> Option<Symbol<'a>> {
        if let Some(found) = Symbol::parse(symbol) {
            return Some(found);
        }
        let mut parts = symbol.splitn(5, ' ');
        let (_scheme, _manager) = (parts.next()?, parts.next()?);
        let (package, _version, descriptors) = (parts.next()?, parts.next()?, parts.next()?);
        let mut modules: Vec<&str> = descriptors.strip_suffix('#')?.split('/').collect();
        let name = modules.pop()?;
        let named =
            |name: &str| !name.is_empty() && name.chars().all(|c| c.is_alphanumeric() || c == '_');
        if !named(name) || !modules.iter().all(|module| named(module)) {
            return None;
        }
        Some(Symbol {
            package,
            modules,
            owner: None,
            of_trait: None,
            name,
            form: Form::Type,
        })
    }
}

fn text_of(bytes: &[u8]) -> Result<&str, String> {
    std::str::from_utf8(bytes).map_err(|_| "a text of the index is not UTF-8".to_owned())
}

enum Value<'a> {
    Number(u64),
    Bytes(&'a [u8]),
}

/// The fields of one message, each with its number.
struct Fields<'a> {
    bytes: &'a [u8],
}

impl<'a> Fields<'a> {
    fn new(bytes: &'a [u8]) -> Fields<'a> {
        Fields { bytes }
    }

    fn take(&mut self, length: usize) -> Result<&'a [u8], String> {
        if length > self.bytes.len() {
            return Err("the index ends inside a field".to_owned());
        }
        let (taken, rest) = self.bytes.split_at(length);
        self.bytes = rest;
        Ok(taken)
    }
}

impl<'a> Iterator for Fields<'a> {
    type Item = Result<(u64, Value<'a>), String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.bytes.is_empty() {
            return None;
        }
        let mut read = || {
            let key = varint(&mut self.bytes)?;
            let value = match key & 7 {
                0 => Value::Number(varint(&mut self.bytes)?),
                1 => Value::Bytes(self.take(8)?),
                2 => {
                    let length = varint(&mut self.bytes)?;
                    Value::Bytes(self.take(length as usize)?)
                }
                5 => Value::Bytes(self.take(4)?),
                other => return Err(format!("the index has a field of the unknown type {other}")),
            };
            Ok((key >> 3, value))
        };
        let field = read();
        if field.is_err() {
            // Nothing after an error can be read.
            self.bytes = &[];
        }
        Some(field)
    }
}

/// The numbers of a packed field.
struct Packed<'a>(&'a [u8]);

impl Iterator for Packed<'_> {
    type Item = Result<u64, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.0.is_empty() {
            return None;
        }
        Some(varint(&mut self.0))
    }
}

fn varint(bytes: &mut &[u8]) -> Result<u64, String> {
    let mut number = 0u64;
    for (position, byte) in bytes.iter().enumerate().take(10) {
        number |= u64::from(byte & 0x7f) << (7 * position);
        if byte & 0x80 == 0 {
            *bytes = &bytes[position + 1..];
            return Ok(number);
        }
    }
    Err("the index ends inside a number".to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn symbol(descriptors: &str) -> Option<Symbol<'_>> {
        Symbol::parse(descriptors)
    }

    #[test]
    fn a_symbol_names_its_package_its_modules_and_its_end() {
        let found =
            symbol("rust-analyzer cargo std https://x/std fs/impl#[File][Read]read_to_end().");
        assert_eq!(
            found,
            Some(Symbol {
                package: "std",
                modules: vec!["fs"],
                owner: Some("File"),
                of_trait: Some("Read"),
                name: "read_to_end",
                form: Form::Method,
            })
        );
        let found = symbol("rust-analyzer cargo shop 0.1.0 domain/Order#total.").unwrap();
        assert_eq!(
            (found.owner, found.name, found.form),
            (Some("Order"), "total", Form::Term)
        );
        let found = symbol("rust-analyzer cargo shop 0.1.0 api/make().").unwrap();
        assert_eq!(
            (found.owner, found.modules, found.form),
            (None, vec!["api"], Form::Method)
        );
        let found = symbol("rust-analyzer cargo shop 0.1.0 domain/Priced#price().").unwrap();
        assert_eq!((found.owner, found.of_trait), (Some("Priced"), None));
    }

    #[test]
    fn the_parameters_of_a_type_are_not_part_of_its_name() {
        let found =
            symbol("rust-analyzer cargo alloc v vec/impl#[`Vec<T, A>`][`Index<I>`]index().");
        let found = found.unwrap();
        assert_eq!((found.owner, found.of_trait), (Some("Vec"), Some("Index")));
    }

    #[test]
    fn a_symbol_that_is_no_function_and_no_field_is_not_read() {
        assert_eq!(symbol("rust-analyzer cargo shop 0.1.0 domain/Order#"), None);
        assert_eq!(symbol("rust-analyzer cargo shop 0.1.0 domain/"), None);
        assert_eq!(symbol("rust-analyzer cargo std v macros/println!"), None);
        assert_eq!(symbol("local 3"), None);
        // A method of a type without a name.
        assert_eq!(
            symbol("rust-analyzer cargo core v cmp/impls/impl#[`&A`][`PartialEq<&B>`]eq()."),
            None
        );
    }

    #[test]
    fn the_definition_of_a_symbol_of_any_form_has_a_path() {
        let names = |descriptors: &str| {
            let symbol = format!("rust-analyzer cargo rand_core 0.6.4 {descriptors}");
            Defined::parse(&symbol).map(|found| (found.names.join("::"), found.is_macro))
        };
        assert_eq!(names("os/OsRng#"), Some(("os::OsRng".to_owned(), false)));
        assert_eq!(
            names("option/Option#Some#"),
            Some(("option::Option::Some".to_owned(), false))
        );
        assert_eq!(
            names("collections/hash_map/"),
            Some(("collections::hash_map".to_owned(), false))
        );
        assert_eq!(
            names("process/id()."),
            Some(("process::id".to_owned(), false))
        );
        assert_eq!(
            names("fs/impl#[File]open()."),
            Some(("fs::File::open".to_owned(), false))
        );
        assert_eq!(
            names("fs/impl#[File][`Read<T>`]read()."),
            Some(("fs::File::read".to_owned(), false))
        );
        assert_eq!(
            names("data/Field#ident."),
            Some(("data::Field::ident".to_owned(), false))
        );
        assert_eq!(names("macros/vec!"), Some(("macros::vec".to_owned(), true)));
        assert_eq!(names("crate/"), Some((String::new(), false)));
        // A type parameter, and a method of a type without a name.
        assert_eq!(names("os/OsRng#[T]"), None);
        assert_eq!(names("slice/impl#[`[T]`]len()."), None);
        let found = Defined::parse("rust-analyzer cargo rand-core 0.6.4 os/OsRng#").unwrap();
        assert_eq!(found.package, "rand-core");
    }

    #[test]
    fn an_index_that_is_cut_off_is_an_error() {
        assert!(Index::read(&[]).is_err());
        // A document of 5 bytes, of which 1 is there.
        assert!(Index::read(&[0x12, 0x05, 0x0a]).is_err());
    }
}
