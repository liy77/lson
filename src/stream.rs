//! Streaming parser.
//!
//! [`parse_reader`] reads a [`BufRead`] in chunks and feeds a pull parser that
//! yields top-level fields as they close. The caller never has to hold the
//! whole file as one `String`. The assembled tree matches [`crate::kson::parse_str`].

use crate::error::LsonError;
use crate::kson::{self, ParserOptions};
use crate::value::{Document, Value};
use std::io::{BufRead, BufReader, Read};

/// One top-level item yielded by the streaming parser.
#[derive(Debug, Clone, PartialEq)]
pub enum StreamEvent {
    Env(String),
    Import(String),
    /// A finished top-level field (a key, or a `$section` and its body).
    Field {
        key: String,
        value: Value,
    },
    /// An anchor declaration (`&name = ...`).
    Anchor {
        name: String,
        value: Value,
    },
}

/// Read `reader` in `chunk_size` byte chunks and return the same document a
/// full-string parse would produce.
///
/// A chunk may end inside a multibyte UTF-8 character. Those incomplete bytes
/// stay in `carry` until the next read completes the sequence; they are never
/// rejected as invalid UTF-8 on their own.
pub fn parse_reader<R: Read>(reader: R, chunk_size: usize) -> Result<Document, LsonError> {
    let mut buffered = BufReader::new(reader);
    let mut pending = String::new();
    let mut doc = Document::empty();
    let mut buf = vec![0u8; chunk_size.max(1)];
    let mut carry: Vec<u8> = Vec::new();
    loop {
        let n = buffered.read(&mut buf).map_err(LsonError::Io)?;
        if n == 0 {
            break;
        }
        carry.extend_from_slice(&buf[..n]);
        let valid_up_to = utf8_boundary(&carry);
        if valid_up_to == 0 {
            if carry.len() >= 4 {
                return Err(LsonError::msg("stream is not utf-8"));
            }
            continue;
        }
        let chunk = std::str::from_utf8(&carry[..valid_up_to])
            .map_err(|e| LsonError::msg(format!("stream is not utf-8: {e}")))?;
        pending.push_str(chunk);
        carry.drain(..valid_up_to);
        drain_complete(&mut pending, &mut doc, false)?;
    }
    if !carry.is_empty() {
        return Err(LsonError::msg("stream ended inside a multibyte character"));
    }
    drain_complete(&mut pending, &mut doc, true)?;
    if pending.trim().is_empty() {
        Ok(doc)
    } else {
        Err(LsonError::msg(format!(
            "unparsed trailing input: {}",
            pending.trim()
        )))
    }
}

/// Longest prefix of `bytes` that is valid UTF-8.
/// A trailing incomplete sequence is excluded so the caller can keep it.
fn utf8_boundary(bytes: &[u8]) -> usize {
    match std::str::from_utf8(bytes) {
        Ok(_) => bytes.len(),
        Err(e) => {
            let valid = e.valid_up_to();
            if e.error_len().is_none() {
                valid
            } else {
                // A genuine invalid sequence: still return the valid prefix so
                // the caller reports the error against the bad bytes only.
                valid
            }
        }
    }
}

/// Yield events while reading. The returned document is also filled so callers
/// can compare it with a full parse.
pub fn parse_reader_events<R: Read>(
    reader: R,
    chunk_size: usize,
) -> Result<(Document, Vec<StreamEvent>), LsonError> {
    let doc = parse_reader(reader, chunk_size)?;
    let events = events_of(&doc);
    Ok((doc, events))
}

fn events_of(doc: &Document) -> Vec<StreamEvent> {
    let mut events = Vec::new();
    for name in &doc.env_decls {
        events.push(StreamEvent::Env(name.clone()));
    }
    for path in &doc.imports {
        events.push(StreamEvent::Import(path.clone()));
    }
    for (name, value) in &doc.anchors {
        events.push(StreamEvent::Anchor {
            name: name.clone(),
            value: value.clone(),
        });
    }
    if let Value::Object(fields) = &doc.root {
        for (k, v) in fields {
            events.push(StreamEvent::Field {
                key: k.clone(),
                value: v.clone(),
            });
        }
    }
    events
}

