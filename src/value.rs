//! Typed KSON document tree.
//!
//! Values carry their own type. The tree has no I/O: parsers, serializers,
//! validators, and tools all operate on [`Value`] / [`Document`].

use crate::error::LsonError;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;

/// A single typed node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", content = "v")]
pub enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Bool(bool),
    Bytes(Vec<u8>),
    /// Calendar date `YYYY-MM-DD`.
    Date(String),
    /// Instant `YYYY-MM-DDTHH:MM:SS[.fff]Z`.
    DateTime(String),
    Array(Vec<Value>),
    /// Object fields stay in source order for display; canonical form sorts them.
    Object(Vec<(String, Value)>),
    /// Tagged union: `Tag { payload }` or `Tag`.
    Tagged {
        tag: String,
        payload: Box<Value>,
    },
    /// Unresolved or resolved reference (`*name` / `&name`).
    Ref(String),
    Null,
}

impl Value {
    pub fn as_object(&self) -> Option<&[(String, Value)]> {
        match self {
            Value::Object(fields) => Some(fields),
            _ => None,
        }
    }

    pub fn as_object_mut(&mut self) -> Option<&mut Vec<(String, Value)>> {
        match self {
            Value::Object(fields) => Some(fields),
            _ => None,
        }
    }

    pub fn get(&self, key: &str) -> Option<&Value> {
        self.as_object()?
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn get_mut(&mut self, key: &str) -> Option<&mut Value> {
        self.as_object_mut()?
            .iter_mut()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
    }

    pub fn type_name(&self) -> &'static str {
        match self {
            Value::Int(_) => "Integer",
            Value::Float(_) => "Float",
            Value::Str(_) => "String",
            Value::Bool(_) => "Bool",
            Value::Bytes(_) => "Bytes",
            Value::Date(_) => "Date",
            Value::DateTime(_) => "DateTime",
            Value::Array(_) => "Array",
            Value::Object(_) => "Object",
            Value::Tagged { .. } => "Tagged",
            Value::Ref(_) => "Ref",
            Value::Null => "Null",
        }
    }

    /// Walk `a.b.c` (and `a[0].b`) and return the node.
    pub fn pointer(&self, path: &str) -> Option<&Value> {
        if path.is_empty() {
            return Some(self);
        }
        let mut cur = self;
        for seg in split_path(path) {
            cur = match seg {
                PathSeg::Key(k) => cur.get(&k)?,
                PathSeg::Index(i) => match cur {
                    Value::Array(items) => items.get(i)?,
                    _ => return None,
                },
            };
        }
        Some(cur)
    }

    pub fn pointer_mut(&mut self, path: &str) -> Option<&mut Value> {
        if path.is_empty() {
            return Some(self);
        }
        let segs = split_path(path);
        let mut cur = self;
        for seg in segs {
            cur = match seg {
                PathSeg::Key(k) => cur.get_mut(&k)?,
                PathSeg::Index(i) => match cur {
                    Value::Array(items) => items.get_mut(i)?,
                    _ => return None,
                },
            };
        }
        Some(cur)
    }

    /// Insert or replace `path`, creating intermediate objects as needed.
    /// Array indexes are not created; the parent must already be an array.
    pub fn insert_path(&mut self, path: &str, value: Value) -> Result<(), LsonError> {
        let segs = split_path(path);
        if segs.is_empty() {
            *self = value;
            return Ok(());
        }
        let mut cur = self;
        for (i, seg) in segs.iter().enumerate() {
            let last = i + 1 == segs.len();
            match seg {
                PathSeg::Key(k) => {
                    if !matches!(cur, Value::Object(_)) {
                        *cur = Value::Object(Vec::new());
                    }
                    if last {
                        let fields = cur.as_object_mut().unwrap();
                        if let Some(slot) = fields.iter_mut().find(|(name, _)| name == k) {
                            slot.1 = value;
                        } else {
                            fields.push((k.clone(), value));
                        }
                        return Ok(());
                    }
                    let fields = cur.as_object_mut().unwrap();
                    if !fields.iter().any(|(name, _)| name == k) {
                        fields.push((k.clone(), Value::Object(Vec::new())));
                    }
                    cur = fields
                        .iter_mut()
                        .find(|(name, _)| name == k)
                        .map(|(_, v)| v)
                        .unwrap();
                }
                PathSeg::Index(idx) => {
                    let items = match cur {
                        Value::Array(items) => items,
                        _ => {
                            return Err(LsonError::msg(format!(
                                "cannot index non-array at '{path}'"
                            )))
                        }
                    };
                    let slot = items.get_mut(*idx).ok_or_else(|| {
                        LsonError::msg(format!("index {idx} out of range in '{path}'"))
                    })?;
                    if last {
                        *slot = value;
                        return Ok(());
                    }
                    cur = slot;
                }
            }
        }
        Ok(())
    }
}

