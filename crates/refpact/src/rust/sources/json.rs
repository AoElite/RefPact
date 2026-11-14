//! A small reader of JSON, for what rust-analyzer and Cargo write.

/// A value of JSON.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Json {
    Null,
    Bool(bool),
    Number(f64),
    Text(String),
    List(Vec<Json>),
    Object(Vec<(String, Json)>),
}

impl Json {
    pub(crate) fn get(&self, key: &str) -> Option<&Json> {
        match self {
            Json::Object(fields) => fields.iter().find(|(name, _)| name == key).map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn take(self, key: &str) -> Option<Json> {
        match self {
            Json::Object(fields) => fields
                .into_iter()
                .find(|(name, _)| name == key)
                .map(|(_, v)| v),
            _ => None,
        }
    }

    pub(crate) fn text(&self) -> Option<&str> {
        match self {
            Json::Text(text) => Some(text),
            _ => None,
        }
    }

    pub(crate) fn list(&self) -> &[Json] {
        match self {
            Json::List(items) => items,
            _ => &[],
        }
    }

    pub(crate) fn number(&self) -> Option<f64> {
        match self {
            Json::Number(number) => Some(*number),
            _ => None,
        }
    }

    pub(crate) fn parse(text: &str) -> Option<Json> {
        let mut rest = text.trim_start();
        let value = Json::value(&mut rest, 0)?;
        rest.trim().is_empty().then_some(value)
    }

    fn value(rest: &mut &str, depth: u32) -> Option<Json> {
        *rest = rest.trim_start();
        if depth > 64 {
            return None;
        }
        let first = rest.chars().next()?;
        match first {
            'n' => rest
                .strip_prefix("null")
                .map(|after| (*rest = after, Json::Null).1),
            't' => rest
                .strip_prefix("true")
                .map(|after| (*rest = after, Json::Bool(true)).1),
            'f' => rest
                .strip_prefix("false")
                .map(|after| (*rest = after, Json::Bool(false)).1),
            '"' => Json::string(rest).map(Json::Text),
            '[' => {
                *rest = &rest[1..];
                let mut items = Vec::new();
                loop {
                    *rest = rest.trim_start();
                    if let Some(after) = rest.strip_prefix(']') {
                        *rest = after;
                        return Some(Json::List(items));
                    }
                    if !items.is_empty() {
                        *rest = rest.strip_prefix(',')?;
                    }
                    items.push(Json::value(rest, depth + 1)?);
                }
            }
            '{' => {
                *rest = &rest[1..];
                let mut fields = Vec::new();
                loop {
                    *rest = rest.trim_start();
                    if let Some(after) = rest.strip_prefix('}') {
                        *rest = after;
                        return Some(Json::Object(fields));
                    }
                    if !fields.is_empty() {
                        *rest = rest.strip_prefix(',')?.trim_start();
                    }
                    let name = Json::string(rest)?;
                    *rest = rest.trim_start().strip_prefix(':')?;
                    fields.push((name, Json::value(rest, depth + 1)?));
                }
            }
            _ => {
                let end = rest
                    .find(|c: char| !(c.is_ascii_digit() || "+-.eE".contains(c)))
                    .unwrap_or(rest.len());
                let number = rest[..end].parse().ok()?;
                *rest = &rest[end..];
                Some(Json::Number(number))
            }
        }
    }

    fn string(rest: &mut &str) -> Option<String> {
        let mut text = String::new();
        let mut characters = rest.strip_prefix('"')?.char_indices();
        let body = &rest[1..];
        while let Some((at, character)) = characters.next() {
            match character {
                '"' => {
                    *rest = &body[at + 1..];
                    return Some(text);
                }
                '\\' => match characters.next()?.1 {
                    'n' => text.push('\n'),
                    't' => text.push('\t'),
                    'r' => text.push('\r'),
                    'b' => text.push('\u{8}'),
                    'f' => text.push('\u{c}'),
                    'u' => {
                        let unit = |characters: &mut std::str::CharIndices<'_>| {
                            let digits: String = (0..4)
                                .filter_map(|_| characters.next())
                                .map(|(_, digit)| digit)
                                .collect();
                            u32::from_str_radix(&digits, 16).ok()
                        };
                        let first = unit(&mut characters)?;
                        // Two escapes can be the two halves of one character.
                        let code = if (0xd800..0xdc00).contains(&first) {
                            characters.next().filter(|(_, c)| *c == '\\')?;
                            characters.next().filter(|(_, c)| *c == 'u')?;
                            let second = unit(&mut characters)?.checked_sub(0xdc00)?;
                            0x10000 + ((first - 0xd800) << 10) + second
                        } else {
                            first
                        };
                        text.push(char::from_u32(code)?);
                    }
                    other => text.push(other),
                },
                other => text.push(other),
            }
        }
        None
    }
}