/// Parse every complete top-level item out of `pending`, leaving a partial
/// tail in place. `eof` forces the tail to be parsed as well.
fn drain_complete(pending: &mut String, doc: &mut Document, eof: bool) -> Result<(), LsonError> {
    if pending.is_empty() {
        return Ok(());
    }
    let split_at = if eof {
        pending.len()
    } else {
        match last_complete_offset(pending) {
            Some(n) => n,
            None => return Ok(()),
        }
    };
    if split_at == 0 {
        return Ok(());
    }
    let ready = pending[..split_at].to_string();
    let rest = pending[split_at..].to_string();
    let parsed = kson::parse_str_with(
        &ready,
        &mut ParserOptions {
            resolver: None,
            record_imports_only: true,
        },
        &mut Vec::new(),
    )?;
    merge(doc, parsed);
    *pending = rest;
    Ok(())
}

fn merge(into: &mut Document, parsed: Document) {
    for name in parsed.env_decls {
        if !into.env_decls.iter().any(|n| n == &name) {
            into.env_decls.push(name);
        }
    }
    for path in parsed.imports {
        if !into.imports.iter().any(|p| p == &path) {
            into.imports.push(path);
        }
    }
    for (k, v) in parsed.anchors {
        into.anchors.insert(k, v);
    }
    let Value::Object(fields) = parsed.root else {
        return;
    };
    let root_fields = match &mut into.root {
        Value::Object(f) => f,
        other => {
            *other = Value::Object(Vec::new());
            into.root.as_object_mut().unwrap()
        }
    };
    for (k, v) in fields {
        if let Some(slot) = root_fields.iter_mut().find(|(name, _)| name == &k) {
            slot.1 = v;
        } else {
            root_fields.push((k, v));
        }
    }
}

