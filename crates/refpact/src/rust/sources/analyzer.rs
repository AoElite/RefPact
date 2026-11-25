//! rust-analyzer as a language server, which gives the expansion of a macro.

use std::collections::hash_map::DefaultHasher;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex};

use super::json::Json;
use super::{span, Disk, INDEX_LIMIT, RUST_ANALYZER};
use crate::rust::workers;

/// The expansions of some macros, by the place of the macro: the file, the line and the
/// column.
pub(crate) type Expansions = HashMap<(String, u32, u32), String>;

/// Asks rust-analyzer for the expansion of the macro at each place of the directory. A place
/// is a file, a line and a column, both counted from 1. A macro that rust-analyzer does not
/// expand is not in the result.
///
/// The expansions are kept beside the index of the directory, and are read from there while
/// the `inputs` and the version of the program stay the same.
pub(crate) fn expansions(
    disk: &Disk,
    inputs: &[String],
    places: &[(String, u32, u32)],
) -> Result<Expansions, String> {
    if places.is_empty() {
        return Ok(Expansions::new());
    }
    let mut inputs = inputs.to_vec();
    inputs.extend(["Cargo.toml", "Cargo.lock"].map(str::to_owned));
    inputs.sort();
    inputs.dedup();
    let cache = disk.kept_in();
    // The expansions of one directory have one file: `<directory>-<state>.expansions`.
    let base = std::fs::canonicalize(&disk.base).unwrap_or_else(|_| disk.base.clone());
    let mut place = DefaultHasher::new();
    base.hash(&mut place);
    let own = format!("{:016x}-", place.finish());
    let state = disk.state(RUST_ANALYZER, &inputs);
    let name = format!("{own}{state:016x}.expansions");
    if let Some(kept) = std::fs::read_to_string(cache.join(&name))
        .ok()
        .and_then(read_kept)
    {
        return Ok(kept);
    }
    let found = ask(disk, places)?;
    // Without the directory, the expansions are made and not kept.
    if std::fs::create_dir_all(&cache).is_ok() {
        for entry in std::fs::read_dir(&cache).into_iter().flatten().flatten() {
            let kept = entry.file_name();
            let kept = kept.to_string_lossy();
            if kept.starts_with(&own) && kept.ends_with(".expansions") {
                let _ = std::fs::remove_file(entry.path());
            }
        }
        let part = cache.join(format!("{}.expansions.part", std::process::id()));
        if std::fs::write(&part, write_kept(&found)).is_err()
            || std::fs::rename(&part, cache.join(&name)).is_err()
        {
            let _ = std::fs::remove_file(&part);
        }
    }
    Ok(found)
}

/// The expansions as a text: for each, a line `<line> <column> <bytes> <file>`, and then
/// the expansion and a line break.
fn write_kept(expansions: &Expansions) -> String {
    let mut entries: Vec<_> = expansions.iter().collect();
    entries.sort();
    let mut text = String::new();
    for ((file, line, column), expansion) in entries {
        text.push_str(&format!(
            "{line} {column} {} {file}\n{expansion}\n",
            expansion.len()
        ));
    }
    text
}

fn read_kept(text: String) -> Option<Expansions> {
    let mut found = Expansions::new();
    let mut rest = text.as_str();
    while !rest.is_empty() {
        let (head, after) = rest.split_once('\n')?;
        let mut parts = head.splitn(4, ' ');
        let line = parts.next()?.parse().ok()?;
        let column = parts.next()?.parse().ok()?;
        let length: usize = parts.next()?.parse().ok()?;
        let file = parts.next()?;
        let expansion = after.get(..length)?;
        rest = after.get(length..)?.strip_prefix('\n')?;
        found.insert((file.to_owned(), line, column), expansion.to_owned());
    }
    Some(found)
}

