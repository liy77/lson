//! Patch engine.
//!
//! A patch names the fields to change and leaves every other field alone.

use crate::error::LsonError;
use crate::value::Value;

#[derive(Debug, Clone, PartialEq)]
pub enum Op {
    /// Set `path` to `value`, creating intermediate objects.
    Set { path: String, value: Value },
    /// Remove `path` if it exists.
    Remove { path: String },
}

#[derive(Debug, Clone, PartialEq, Default)]
pub struct Patch {
    pub ops: Vec<Op>,
}

impl Patch {
    pub fn new(ops: Vec<Op>) -> Self {
        Patch { ops }
    }
}

/// Apply `patch` to a clone of `doc`. Untouched fields keep their values.
pub fn apply(doc: &Value, patch: &Patch) -> Result<Value, LsonError> {
    let mut out = doc.clone();
    for op in &patch.ops {
        match op {
            Op::Set { path, value } => out.insert_path(path, value.clone())?,
            Op::Remove { path } => {
                remove_path(&mut out, path)?;
            }
        }
    }
    Ok(out)
}

fn remove_path(doc: &mut Value, path: &str) -> Result<(), LsonError> {
    if path.is_empty() {
        return Err(LsonError::msg("cannot remove the document root"));
    }
    let (parent_path, key) = match path.rfind('.') {
        Some(i) => (&path[..i], &path[i + 1..]),
        None => ("", path),
    };
    let parent = if parent_path.is_empty() {
        doc
    } else {
        match doc.pointer_mut(parent_path) {
            Some(p) => p,
            None => return Ok(()),
        }
    };
    if let Some(fields) = parent.as_object_mut() {
        fields.retain(|(k, _)| k != key);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::parse_str;

    #[test]
    fn patch_changes_only_named_fields() {
        let doc = parse_str("name = \"ada\"\nport = 1\nflag = true\n")
            .unwrap()
            .root;
        let patch = Patch::new(vec![
            Op::Set {
                path: "port".into(),
                value: Value::Int(8080),
            },
            Op::Set {
                path: "server.host".into(),
                value: Value::Str("local".into()),
            },
        ]);
        let patched = apply(&doc, &patch).unwrap();
        assert_eq!(patched.get("port"), Some(&Value::Int(8080)));
        assert_eq!(patched.get("name"), Some(&Value::Str("ada".into())));
        assert_eq!(patched.get("flag"), Some(&Value::Bool(true)));
        assert_eq!(
            patched.get("server").and_then(|s| s.get("host")),
            Some(&Value::Str("local".into()))
        );
    }
}
