//! Environment substitution.
//!
//! Declared `@env` names become [`Value::Ref`] during parse. Resolution
//! replaces those refs (and `$NAME` / `${NAME}` inside strings) using a
//! caller-supplied lookup, so tests never touch the process environment.

use crate::error::LsonError;
use crate::value::{Document, Value};
use std::collections::HashMap;

/// Replace declared environment references in `doc`.
///
/// A missing variable is an error (fail closed), matching the historical
/// `MISSING_ENV` behaviour, but returned instead of aborting the process.
pub fn resolve<F>(doc: &Document, lookup: F) -> Result<Value, LsonError>
where
    F: Fn(&str) -> Option<String>,
{
    let declared: Vec<&str> = doc.env_decls.iter().map(|s| s.as_str()).collect();
    resolve_value(&doc.root, &declared, &lookup)
}

pub fn resolve_with_map(
    doc: &Document,
    vars: &HashMap<String, String>,
) -> Result<Value, LsonError> {
    resolve(doc, |name| vars.get(name).cloned())
}

fn resolve_value<F>(value: &Value, declared: &[&str], lookup: &F) -> Result<Value, LsonError>
where
    F: Fn(&str) -> Option<String>,
{
    match value {
        Value::Ref(name) if declared.iter().any(|d| d == name) || declared.is_empty() => {
            let raw = lookup(name)
                .ok_or_else(|| LsonError::msg(format!("missing environment variable '{name}'")))?;
            Ok(coerce_env(&raw))
        }
        Value::Ref(name) => Ok(Value::Ref(name.clone())),
        Value::Str(s) => Ok(Value::Str(expand_string(s, lookup)?)),
        Value::Array(items) => {
            let mut out = Vec::with_capacity(items.len());
            for item in items {
                out.push(resolve_value(item, declared, lookup)?);
            }
            Ok(Value::Array(out))
        }
        Value::Object(fields) => {
            let mut out = Vec::with_capacity(fields.len());
            for (k, v) in fields {
                out.push((k.clone(), resolve_value(v, declared, lookup)?));
            }
            Ok(Value::Object(out))
        }
        Value::Tagged { tag, payload } => Ok(Value::Tagged {
            tag: tag.clone(),
            payload: Box::new(resolve_value(payload, declared, lookup)?),
        }),
        other => Ok(other.clone()),
    }
}

/// `$NAME` and `${NAME}` inside strings. Unknown names are left intact so a
/// literal dollar sign in prose is not a hard error.
fn expand_string<F>(s: &str, lookup: &F) -> Result<String, LsonError>
where
    F: Fn(&str) -> Option<String>,
{
    let mut out = String::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if chars[i] == '$' && chars.get(i + 1) == Some(&'{') {
            if let Some(end) = chars[i + 2..].iter().position(|c| *c == '}') {
                let name: String = chars[i + 2..i + 2 + end].iter().collect();
                match lookup(&name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        return Err(LsonError::msg(format!(
                            "missing environment variable '{name}'"
                        )))
                    }
                }
                i += 3 + end;
                continue;
            }
        }
        if chars[i] == '$' {
            let start = i + 1;
            let mut end = start;
            while end < chars.len() && (chars[end].is_ascii_alphanumeric() || chars[end] == '_') {
                end += 1;
            }
            if end > start {
                let name: String = chars[start..end].iter().collect();
                match lookup(&name) {
                    Some(v) => out.push_str(&v),
                    None => {
                        return Err(LsonError::msg(format!(
                            "missing environment variable '{name}'"
                        )))
                    }
                }
                i = end;
                continue;
            }
        }
        out.push(chars[i]);
        i += 1;
    }
    Ok(out)
}

fn coerce_env(raw: &str) -> Value {
    if raw == "true" {
        Value::Bool(true)
    } else if raw == "false" {
        Value::Bool(false)
    } else if let Ok(n) = raw.parse::<i64>() {
        Value::Int(n)
    } else {
        Value::Str(raw.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::parse_str;
    use std::collections::HashMap;

    #[test]
    fn substitutes_declared_env_and_string_placeholders() {
        let doc = parse_str(
            r#"
@env(DB_URL)
@env(PORT)
url = DB_URL
port = PORT
note = "connect ${DB_URL}"
"#,
        )
        .unwrap();
        let mut vars = HashMap::new();
        vars.insert("DB_URL".into(), "postgres://db".into());
        vars.insert("PORT".into(), "5432".into());
        let resolved = resolve_with_map(&doc, &vars).unwrap();
        assert_eq!(
            resolved.get("url"),
            Some(&Value::Str("postgres://db".into()))
        );
        assert_eq!(resolved.get("port"), Some(&Value::Int(5432)));
        assert_eq!(
            resolved.get("note"),
            Some(&Value::Str("connect postgres://db".into()))
        );
    }

    #[test]
    fn missing_env_is_an_error() {
        let doc = parse_str("@env(NOPE)\nk = NOPE\n").unwrap();
        let err = resolve_with_map(&doc, &HashMap::new()).unwrap_err();
        assert!(err.to_string().contains("NOPE"));
    }
}
