//! Indentation parser for KSON.
//!
//! Source is stripped of comments, then walked line by line. Values that
//! contain nested braces, brackets, or multiline strings are collected before
//! a recursive value parser turns them into [`Value`] nodes.

use super::lex::strip_comments;
use crate::error::LsonError;
use crate::value::{sort_value, Document, Value};
use std::collections::BTreeMap;

/// How a caller supplies imported files. Tests inject a map; the CLI reads disk.
pub type ImportResolver<'a> = dyn Fn(&str) -> Result<String, LsonError> + 'a;

/// Parser knobs. Defaults match a standalone document with no imports.
#[derive(Default)]
pub struct ParserOptions<'a> {
    pub resolver: Option<&'a ImportResolver<'a>>,
    /// When true, `@import` is recorded but not loaded (used to avoid cycles
    /// while the caller resolves files itself).
    pub record_imports_only: bool,
}

/// Parse a KSON document. `@import` without a resolver is an error.
pub fn parse_str(src: &str) -> Result<Document, LsonError> {
    parse_str_with(src, &mut ParserOptions::default(), &mut Vec::new())
}

/// Parse, resolving `@import("path")` through `resolver`.
pub fn parse_with_imports<F>(src: &str, resolver: F) -> Result<Document, LsonError>
where
    F: Fn(&str) -> Result<String, LsonError>,
{
    let mut opts = ParserOptions {
        resolver: Some(&resolver),
        record_imports_only: false,
    };
    let mut import_stack = Vec::new();
    parse_str_with(src, &mut opts, &mut import_stack)
}

pub fn parse_str_with(
    src: &str,
    opts: &mut ParserOptions<'_>,
    import_stack: &mut Vec<String>,
) -> Result<Document, LsonError> {
    let stripped = strip_comments(src);
    let mut doc = Document::empty();
    let mut stack: Vec<(usize, String)> = Vec::new();
    let lines: Vec<&str> = stripped.lines().collect();
    let mut i = 0usize;

    while i < lines.len() {
        let raw = lines[i];
        let line_no = i + 1;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("@env") {
            let name = directive_arg(rest)
                .ok_or_else(|| LsonError::parse(line_no, "@env expects @env(NAME)"))?;
            if !doc.env_decls.iter().any(|n| n == &name) {
                doc.env_decls.push(name);
            }
            i += 1;
            continue;
        }

        if let Some(rest) = trimmed.strip_prefix("@import") {
            let path = directive_arg(rest)
                .ok_or_else(|| LsonError::parse(line_no, "@import expects @import(\"path\")"))?;
            doc.imports.push(path.clone());
            if !opts.record_imports_only {
                let resolver = opts.resolver.ok_or_else(|| {
                    LsonError::parse(line_no, format!("no resolver for import '{path}'"))
                })?;
                if import_stack.iter().any(|p| p == &path) {
                    return Err(LsonError::parse(line_no, format!("cyclic import '{path}'")));
                }
                let text = resolver(&path)?;
                import_stack.push(path.clone());
                let imported = parse_str_with(&text, opts, import_stack)?;
                import_stack.pop();
                merge_import(&mut doc, imported);
            }
            i += 1;
            continue;
        }

        if trimmed.starts_with("@model") {
            // Historical directive. The library validator takes a schema argument
            // instead of loading a path and aborting, so the line is ignored here.
            i += 1;
            continue;
        }

        let indent = raw.len() - raw.trim_start().len();
        while stack.last().map(|(d, _)| *d >= indent).unwrap_or(false) {
            stack.pop();
        }

        if let Some(name) = trimmed.strip_prefix('$') {
            let name = name.trim().trim_end_matches(['{', '[']).trim();
            if name.is_empty() {
                return Err(LsonError::parse(line_no, "empty section name"));
            }
            ensure_section(&mut doc.root, &stack, name)?;
            stack.push((indent, name.to_string()));
            i += 1;
            continue;
        }

        let (key, value_src, consumed) = take_assignment(&lines, i, line_no)?;
        i += consumed;

        let is_anchor = key.starts_with('&');
        let key_name = key.trim_start_matches('&').to_string();
        if key_name.is_empty() {
            return Err(LsonError::parse(line_no, "empty key"));
        }

        let mut value = parse_value(&value_src, line_no, &doc.env_decls)?;
        if let Value::Ref(name) = &value {
            if let Some(anchored) = doc.anchors.get(name) {
                value = anchored.clone();
            }
        }

        if is_anchor {
            doc.anchors.insert(key_name.clone(), value.clone());
        }
        insert_field(&mut doc.root, &stack, key_name, value)?;
    }

    Ok(doc)
}