/// Runs rust-analyzer as a language server on the directory and asks it for each expansion.
fn ask(disk: &Disk, places: &[(String, u32, u32)]) -> Result<Expansions, String> {
    let base = std::fs::canonicalize(&disk.base).unwrap_or_else(|_| disk.base.clone());
    let uri = |file: &str| {
        let path = base.join(file);
        let mut uri = String::from("file://");
        for byte in path.to_string_lossy().bytes() {
            match byte {
                b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' => {
                    uri.push(char::from(byte));
                }
                other => uri.push_str(&format!("%{other:02X}")),
            }
        }
        uri
    };
    let mut server = Server::start()?;
    let root = json_text(&uri(""));
    server.request(
        "initialize",
        &format!(
            "{{\"processId\":{},\"rootUri\":{root},\"capabilities\":{{\"experimental\":\
             {{\"serverStatusNotification\":true}}}},\"initializationOptions\":{{\
             \"procMacro\":{{\"enable\":true}},\"cargo\":{{\"buildScripts\":{{\"enable\":true}}}},\
             \"checkOnSave\":false,\"cachePriming\":{{\"enable\":false}}}}}}",
            std::process::id()
        ),
    )?;
    server.notify("initialized", "{}")?;
    server.until_ready()?;

    let mut found = Expansions::new();
    // The lines of each file, for the columns that the server counts in UTF-16.
    let mut lines: HashMap<&str, Vec<String>> = HashMap::new();
    for (file, line, column) in places {
        let text = lines.entry(file.as_str()).or_insert_with(|| {
            let text = std::fs::read_to_string(base.join(file)).unwrap_or_default();
            text.lines().map(str::to_owned).collect()
        });
        let before = text.get(*line as usize - 1).map_or("", String::as_str);
        let before = before.chars().take(*column as usize - 1);
        let character: usize = before.map(char::len_utf16).sum();
        let reply = server.request(
            "rust-analyzer/expandMacro",
            &format!(
                "{{\"textDocument\":{{\"uri\":{}}},\"position\":{{\"line\":{},\"character\":{character}}}}}",
                json_text(&uri(file)),
                line - 1
            ),
        )?;
        if let Some(expansion) = reply.get("expansion").and_then(Json::text) {
            found.insert((file.clone(), *line, *column), expansion.to_owned());
        }
    }
    server.stop();
    Ok(found)
}

/// rust-analyzer as a language server: messages of JSON, each after a header with its length.
struct Server {
    child: Arc<Mutex<Child>>,
    input: ChildStdin,
    output: BufReader<ChildStdout>,
    requests: u32,
    /// The server took longer than [`INDEX_LIMIT`] and was stopped.
    late: Arc<AtomicBool>,
    /// While this is there, a thread waits for the limit. It stops the server then, so
    /// that no wait for a message is without end.
    _watched: Sender<()>,
}

