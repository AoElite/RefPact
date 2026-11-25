//! A small JSON reader and writer, so that the core needs no dependency.

use std::borrow::Cow;
use std::fmt::Write as _;

/// One JSON value. Objects keep their keys in the order of the text. A string borrows the
/// text that it was read from when it has no escape, and the data that it is written from.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Value<'a> {
    Null,
    Bool(bool),
    Number(f64),
    String(Cow<'a, str>),
    Array(Vec<Value<'a>>),
    Object(Vec<(Cow<'a, str>, Value<'a>)>),
}

impl<'a> Value<'a> {
    /// The value of a key of an object. `None` when the key is absent or the value is not an
    /// object.
    pub fn get(&self, key: &str) -> Option<&Value<'a>> {
        match self {
            Value::Object(fields) => fields
                .iter()
                .find(|(name, _)| name == key)
                .map(|(_, value)| value),
            _ => None,
        }
    }

    /// The JSON text of the value.
    pub fn to_text(&self) -> String {
        let mut text = String::new();
        self.write(&mut text);
        text
    }

    fn write(&self, text: &mut String) {
        match self {
            Value::Null => text.push_str("null"),
            Value::Bool(value) => text.push_str(if *value { "true" } else { "false" }),
            Value::Number(value) => {
                if value.fract() == 0.0 && value.abs() < 1e15 {
                    let _ = write!(text, "{}", *value as i64);
                } else {
                    let _ = write!(text, "{value}");
                }
            }
            Value::String(value) => write_string(value, text),
            Value::Array(items) => {
                text.push('[');
                for (index, item) in items.iter().enumerate() {
                    if index > 0 {
                        text.push(',');
                    }
                    item.write(text);
                }
                text.push(']');
            }
            Value::Object(fields) => {
                text.push('{');
                for (index, (key, value)) in fields.iter().enumerate() {
                    if index > 0 {
                        text.push(',');
                    }
                    write_string(key, text);
                    text.push(':');
                    value.write(text);
                }
                text.push('}');
            }
        }
    }
}

#[cfg(test)]
impl Value<'_> {
    /// The value with its own strings, for tests that keep a value beyond its text.
    pub fn into_owned(self) -> Value<'static> {
        match self {
            Value::Null => Value::Null,
            Value::Bool(value) => Value::Bool(value),
            Value::Number(value) => Value::Number(value),
            Value::String(text) => Value::String(Cow::Owned(text.into_owned())),
            Value::Array(items) => Value::Array(items.into_iter().map(Value::into_owned).collect()),
            Value::Object(fields) => Value::Object(
                fields
                    .into_iter()
                    .map(|(key, value)| (Cow::Owned(key.into_owned()), value.into_owned()))
                    .collect(),
            ),
        }
    }
}

fn write_string(value: &str, text: &mut String) {
    text.push('"');
    for character in value.chars() {
        match character {
            '"' => text.push_str("\\\""),
            '\\' => text.push_str("\\\\"),
            '\n' => text.push_str("\\n"),
            '\r' => text.push_str("\\r"),
            '\t' => text.push_str("\\t"),
            control if (control as u32) < 0x20 => {
                let _ = write!(text, "\\u{:04x}", control as u32);
            }
            other => text.push(other),
        }
    }
    text.push('"');
}

/// The deepest nesting that the reader accepts. The wire format nests about ten levels.
const MAX_DEPTH: usize = 128;

/// Reads one JSON value that fills the whole text.
#[cfg(test)]
pub(crate) fn parse(text: &str) -> Result<Value<'_>, String> {
    let mut reader = Reader {
        text,
        bytes: text.as_bytes(),
        position: 0,
        depth: 0,
    };
    let value = reader.value()?;
    reader.skip_space();
    if reader.position != reader.bytes.len() {
        return Err(reader.error("text after the value"));
    }
    Ok(value)
}

/// A reader that the caller drives, to decode a large value without the tree of [`Value`].
pub(crate) fn reader(text: &str) -> Reader<'_> {
    Reader {
        text,
        bytes: text.as_bytes(),
        position: 0,
        depth: 0,
    }
}

pub(crate) struct Reader<'t> {
    text: &'t str,
    bytes: &'t [u8],
    position: usize,
    depth: usize,
}

impl<'t> Reader<'t> {
    /// Checks that only space follows the value.
    pub fn finish(&mut self) -> Result<(), String> {
        self.skip_space();
        if self.position == self.bytes.len() {
            Ok(())
        } else {
            Err(self.error("text after the value"))
        }
    }