fn merge_import(into: &mut Document, imported: Document) {
    for (k, v) in imported.anchors {
        into.anchors.entry(k).or_insert(v);
    }
    for name in imported.env_decls {
        if !into.env_decls.iter().any(|n| n == &name) {
            into.env_decls.push(name);
        }
    }
    let Value::Object(fields) = imported.root else {
        return;
    };
    for (k, v) in fields {
        if into.root.get(&k).is_none() {
            let _ = into.root.insert_path(&k, v);
        }
    }
}

fn directive_arg(rest: &str) -> Option<String> {
    let rest = rest.trim();
    let inner = rest.strip_prefix('(')?.trim().strip_suffix(')')?.trim();
    let inner = inner
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(inner);
    if inner.is_empty() {
        None
    } else {
        Some(inner.to_string())
    }
}

fn ensure_section(
    root: &mut Value,
    stack: &[(usize, String)],
    name: &str,
) -> Result<(), LsonError> {
    let parent = navigate(root, stack)?;
    if parent.get(name).is_none() {
        let fields = parent
            .as_object_mut()
            .ok_or_else(|| LsonError::msg("section parent is not an object"))?;
        fields.push((name.to_string(), Value::Object(Vec::new())));
    }
    Ok(())
}

fn insert_field(
    root: &mut Value,
    stack: &[(usize, String)],
    key: String,
    value: Value,
) -> Result<(), LsonError> {
    let parent = navigate(root, stack)?;
    let fields = parent
        .as_object_mut()
        .ok_or_else(|| LsonError::msg("cannot insert into non-object"))?;
    if let Some(slot) = fields.iter_mut().find(|(k, _)| k == &key) {
        slot.1 = value;
    } else {
        fields.push((key, value));
    }
    Ok(())
}

fn navigate<'a>(
    root: &'a mut Value,
    stack: &[(usize, String)],
) -> Result<&'a mut Value, LsonError> {
    let mut cur = root;
    for (_, name) in stack {
        if cur.get(name).is_none() {
            let fields = cur
                .as_object_mut()
                .ok_or_else(|| LsonError::msg("broken section stack"))?;
            fields.push((name.clone(), Value::Object(Vec::new())));
        }
        cur = cur
            .get_mut(name)
            .ok_or_else(|| LsonError::msg("broken section stack"))?;
    }
    Ok(cur)
}

/// Returns (key, value source, lines consumed).
fn take_assignment(
    lines: &[&str],
    start: usize,
    line_no: usize,
) -> Result<(String, String, usize), LsonError> {
    let first = lines[start];
    let trimmed = first.trim();
    let eq = find_eq(trimmed).ok_or_else(|| {
        LsonError::parse(line_no, format!("expected key = value, got '{trimmed}'"))
    })?;
    let key = trimmed[..eq].trim().to_string();
    let mut value = trimmed[eq + 1..].trim().to_string();
    let mut consumed = 1;

    if value.starts_with("\"\"\"") && !triple_closed(&value) {
        let mut buf = value;
        let mut j = start + 1;
        while j < lines.len() {
            buf.push('\n');
            buf.push_str(lines[j]);
            consumed += 1;
            j += 1;
            if triple_closed(&buf) {
                break;
            }
        }
        if !triple_closed(&buf) {
            return Err(LsonError::parse(line_no, "unterminated multiline string"));
        }
        value = buf;
    } else if needs_more(&value) {
        let mut buf = value;
        let mut j = start + 1;
        while j < lines.len() && needs_more(&buf) {
            buf.push('\n');
            buf.push_str(lines[j]);
            consumed += 1;
            j += 1;
        }
        if needs_more(&buf) {
            return Err(LsonError::parse(line_no, "unterminated value"));
        }
        value = buf;
    }

    Ok((key, value, consumed))
}

