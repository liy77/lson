//! Schema migration.
//!
//! A [`Migration`] is an ordered list of steps that rewrite a document from
//! one schema version to the next. Steps are data, so tests construct them
//! without touching the filesystem.

use crate::error::LsonError;
use crate::value::Value;

/// One rewrite applied to a document.
#[derive(Debug, Clone, PartialEq)]
pub enum Step {
    /// Rename a field. `from` and `to` are dotted paths; the last segment is the key.
    Rename { from: String, to: String },
    /// Delete a field.
    Remove { path: String },
    /// Set a field, creating intermediate objects.
    Set { path: String, value: Value },
    /// Copy a field to another path, leaving the source in place.
    Copy { from: String, to: String },
    /// Add `n` to an integer field.
    AddInt { path: String, n: i64 },
}

/// A named migration from `from_version` to `to_version`.
#[derive(Debug, Clone, PartialEq)]
pub struct Migration {
    pub from_version: u32,
    pub to_version: u32,
    pub steps: Vec<Step>,
}

impl Migration {
    pub fn new(from_version: u32, to_version: u32, steps: Vec<Step>) -> Self {
        Migration {
            from_version,
            to_version,
            steps,
        }
    }
}

/// Apply `migration` to `value`, returning the rewritten document.
pub fn migrate(value: &Value, migration: &Migration) -> Result<Value, LsonError> {
    let mut doc = value.clone();
    for step in &migration.steps {
        apply(&mut doc, step)?;
    }
    if let Some(meta) = doc.get_mut("schema_version") {
        *meta = Value::Int(i64::from(migration.to_version));
    } else if let Value::Object(fields) = &mut doc {
        fields.push((
            "schema_version".into(),
            Value::Int(i64::from(migration.to_version)),
        ));
    }
    Ok(doc)
}

fn apply(doc: &mut Value, step: &Step) -> Result<(), LsonError> {
    match step {
        Step::Rename { from, to } => {
            let value = take_path(doc, from)?;
            doc.insert_path(to, value)?;
        }
        Step::Remove { path } => {
            let _ = take_path(doc, path);
        }
        Step::Set { path, value } => {
            doc.insert_path(path, value.clone())?;
        }
        Step::Copy { from, to } => {
            let value = doc
                .pointer(from)
                .cloned()
                .ok_or_else(|| LsonError::msg(format!("missing '{from}'")))?;
            doc.insert_path(to, value)?;
        }
        Step::AddInt { path, n } => {
            let slot = doc
                .pointer_mut(path)
                .ok_or_else(|| LsonError::msg(format!("missing '{path}'")))?;
            match slot {
                Value::Int(cur) => *cur += n,
                other => {
                    return Err(LsonError::msg(format!(
                        "'{path}' is {}, not Integer",
                        other.type_name()
                    )))
                }
            }
        }
    }
    Ok(())
}

fn take_path(doc: &mut Value, path: &str) -> Result<Value, LsonError> {
    let (parent_path, key) = split_last(path)?;
    let parent = if parent_path.is_empty() {
        doc
    } else {
        doc.pointer_mut(parent_path)
            .ok_or_else(|| LsonError::msg(format!("missing '{parent_path}'")))?
    };
    let fields = parent
        .as_object_mut()
        .ok_or_else(|| LsonError::msg(format!("'{parent_path}' is not an object")))?;
    let idx = fields
        .iter()
        .position(|(k, _)| k == key)
        .ok_or_else(|| LsonError::msg(format!("missing '{path}'")))?;
    Ok(fields.remove(idx).1)
}

fn split_last(path: &str) -> Result<(&str, &str), LsonError> {
    if path.is_empty() {
        return Err(LsonError::msg("empty path"));
    }
    match path.rfind('.') {
        Some(i) => Ok((&path[..i], &path[i + 1..])),
        None => Ok(("", path)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::parse_str;

    #[test]
    fn migrates_old_schema_document_to_new() {
        let old = parse_str(
            r#"
schema_version = 1
name = "ada"
db_url = "postgres://old"
workers = 2
"#,
        )
        .unwrap()
        .root;
        let migration = Migration::new(
            1,
            2,
            vec![
                Step::Rename {
                    from: "db_url".into(),
                    to: "database.url".into(),
                },
                Step::AddInt {
                    path: "workers".into(),
                    n: 1,
                },
                Step::Remove {
                    path: "name".into(),
                },
                Step::Set {
                    path: "database.pool".into(),
                    value: Value::Int(4),
                },
            ],
        );
        let new_doc = migrate(&old, &migration).unwrap();
        assert_eq!(new_doc.get("schema_version"), Some(&Value::Int(2)));
        assert!(new_doc.get("name").is_none());
        assert!(new_doc.get("db_url").is_none());
        assert_eq!(new_doc.get("workers"), Some(&Value::Int(3)));
        let db = new_doc.get("database").unwrap();
        assert_eq!(db.get("url"), Some(&Value::Str("postgres://old".into())));
        assert_eq!(db.get("pool"), Some(&Value::Int(4)));
    }
}