    /// Reads an object, and calls `each` with the reader at the value of each key.
    pub fn fields(
        &mut self,
        mut each: impl FnMut(&mut Self, Cow<'t, str>) -> Result<(), String>,
    ) -> Result<(), String> {
        match self.peek() {
            Some(b'{') => {}
            None => return Err(self.error("the text ends")),
            Some(_) => return Err("expected an object".to_owned()),
        }
        self.enter()?;
        self.position += 1;
        if self.peek() == Some(b'}') {
            self.position += 1;
            self.depth -= 1;
            return Ok(());
        }
        loop {
            if self.peek() != Some(b'"') {
                return Err(self.error("expected a key"));
            }
            let key = self.string()?;
            self.expect(b':')?;
            each(self, key)?;
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b'}') => {
                    self.position += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(self.error("expected `,` or `}`")),
            }
        }
    }

    /// Reads a list, and calls `each` with the reader at each item and its position.
    pub fn items(
        &mut self,
        mut each: impl FnMut(&mut Self, usize) -> Result<(), String>,
    ) -> Result<(), String> {
        match self.peek() {
            Some(b'[') => {}
            None => return Err(self.error("the text ends")),
            Some(_) => return Err("expected a list".to_owned()),
        }
        self.enter()?;
        self.position += 1;
        if self.peek() == Some(b']') {
            self.position += 1;
            self.depth -= 1;
            return Ok(());
        }
        let mut index = 0;
        loop {
            each(self, index)?;
            index += 1;
            match self.peek() {
                Some(b',') => self.position += 1,
                Some(b']') => {
                    self.position += 1;
                    self.depth -= 1;
                    return Ok(());
                }
                _ => return Err(self.error("expected `,` or `]`")),
            }
        }
    }