fn triple_closed(s: &str) -> bool {
    let rest = s.strip_prefix("\"\"\"").unwrap_or(s);
    rest.contains("\"\"\"")
}

fn needs_more(s: &str) -> bool {
    let t = s.trim();
    if t.is_empty() {
        return true;
    }
    let mut depth_brace = 0i32;
    let mut depth_bracket = 0i32;
    let mut depth_paren = 0i32;
    let mut in_string = false;
    let mut chars = t.chars().peekable();
    while let Some(c) = chars.next() {
        if in_string {
            if c == '\\' {
                chars.next();
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => depth_brace += 1,
            '}' => depth_brace -= 1,
            '[' => depth_bracket += 1,
            ']' => depth_bracket -= 1,
            '(' => depth_paren += 1,
            ')' => depth_paren -= 1,
            _ => {}
        }
    }
    depth_brace > 0 || depth_bracket > 0 || depth_paren > 0 || in_string
}

fn find_eq(s: &str) -> Option<usize> {
    let mut in_string = false;
    for (i, c) in s.char_indices() {
        if c == '"' && !in_string {
            in_string = true;
        } else if c == '"' && in_string {
            in_string = false;
        } else if c == '=' && !in_string {
            return Some(i);
        }
    }
    None
}

fn parse_value(src: &str, line_no: usize, env_decls: &[String]) -> Result<Value, LsonError> {
    let s = src.trim();
    if s.is_empty() {
        return Err(LsonError::parse(line_no, "empty value"));
    }
    if s == "null" {
        return Ok(Value::Null);
    }
    if s == "true" {
        return Ok(Value::Bool(true));
    }
    if s == "false" {
        return Ok(Value::Bool(false));
    }
    if let Some(rest) = s.strip_prefix("b64\"") {
        let inner = rest
            .strip_suffix('"')
            .ok_or_else(|| LsonError::parse(line_no, "unterminated base64 literal"))?;
        let bytes = base64::Engine::decode(&base64::engine::general_purpose::STANDARD, inner)
            .map_err(|e| LsonError::parse(line_no, format!("bad base64: {e}")))?;
        return Ok(Value::Bytes(bytes));
    }
    if let Some(rest) = s.strip_prefix("b\"") {
        let inner = rest
            .strip_suffix('"')
            .ok_or_else(|| LsonError::parse(line_no, "unterminated binary literal"))?;
        let bytes = decode_byte_literal(inner).map_err(|e| LsonError::parse(line_no, e))?;
        return Ok(Value::Bytes(bytes));
    }
    if let Some(rest) = s.strip_prefix("d\"") {
        let inner = rest
            .strip_suffix('"')
            .ok_or_else(|| LsonError::parse(line_no, "unterminated date"))?;
        if !is_date(inner) {
            return Err(LsonError::parse(line_no, format!("invalid date '{inner}'")));
        }
        return Ok(Value::Date(inner.to_string()));
    }
    if let Some(rest) = s.strip_prefix("dt\"") {
        let inner = rest
            .strip_suffix('"')
            .ok_or_else(|| LsonError::parse(line_no, "unterminated datetime"))?;
        if !is_datetime(inner) {
            return Err(LsonError::parse(
                line_no,
                format!("invalid datetime '{inner}'"),
            ));
        }
        return Ok(Value::DateTime(inner.to_string()));
    }
    if s.starts_with("\"\"\"") {
        let inner = s
            .strip_prefix("\"\"\"")
            .and_then(|r| r.strip_suffix("\"\"\""))
            .ok_or_else(|| LsonError::parse(line_no, "unterminated multiline string"))?;
        let inner = inner.strip_prefix('\n').unwrap_or(inner);
        let inner = inner.strip_suffix('\n').unwrap_or(inner);
        return Ok(Value::Str(inner.to_string()));
    }
    if s.starts_with('"') {
        let inner = unquote(s).map_err(|e| LsonError::parse(line_no, e))?;
        return Ok(Value::Str(inner));
    }
    if s.starts_with('\'') && s.ends_with('\'') && s.chars().count() >= 2 {
        // Historical Char spelling. Stored as a one-character string so JSON
        // conversion stays lossless for existing files.
        let inner: String = s.chars().skip(1).collect();
        let inner = inner.trim_end_matches('\'').to_string();
        return Ok(Value::Str(inner));
    }
    if s.starts_with('#') {
        return parse_tagged(s, line_no, env_decls);
    }
    if let Some(name) = s.strip_prefix('*').or_else(|| s.strip_prefix('&')) {
        if is_ident(name) {
            return Ok(Value::Ref(name.to_string()));
        }
    }
    if s.starts_with('[') {
        return parse_array(s, line_no, env_decls);
    }
    if s.starts_with('{') {
        return parse_inline_object(s, line_no, env_decls);
    }
    if let Some(n) = parse_int(s) {
        return Ok(Value::Int(n));
    }
    if let Some(n) = parse_float(s) {
        return Ok(Value::Float(n));
    }
    if is_ident(s) {
        if env_decls.iter().any(|n| n == s) {
            return Ok(Value::Ref(s.to_string()));
        }
        // Bare words that are not declared env vars stay strings so historical
        // unquoted values (rare) do not vanish. Declared env names are refs.
        return Ok(Value::Str(s.to_string()));
    }
    Err(LsonError::parse(
        line_no,
        format!("cannot parse value '{s}'"),
    ))
}

fn parse_tagged(s: &str, line_no: usize, env_decls: &[String]) -> Result<Value, LsonError> {
    let rest = &s[1..];
    let tag_end = rest
        .find(|c: char| !c.is_ascii_alphanumeric() && c != '_')
        .unwrap_or(rest.len());
    let tag = &rest[..tag_end];
    if tag.is_empty() {
        return Err(LsonError::parse(line_no, "empty tag"));
    }
    let after = rest[tag_end..].trim();
    let payload = if after.is_empty() {
        Value::Null
    } else if let Some(inner) = after.strip_prefix('(').and_then(|r| r.strip_suffix(')')) {
        parse_value(inner.trim(), line_no, env_decls)?
    } else {
        return Err(LsonError::parse(line_no, format!("bad tagged value '{s}'")));
    };
    Ok(Value::Tagged {
        tag: tag.to_string(),
        payload: Box::new(payload),
    })
}

fn parse_array(s: &str, line_no: usize, env_decls: &[String]) -> Result<Value, LsonError> {
    let inner = s
        .trim()
        .strip_prefix('[')
        .and_then(|r| r.trim().strip_suffix(']'))
        .ok_or_else(|| LsonError::parse(line_no, "unterminated array"))?;
    if inner.trim().is_empty() {
        return Ok(Value::Array(Vec::new()));
    }
    let parts = split_top(inner, ',')?;
    let mut items = Vec::new();
    for part in parts {
        items.push(parse_value(part.trim(), line_no, env_decls)?);
    }
    Ok(Value::Array(items))
}

fn parse_inline_object(s: &str, line_no: usize, env_decls: &[String]) -> Result<Value, LsonError> {
    let inner = s
        .trim()
        .strip_prefix('{')
        .and_then(|r| r.trim().strip_suffix('}'))
        .ok_or_else(|| LsonError::parse(line_no, "unterminated object"))?;
    if inner.trim().is_empty() {
        return Ok(Value::Object(Vec::new()));
    }
    let parts = split_top(inner, ',')?;
    let mut fields = Vec::new();
    for part in parts {
        let part = part.trim();
        if part.is_empty() {
            continue;
        }
        let eq = find_eq(part).ok_or_else(|| {
            LsonError::parse(line_no, format!("object field missing '=': '{part}'"))
        })?;
        let key = part[..eq].trim().to_string();
        let value = parse_value(part[eq + 1..].trim(), line_no, env_decls)?;
        fields.push((key, value));
    }
    Ok(Value::Object(fields))
}

fn split_top(s: &str, sep: char) -> Result<Vec<&str>, LsonError> {
    let mut out = Vec::new();
    let mut start = 0;
    let mut brace = 0i32;
    let mut bracket = 0i32;
    let mut paren = 0i32;
    let mut in_string = false;
    let bytes = s.char_indices().collect::<Vec<_>>();
    let mut i = 0;
    while i < bytes.len() {
        let (idx, c) = bytes[i];
        if in_string {
            if c == '\\' {
                i += 2;
                continue;
            }
            if c == '"' {
                in_string = false;
            }
            i += 1;
            continue;
        }
        match c {
            '"' => in_string = true,
            '{' => brace += 1,
            '}' => brace -= 1,
            '[' => bracket += 1,
            ']' => bracket -= 1,
            '(' => paren += 1,
            ')' => paren -= 1,
            c if c == sep && brace == 0 && bracket == 0 && paren == 0 => {
                out.push(&s[start..idx]);
                start = idx + c.len_utf8();
            }
            _ => {}
        }
        i += 1;
    }
    out.push(&s[start..]);
    Ok(out)
}

fn unquote(s: &str) -> Result<String, String> {
    let s = s.trim();
    if !s.starts_with('"') || !s.ends_with('"') || s.len() < 2 {
        return Err(format!("unterminated string '{s}'"));
    }
    let inner = &s[1..s.len() - 1];
    let mut out = String::new();
    let mut chars = inner.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some('\\') => out.push('\\'),
                Some('"') => out.push('"'),
                Some(other) => {
                    out.push('\\');
                    out.push(other);
                }
                None => return Err("trailing escape".into()),
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

fn decode_byte_literal(inner: &str) -> Result<Vec<u8>, String> {
    let hexed: String = inner
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '_')
        .collect();
    if !hexed.len().is_multiple_of(2) {
        return Err("binary literal must have an even number of hex digits".into());
    }
    hex::decode(&hexed).map_err(|e| format!("bad binary literal: {e}"))
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_')
}

fn parse_int(s: &str) -> Option<i64> {
    if s.contains('.') || s.contains('e') || s.contains('E') {
        return None;
    }
    s.parse::<i64>().ok()
}

fn parse_float(s: &str) -> Option<f64> {
    if !(s.contains('.') || s.contains('e') || s.contains('E')) {
        return None;
    }
    let n = s.parse::<f64>().ok()?;
    if n.is_finite() {
        Some(n)
    } else {
        None
    }
}

fn is_date(s: &str) -> bool {
    let b = s.as_bytes();
    b.len() == 10
        && b[4] == b'-'
        && b[7] == b'-'
        && b[..4].iter().all(|c| c.is_ascii_digit())
        && b[5..7].iter().all(|c| c.is_ascii_digit())
        && b[8..].iter().all(|c| c.is_ascii_digit())
}

fn is_datetime(s: &str) -> bool {
    // YYYY-MM-DDTHH:MM:SSZ or with fractional seconds.
    if s.len() < 20 || !s.ends_with('Z') {
        return false;
    }
    let Some((date, time)) = s.split_once('T') else {
        return false;
    };
    if !is_date(date) {
        return false;
    }
    let time = time.trim_end_matches('Z');
    let main = time.split('.').next().unwrap_or(time);
    let parts: Vec<&str> = main.split(':').collect();
    parts.len() == 3
        && parts
            .iter()
            .all(|p| p.len() == 2 && p.bytes().all(|c| c.is_ascii_digit()))
}

// ── Canonical text ────────────────────────────────────────────────────────────

/// Deterministic textual form: sorted keys, stable scalar encoding, no comments.
pub fn canonical(value: &Value) -> String {
    let sorted = sort_value(value);
    let mut out = String::new();
    write_canonical(&sorted, 0, &mut out);
    if !out.ends_with('\n') {
        out.push('\n');
    }
    out
}

fn write_canonical(value: &Value, indent: usize, out: &mut String) {
    match value {
        Value::Object(fields) => {
            if indent == 0 {
                for (k, v) in fields {
                    write_field(k, v, indent, out);
                }
            } else {
                out.push_str(&format_inline(value));
                out.push('\n');
            }
        }
        other => {
            out.push_str(&format_inline(other));
            out.push('\n');
        }
    }
}

fn write_field(key: &str, value: &Value, indent: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    match value {
        Value::Object(fields) if !fields.is_empty() && indent_friendly(fields) => {
            out.push_str(&pad);
            out.push('$');
            out.push_str(key);
            out.push('\n');
            for (k, v) in fields {
                write_field(k, v, indent + 1, out);
            }
        }
        _ => {
            out.push_str(&pad);
            out.push_str(key);
            out.push_str(" = ");
            out.push_str(&format_inline(value));
            out.push('\n');
        }
    }
}

fn indent_friendly(fields: &[(String, Value)]) -> bool {
    fields.iter().all(|(k, _)| is_ident(k))
}

fn format_inline(value: &Value) -> String {
    match value {
        Value::Int(n) => n.to_string(),
        Value::Float(n) => format_float(*n),
        Value::Str(s) => quote(s),
        Value::Bool(b) => b.to_string(),
        Value::Bytes(b) => format!("b\"{}\"", hex::encode(b)),
        Value::Date(d) => format!("d\"{d}\""),
        Value::DateTime(d) => format!("dt\"{d}\""),
        Value::Array(items) => {
            let parts: Vec<String> = items.iter().map(format_inline).collect();
            format!("[{}]", parts.join(", "))
        }
        Value::Object(fields) => {
            let parts: Vec<String> = fields
                .iter()
                .map(|(k, v)| format!("{k} = {}", format_inline(v)))
                .collect();
            format!("{{ {} }}", parts.join(", "))
        }
        Value::Tagged { tag, payload } => {
            if matches!(payload.as_ref(), Value::Null) {
                format!("#{tag}")
            } else {
                format!("#{tag}({})", format_inline(payload))
            }
        }
        Value::Ref(name) => format!("*{name}"),
        Value::Null => "null".into(),
    }
}

fn format_float(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{n:.1}")
    } else {
        let s = format!("{n}");
        if s.contains('.') || s.contains('e') || s.contains('E') {
            s
        } else {
            format!("{s}.0")
        }
    }
}