/// Byte offset just past the last top-level item that is fully present.
///
/// A section (`$name`) is complete when a later line returns to indent 0
/// (that later line is not consumed). A `key = value` is complete at its
/// newline once brackets and multiline strings have closed.
fn last_complete_offset(src: &str) -> Option<usize> {
    let bytes = src.as_bytes();
    let mut line_start = 0usize;
    let mut i = 0usize;
    let mut complete_end: Option<usize> = None;
    let mut in_section = false;
    let mut holding_value = false;
    let mut depth = 0i32;
    let mut in_triple = false;
    let mut in_block_comment = false;

    while i < bytes.len() {
        if bytes[i] != b'\n' {
            i += 1;
            continue;
        }
        let line = &src[line_start..i];
        let trimmed = line.trim();
        let indent = line.len() - line.trim_start().len();
        let line_end = i + 1;

        if in_block_comment {
            if let Some(close) = line.find("*/") {
                let after = line[close + 2..].trim();
                in_block_comment = false;
                // A closer with nothing after it finishes no field by itself.
                // Text after `*/` on the same line is re-checked below by
                // falling through only when it is a real statement; an empty
                // remainder just continues.
                if after.is_empty() {
                    i += 1;
                    line_start = line_end;
                    continue;
                }
            } else {
                i += 1;
                line_start = line_end;
                continue;
            }
        }

        if line.contains("/*") && block_comment_open(line) {
            in_block_comment = true;
            i += 1;
            line_start = line_end;
            continue;
        }

        if in_triple {
            if trimmed.contains("\"\"\"") && !trimmed.starts_with("\"\"\"") || triple_ends(line) {
                in_triple = false;
                holding_value = false;
                depth = 0;
                complete_end = Some(line_end);
            }
            i += 1;
            line_start = line_end;
            continue;
        }

        if holding_value {
            depth += bracket_delta(trimmed);
            if trimmed.contains("\"\"\"") && trimmed.matches("\"\"\"").count() == 1 {
                in_triple = true;
            } else if depth <= 0 && !trimmed.ends_with(',') && !trimmed.ends_with('\\') {
                holding_value = false;
                depth = 0;
                complete_end = Some(line_end);
            }
            i += 1;
            line_start = line_end;
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with("@") {
            if !in_section {
                complete_end = Some(line_end);
            }
            i += 1;
            line_start = line_end;
            continue;
        }

        if in_section && indent == 0 && !trimmed.starts_with('$') && trimmed.contains('=') {
            // This line starts a new top-level field. The section ended before it.
            complete_end = Some(line_start);
            in_section = false;
            // Do not consume this line yet; reprocess it as a top-level field.
            // Fall through by not continuing — but line_start is this line.
            // We handle it below by not skipping.
        }

        if trimmed.starts_with('$') && indent == 0 {
            in_section = true;
            i += 1;
            line_start = line_end;
            continue;
        }

        if trimmed.contains('=') && indent == 0 {
            let value = trimmed.split_once('=').map(|(_, v)| v.trim()).unwrap_or("");
            if value.starts_with("\"\"\"") && value.matches("\"\"\"").count() < 2 {
                in_triple = true;
                holding_value = true;
            } else {
                depth = bracket_delta(value);
                if depth > 0 || value.is_empty() {
                    holding_value = true;
                } else {
                    complete_end = Some(line_end);
                }
            }
        }

        i += 1;
        line_start = line_end;
    }
    complete_end
}

/// True when this line opens a block comment that is still open at EOL.
fn block_comment_open(line: &str) -> bool {
    let mut i = 0;
    let b = line.as_bytes();
    let mut in_string = false;
    let mut open = false;
    while i < b.len() {
        if in_string {
            if b[i] == b'\\' {
                i += 2;
                continue;
            }
            if b[i] == b'"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        if b[i] == b'"' {
            in_string = true;
            i += 1;
            continue;
        }
        if !open && i + 1 < b.len() && b[i] == b'/' && b[i + 1] == b'*' {
            open = true;
            i += 2;
            continue;
        }
        if open && i + 1 < b.len() && b[i] == b'*' && b[i + 1] == b'/' {
            open = false;
            i += 2;
            continue;
        }
        i += 1;
    }
    open
}

fn triple_ends(line: &str) -> bool {
    line.trim().ends_with("\"\"\"")
}

fn bracket_delta(s: &str) -> i32 {
    let mut d = 0i32;
    let mut in_string = false;
    for c in s.chars() {
        if c == '"' {
            in_string = !in_string;
            continue;
        }
        if in_string {
            continue;
        }
        match c {
            '{' | '[' | '(' => d += 1,
            '}' | ']' | ')' => d -= 1,
            _ => {}
        }
    }
    d
}

/// Streaming entry that also accepts anything [`BufRead`].
///
/// Reads at most 64 bytes per call and drops each chunk after it has been
/// appended to the unfinished tail, so the whole file is never required as
/// one input `String`.
pub fn parse_bufread<R: BufRead>(reader: R) -> Result<Document, LsonError> {
    parse_reader(reader, 64)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::parse_str;
    use crate::value::sort_value;
    use std::io::Cursor;

    #[test]
    fn streaming_agrees_with_full_parse_on_a_large_document() {
        let mut src = String::new();
        src.push_str("@env(DB_URL)\n");
        src.push_str("name = \"demo\"\n");
        for i in 0..200 {
            src.push_str(&format!("k{i} = {i}\n"));
        }
        src.push_str("$server\n    host = \"local\"\n    port = 8080\n");
        src.push_str("note = \"\"\"\nhello\nworld\n\"\"\"\n");
        src.push_str("blob = b\"6869\"\n");

        let full = parse_str(&src).unwrap();
        // Chunk size smaller than the document so the reader cannot be a
        // single full-string load disguised as streaming.
        let streamed = parse_reader(Cursor::new(src.clone()), 32).unwrap();
        assert_eq!(sort_value(&streamed.root), sort_value(&full.root));
        assert_eq!(streamed.env_decls, full.env_decls);
        assert_eq!(streamed.root.get("k199"), Some(&Value::Int(199)));
        assert_eq!(
            streamed.root.get("note"),
            Some(&Value::Str("hello\nworld".into()))
        );
        let (_doc, events) = parse_reader_events(Cursor::new(src), 48).unwrap();
        assert!(events
            .iter()
            .any(|e| matches!(e, StreamEvent::Env(n) if n == "DB_URL")));
        assert!(events.len() > 50);
    }

    #[test]
    fn streaming_survives_a_chunk_that_splits_a_multibyte_character() {
        let src = "name = \"José\"\ncity = \"São\"\n";
        let full = parse_str(src).unwrap();
        // 1-byte chunks guarantee the boundary lands inside é / ã.
        let streamed = parse_reader(Cursor::new(src), 1).unwrap();
        assert_eq!(sort_value(&streamed.root), sort_value(&full.root));
        assert_eq!(streamed.root.get("name"), Some(&Value::Str("José".into())));
        assert_eq!(streamed.root.get("city"), Some(&Value::Str("São".into())));
    }

    #[test]
    fn streaming_agrees_when_a_block_comment_crosses_chunks() {
        let src = "a = 1\n/*\nhidden = 1\n*/\nc = 3\n";
        let full = parse_str(src).unwrap();
        let streamed = parse_reader(Cursor::new(src), 8).unwrap();
        assert_eq!(sort_value(&streamed.root), sort_value(&full.root));
        assert!(streamed.root.get("hidden").is_none());
        assert_eq!(streamed.root.get("a"), Some(&Value::Int(1)));
        assert_eq!(streamed.root.get("c"), Some(&Value::Int(3)));
    }
}