#[derive(Clone)]
enum PathSeg {
    Key(String),
    Index(usize),
}

fn split_path(path: &str) -> Vec<PathSeg> {
    let mut out = Vec::new();
    for part in path.split('.') {
        if part.is_empty() {
            continue;
        }
        let mut rest = part;
        if let Some(bracket) = rest.find('[') {
            let key = &rest[..bracket];
            if !key.is_empty() {
                out.push(PathSeg::Key(key.to_string()));
            }
            rest = &rest[bracket..];
            while let Some(start) = rest.find('[') {
                let end = match rest[start + 1..].find(']') {
                    Some(e) => start + 1 + e,
                    None => break,
                };
                if let Ok(i) = rest[start + 1..end].parse::<usize>() {
                    out.push(PathSeg::Index(i));
                }
                rest = &rest[end + 1..];
            }
        } else {
            out.push(PathSeg::Key(rest.to_string()));
        }
    }
    out
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(n) => write!(f, "{n}"),
            Value::Float(n) => write!(f, "{n}"),
            Value::Str(s) => write!(f, "{s:?}"),
            Value::Bool(b) => write!(f, "{b}"),
            Value::Bytes(b) => write!(f, "b\"{}\"", bytes_to_hex(b)),
            Value::Date(d) => write!(f, "d\"{d}\""),
            Value::DateTime(d) => write!(f, "dt\"{d}\""),
            Value::Array(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{item}")?;
                }
                write!(f, "]")
            }
            Value::Object(fields) => {
                write!(f, "{{")?;
                for (i, (k, v)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k} = {v}")?;
                }
                write!(f, "}}")
            }
            Value::Tagged { tag, payload } => {
                if matches!(payload.as_ref(), Value::Null) {
                    write!(f, "#{tag}")
                } else {
                    write!(f, "#{tag}({payload})")
                }
            }
            Value::Ref(name) => write!(f, "*{name}"),
            Value::Null => write!(f, "null"),
        }
    }
}

fn bytes_to_hex(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// A parsed document: the value tree plus the anchors declared in it.
#[derive(Debug, Clone, PartialEq)]
pub struct Document {
    pub root: Value,
    /// Anchor name → value, in declaration order.
    pub anchors: BTreeMap<String, Value>,
    /// `@env` names declared by the source (not yet substituted).
    pub env_decls: Vec<String>,
    /// `@import` paths, in source order.
    pub imports: Vec<String>,
}

impl Document {
    pub fn empty() -> Self {
        Document {
            root: Value::Object(Vec::new()),
            anchors: BTreeMap::new(),
            env_decls: Vec::new(),
            imports: Vec::new(),
        }
    }

    pub fn object(fields: Vec<(String, Value)>) -> Self {
        Document {
            root: Value::Object(fields),
            anchors: BTreeMap::new(),
            env_decls: Vec::new(),
            imports: Vec::new(),
        }
    }
}

impl Default for Document {
    fn default() -> Self {
        Self::empty()
    }
}

/// Stable ordering used by canonical serialization, diff, and hashing.
pub fn sort_value(value: &Value) -> Value {
    match value {
        Value::Object(fields) => {
            let mut fields: Vec<(String, Value)> = fields
                .iter()
                .map(|(k, v)| (k.clone(), sort_value(v)))
                .collect();
            fields.sort_by(|a, b| a.0.cmp(&b.0));
            Value::Object(fields)
        }
        Value::Array(items) => Value::Array(items.iter().map(sort_value).collect()),
        Value::Tagged { tag, payload } => Value::Tagged {
            tag: tag.clone(),
            payload: Box::new(sort_value(payload)),
        },
        other => other.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pointer_and_insert_round_trip() {
        let mut doc = Value::Object(vec![(
            "server".into(),
            Value::Object(vec![("port".into(), Value::Int(1))]),
        )]);
        doc.insert_path("server.host", Value::Str("local".into()))
            .unwrap();
        doc.insert_path("flags", Value::Array(vec![Value::Bool(true)]))
            .unwrap();
        assert_eq!(
            doc.pointer("server.host"),
            Some(&Value::Str("local".into()))
        );
        assert_eq!(doc.pointer("flags[0]"), Some(&Value::Bool(true)));
    }
}