impl Server {
    fn start() -> Result<Server, String> {
        let mut child = Command::new(RUST_ANALYZER)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|error| {
                format!(
                    "`{RUST_ANALYZER}` cannot be started ({error}); install it with \
                     `rustup component add rust-analyzer`"
                )
            })?;
        let (Some(input), Some(output)) = (child.stdin.take(), child.stdout.take()) else {
            return Err(format!("`{RUST_ANALYZER}` has no input and output"));
        };
        let child = Arc::new(Mutex::new(child));
        let late = Arc::new(AtomicBool::new(false));
        let (watched, done) = mpsc::channel::<()>();
        let (stopped, slow) = (Arc::clone(&child), Arc::clone(&late));
        workers::in_background(move || {
            if done.recv_timeout(INDEX_LIMIT) == Err(RecvTimeoutError::Timeout) {
                slow.store(true, Ordering::Relaxed);
                if let Ok(mut child) = stopped.lock() {
                    let _ = child.kill();
                }
            }
        });
        Ok(Server {
            child,
            input,
            output: BufReader::new(output),
            requests: 0,
            late,
            _watched: watched,
        })
    }

    fn send(&mut self, message: &str) -> Result<(), String> {
        let sent = write!(
            self.input,
            "Content-Length: {}\r\n\r\n{message}",
            message.len()
        )
        .and_then(|()| self.input.flush());
        sent.map_err(|error| format!("`{RUST_ANALYZER}` takes no message ({error})"))
    }

    fn notify(&mut self, method: &str, params: &str) -> Result<(), String> {
        self.send(&format!(
            "{{\"jsonrpc\":\"2.0\",\"method\":\"{method}\",\"params\":{params}}}"
        ))
    }

    /// Sends a request and gives the result of its reply.
    fn request(&mut self, method: &str, params: &str) -> Result<Json, String> {
        self.requests += 1;
        let id = self.requests;
        self.send(&format!(
            "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"method\":\"{method}\",\"params\":{params}}}"
        ))?;
        loop {
            let message = self.receive()?;
            let is_reply = message.get("method").is_none()
                && message.get("id").and_then(Json::number) == Some(f64::from(id));
            if is_reply {
                return Ok(message.take("result").unwrap_or(Json::Null));
            }
        }
    }

    /// Reads one message. A request of the server gets an empty answer.
    fn receive(&mut self) -> Result<Json, String> {
        let late = Arc::clone(&self.late);
        let gone = move || match late.load(Ordering::Relaxed) {
            true => format!(
                "`{RUST_ANALYZER}` did not give the expansions in {} and was stopped",
                span(INDEX_LIMIT)
            ),
            false => format!("`{RUST_ANALYZER}` stopped before it gave the expansions"),
        };
        let mut length = None;
        loop {
            let mut header = String::new();
            if self.output.read_line(&mut header).map_err(|_| gone())? == 0 {
                return Err(gone());
            }
            let header = header.trim();
            if header.is_empty() {
                break;
            }
            if let Some(value) = header.strip_prefix("Content-Length:") {
                length = value.trim().parse::<usize>().ok();
            }
        }
        let mut body = vec![0; length.ok_or_else(&gone)?];
        self.output.read_exact(&mut body).map_err(|_| gone())?;
        let text = String::from_utf8(body).map_err(|_| gone())?;
        let message = Json::parse(&text).ok_or_else(&gone)?;
        if let (Some(_), Some(id)) = (message.get("method"), message.get("id")) {
            let id = match id {
                Json::Text(id) => json_text(id),
                other => other
                    .number()
                    .map_or("null".to_owned(), |id| id.to_string()),
            };
            self.send(&format!(
                "{{\"jsonrpc\":\"2.0\",\"id\":{id},\"result\":null}}"
            ))?;
        }
        Ok(message)
    }

    /// Waits until the server has read the project.
    fn until_ready(&mut self) -> Result<(), String> {
        loop {
            let message = self.receive()?;
            let is_status =
                message.get("method").and_then(Json::text) == Some("experimental/serverStatus");
            let params = message.get("params");
            if is_status && params.and_then(|p| p.get("quiescent")) == Some(&Json::Bool(true)) {
                if params.and_then(|p| p.get("health")).and_then(Json::text) == Some("error") {
                    let said = params.and_then(|p| p.get("message")).and_then(Json::text);
                    let said = said.and_then(|text| text.lines().next());
                    return Err(format!(
                        "`{RUST_ANALYZER}` cannot read the project: {}",
                        said.unwrap_or("it gave no message")
                    ));
                }
                return Ok(());
            }
        }
    }

    fn stop(mut self) {
        let _ = self.request("shutdown", "null");
        let _ = self.notify("exit", "null");
        if let Ok(mut child) = self.child.lock() {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

/// A text as JSON.
fn json_text(text: &str) -> String {
    let mut json = String::from("\"");
    for character in text.chars() {
        match character {
            '"' => json.push_str("\\\""),
            '\\' => json.push_str("\\\\"),
            control if (control as u32) < 0x20 => {
                json.push_str(&format!("\\u{:04x}", control as u32));
            }
            other => json.push(other),
        }
    }
    json.push('"');
    json
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_message_of_the_server_is_read() {
        let message = r#"{"jsonrpc":"2.0","id":3,"result":{"name":"Stored","expansion":"impl A {\n    fn f() -> \"x\\y\" { é😀 }\n}","list":[1,-2.5e1,true,null,{}]}}"#;
        let message = Json::parse(message).unwrap();
        assert_eq!(message.get("id").and_then(Json::number), Some(3.0));
        let result = message.get("result").unwrap();
        assert_eq!(
            result.get("expansion").and_then(Json::text),
            Some("impl A {\n    fn f() -> \"x\\y\" { \u{e9}\u{1f600} }\n}")
        );
        let list = Json::List(vec![
            Json::Number(1.0),
            Json::Number(-25.0),
            Json::Bool(true),
            Json::Null,
            Json::Object(Vec::new()),
        ]);
        assert_eq!(result.get("list"), Some(&list));
        assert_eq!(Json::parse("{\"a\":1} x"), None);
        assert_eq!(Json::parse("{\"a\""), None);
        assert_eq!(json_text("a\"b\\c\n"), "\"a\\\"b\\\\c\\u000a\"");
    }

    #[test]
    fn the_expansions_that_are_kept_are_read_again() {
        let mut expansions = Expansions::new();
        expansions.insert(("src/a b.rs".to_owned(), 3, 10), "impl A {\n}\n".to_owned());
        expansions.insert(("src/lib.rs".to_owned(), 1, 1), String::new());
        let kept = write_kept(&expansions);
        assert_eq!(read_kept(kept.clone()), Some(expansions));
        assert_eq!(read_kept(kept[..kept.len() - 3].to_owned()), None);
    }
}