fn quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

// ── Canonical binary (length-prefixed, sorted) ───────────────────────────────

const BIN_MAGIC: &[u8] = b"KSONBIN1";

/// Length-prefixed binary encoding of the canonical (sorted) tree.
pub fn canonical_bytes(value: &Value) -> Vec<u8> {
    let sorted = sort_value(value);
    let mut out = BIN_MAGIC.to_vec();
    write_bin(&sorted, &mut out);
    out
}

pub fn decode_canonical_bytes(bytes: &[u8]) -> Result<Value, LsonError> {
    if bytes.len() < BIN_MAGIC.len() || &bytes[..BIN_MAGIC.len()] != BIN_MAGIC {
        return Err(LsonError::msg("not a KSON binary document"));
    }
    let mut i = BIN_MAGIC.len();
    read_bin(bytes, &mut i)
}

fn write_bin(value: &Value, out: &mut Vec<u8>) {
    match value {
        Value::Null => out.push(0),
        Value::Bool(b) => {
            out.push(1);
            out.push(u8::from(*b));
        }
        Value::Int(n) => {
            out.push(2);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Value::Float(n) => {
            out.push(3);
            out.extend_from_slice(&n.to_le_bytes());
        }
        Value::Str(s) => {
            out.push(4);
            write_str(s, out);
        }
        Value::Bytes(b) => {
            out.push(5);
            write_len(b.len(), out);
            out.extend_from_slice(b);
        }
        Value::Date(d) => {
            out.push(6);
            write_str(d, out);
        }
        Value::DateTime(d) => {
            out.push(7);
            write_str(d, out);
        }
        Value::Array(items) => {
            out.push(8);
            write_len(items.len(), out);
            for item in items {
                write_bin(item, out);
            }
        }
        Value::Object(fields) => {
            out.push(9);
            write_len(fields.len(), out);
            for (k, v) in fields {
                write_str(k, out);
                write_bin(v, out);
            }
        }
        Value::Tagged { tag, payload } => {
            out.push(10);
            write_str(tag, out);
            write_bin(payload, out);
        }
        Value::Ref(name) => {
            out.push(11);
            write_str(name, out);
        }
    }
}

fn write_str(s: &str, out: &mut Vec<u8>) {
    write_len(s.len(), out);
    out.extend_from_slice(s.as_bytes());
}

fn write_len(n: usize, out: &mut Vec<u8>) {
    out.extend_from_slice(&(n as u32).to_le_bytes());
}

fn read_bin(bytes: &[u8], i: &mut usize) -> Result<Value, LsonError> {
    let tag = *bytes
        .get(*i)
        .ok_or_else(|| LsonError::msg("truncated binary"))?;
    *i += 1;
    match tag {
        0 => Ok(Value::Null),
        1 => {
            let b = *bytes
                .get(*i)
                .ok_or_else(|| LsonError::msg("truncated bool"))?;
            *i += 1;
            Ok(Value::Bool(b != 0))
        }
        2 => {
            let n = read_i64(bytes, i)?;
            Ok(Value::Int(n))
        }
        3 => {
            let n = read_f64(bytes, i)?;
            Ok(Value::Float(n))
        }
        4 => Ok(Value::Str(read_str(bytes, i)?)),
        5 => {
            let len = read_u32(bytes, i)? as usize;
            let end = *i + len;
            let slice = bytes
                .get(*i..end)
                .ok_or_else(|| LsonError::msg("truncated bytes"))?;
            *i = end;
            Ok(Value::Bytes(slice.to_vec()))
        }
        6 => Ok(Value::Date(read_str(bytes, i)?)),
        7 => Ok(Value::DateTime(read_str(bytes, i)?)),
        8 => {
            let len = read_u32(bytes, i)? as usize;
            let mut items = Vec::with_capacity(len);
            for _ in 0..len {
                items.push(read_bin(bytes, i)?);
            }
            Ok(Value::Array(items))
        }
        9 => {
            let len = read_u32(bytes, i)? as usize;
            let mut fields = Vec::with_capacity(len);
            for _ in 0..len {
                let k = read_str(bytes, i)?;
                let v = read_bin(bytes, i)?;
                fields.push((k, v));
            }
            Ok(Value::Object(fields))
        }
        10 => {
            let tag = read_str(bytes, i)?;
            let payload = read_bin(bytes, i)?;
            Ok(Value::Tagged {
                tag,
                payload: Box::new(payload),
            })
        }
        11 => Ok(Value::Ref(read_str(bytes, i)?)),
        other => Err(LsonError::msg(format!("unknown binary tag {other}"))),
    }
}

fn read_u32(bytes: &[u8], i: &mut usize) -> Result<u32, LsonError> {
    let end = *i + 4;
    let slice = bytes
        .get(*i..end)
        .ok_or_else(|| LsonError::msg("truncated length"))?;
    *i = end;
    let mut buf = [0u8; 4];
    buf.copy_from_slice(slice);
    Ok(u32::from_le_bytes(buf))
}

fn read_i64(bytes: &[u8], i: &mut usize) -> Result<i64, LsonError> {
    let end = *i + 8;
    let slice = bytes
        .get(*i..end)
        .ok_or_else(|| LsonError::msg("truncated int"))?;
    *i = end;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(slice);
    Ok(i64::from_le_bytes(buf))
}

fn read_f64(bytes: &[u8], i: &mut usize) -> Result<f64, LsonError> {
    let end = *i + 8;
    let slice = bytes
        .get(*i..end)
        .ok_or_else(|| LsonError::msg("truncated float"))?;
    *i = end;
    let mut buf = [0u8; 8];
    buf.copy_from_slice(slice);
    Ok(f64::from_le_bytes(buf))
}

fn read_str(bytes: &[u8], i: &mut usize) -> Result<String, LsonError> {
    let len = read_u32(bytes, i)? as usize;
    let end = *i + len;
    let slice = bytes
        .get(*i..end)
        .ok_or_else(|| LsonError::msg("truncated string"))?;
    *i = end;
    String::from_utf8(slice.to_vec()).map_err(|e| LsonError::msg(format!("bad utf-8: {e}")))
}

/// Lookup table kept so callers can resolve a ref after the fact.
#[allow(dead_code)]
pub fn anchor_table(doc: &Document) -> &BTreeMap<String, Value> {
    &doc.anchors
}