    fn enter(&mut self) -> Result<(), String> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return Err(self.error("the values nest too deep"));
        }
        Ok(())
    }

    /// Reads a string.
    pub fn text(&mut self) -> Result<Cow<'t, str>, String> {
        match self.peek() {
            Some(b'"') => self.string(),
            None => Err(self.error("the text ends")),
            Some(_) => Err("expected a string".to_owned()),
        }
    }

    /// Reads a number that is whole and fits 32 bits.
    pub fn whole(&mut self) -> Result<u32, String> {
        match self.peek() {
            Some(b'-' | b'0'..=b'9') => match self.number()? {
                Value::Number(number)
                    if number.fract() == 0.0 && number >= 0.0 && number <= f64::from(u32::MAX) =>
                {
                    Ok(number as u32)
                }
                _ => Err("expected a whole number".to_owned()),
            },
            None => Err(self.error("the text ends")),
            Some(_) => Err("expected a whole number".to_owned()),
        }
    }

    /// Reads `null` when it comes next, and says whether it did.
    pub fn null(&mut self) -> bool {
        if self.peek() == Some(b'n') && self.bytes[self.position..].starts_with(b"null") {
            self.position += 4;
            true
        } else {
            false
        }
    }

    /// Reads any value as a tree.
    pub fn any(&mut self) -> Result<Value<'t>, String> {
        self.value()
    }

    fn error(&self, message: &str) -> String {
        format!("invalid JSON at byte {}: {message}", self.position)
    }

    fn skip_space(&mut self) {
        while let Some(b' ' | b'\t' | b'\n' | b'\r') = self.bytes.get(self.position) {
            self.position += 1;
        }
    }

    fn peek(&mut self) -> Option<u8> {
        self.skip_space();
        self.bytes.get(self.position).copied()
    }

    fn expect(&mut self, byte: u8) -> Result<(), String> {
        if self.peek() == Some(byte) {
            self.position += 1;
            Ok(())
        } else {
            Err(self.error(&format!("expected `{}`", byte as char)))
        }
    }

    fn literal(&mut self, word: &str, value: Value<'t>) -> Result<Value<'t>, String> {
        if self.bytes[self.position..].starts_with(word.as_bytes()) {
            self.position += word.len();
            Ok(value)
        } else {
            Err(self.error("unknown word"))
        }
    }

    fn value(&mut self) -> Result<Value<'t>, String> {
        match self.peek() {
            None => Err(self.error("the text ends")),
            Some(b'{') => self.object(),
            Some(b'[') => self.array(),
            Some(b'"') => self.string().map(Value::String),
            Some(b't') => self.literal("true", Value::Bool(true)),
            Some(b'f') => self.literal("false", Value::Bool(false)),
            Some(b'n') => self.literal("null", Value::Null),
            Some(b'-' | b'0'..=b'9') => self.number(),
            Some(_) => Err(self.error("unexpected character")),
        }
    }

    fn object(&mut self) -> Result<Value<'t>, String> {
        let mut fields = Vec::new();
        self.fields(|reader, key| {
            fields.push((key, reader.value()?));
            Ok(())
        })?;
        Ok(Value::Object(fields))
    }

    fn array(&mut self) -> Result<Value<'t>, String> {
        let mut items = Vec::new();
        self.items(|reader, _| {
            items.push(reader.value()?);
            Ok(())
        })?;
        Ok(Value::Array(items))
    }

    fn string(&mut self) -> Result<Cow<'t, str>, String> {
        // The caller has seen the quote.
        self.position += 1;
        let start = self.position;
        // Most strings have no escape: they borrow the text. A run of plain bytes ends only
        // at an ASCII byte, so each run is a slice of the text at character boundaries.
        let mut owned: Option<String> = None;
        let mut run = start;
        loop {
            while let Some(&byte) = self.bytes.get(self.position) {
                if byte == b'"' || byte == b'\\' || byte < 0x20 {
                    break;
                }
                self.position += 1;
            }
            let plain = &self.text[run..self.position];
            match self.bytes.get(self.position) {
                Some(b'"') => {
                    self.position += 1;
                    return Ok(match owned {
                        None => Cow::Borrowed(plain),
                        Some(mut text) => {
                            text.push_str(plain);
                            Cow::Owned(text)
                        }
                    });
                }
                Some(b'\\') => {
                    let text = owned.get_or_insert_with(String::new);
                    text.push_str(plain);
                    self.position += 1;
                    let escape = self.bytes.get(self.position).copied();
                    self.position += 1;
                    let character = match escape {
                        Some(b'"') => '"',
                        Some(b'\\') => '\\',
                        Some(b'/') => '/',
                        Some(b'b') => '\u{8}',
                        Some(b'f') => '\u{c}',
                        Some(b'n') => '\n',
                        Some(b'r') => '\r',
                        Some(b't') => '\t',
                        Some(b'u') => self.unicode()?,
                        _ => return Err(self.error("unknown escape")),
                    };
                    owned.get_or_insert_with(String::new).push(character);
                    run = self.position;
                }
                Some(_) => return Err(self.error("a control character in a string")),
                None => return Err(self.error("the string does not end")),
            }
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        let digits = self
            .bytes
            .get(self.position..self.position + 4)
            .and_then(|digits| std::str::from_utf8(digits).ok())
            .and_then(|digits| u32::from_str_radix(digits, 16).ok())
            .ok_or_else(|| self.error("expected four hex digits"))?;
        self.position += 4;
        Ok(digits)
    }

    /// The character of a `\u` escape, with the second half of a surrogate pair.
    fn unicode(&mut self) -> Result<char, String> {
        let first = self.hex4()?;
        let code = if (0xD800..0xDC00).contains(&first) {
            if self.bytes.get(self.position..self.position + 2) != Some(b"\\u") {
                return Err(self.error("a lone surrogate"));
            }
            self.position += 2;
            let second = self.hex4()?;
            if !(0xDC00..0xE000).contains(&second) {
                return Err(self.error("a lone surrogate"));
            }
            0x10000 + ((first - 0xD800) << 10) + (second - 0xDC00)
        } else {
            first
        };
        char::from_u32(code).ok_or_else(|| self.error("a lone surrogate"))
    }

    fn number(&mut self) -> Result<Value<'t>, String> {
        let start = self.position;
        while let Some(b'-' | b'+' | b'.' | b'e' | b'E' | b'0'..=b'9') =
            self.bytes.get(self.position)
        {
            self.position += 1;
        }
        std::str::from_utf8(&self.bytes[start..self.position])
            .ok()
            .and_then(|text| text.parse::<f64>().ok())
            .filter(|value| value.is_finite())
            .map(Value::Number)
            .ok_or_else(|| self.error("invalid number"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn values_read_and_written() {
        let text =
            r#" {"a": [1, -2.5, true, false, null], "b": {"c": "x\"\\\n\u00e9\ud83d\ude00/"}} "#;
        let value = parse(text).unwrap();
        assert_eq!(
            value.get("b").and_then(|b| b.get("c")),
            Some(&Value::String("x\"\\\né😀/".into()))
        );
        assert_eq!(
            value.to_text(),
            r#"{"a":[1,-2.5,true,false,null],"b":{"c":"x\"\\\né😀/"}}"#
        );
        assert_eq!(parse(&value.to_text()).unwrap(), value);
        assert_eq!(Value::String("\u{1}".into()).to_text(), "\"\\u0001\"");
    }

    #[test]
    fn invalid_text() {
        for text in [
            "",
            "{",
            "[1,]",
            "{\"a\" 1}",
            "\"open",
            "\"\\x\"",
            "\"\\ud800\"",
            "nul",
            "1 2",
            "\"tab\there\"",
            "{1: 2}",
            "-",
        ] {
            assert!(parse(text).is_err(), "{text}");
        }
        let deep = "[".repeat(200) + &"]".repeat(200);
        assert!(parse(&deep).unwrap_err().contains("nest too deep"));
        let fine = "[".repeat(100) + &"]".repeat(100);
        assert!(parse(&fine).is_ok());
    }
}
