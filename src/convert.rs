//! Conversion of a KSON value tree to JSON, YAML, and CBOR.
//!
//! Typed extras (bytes, dates, tags, refs) become tagged objects so the
//! conversion is lossless enough to tell the kinds apart. Plain scalars stay
//! plain so a historical config still looks like JSON.

use crate::error::LsonError;
use crate::value::Value;
use serde_json::Value as Json;

/// JSON text. Object keys follow the value's current order (callers who want
/// a stable file should canonicalise first).
pub fn to_json(value: &Value) -> Result<String, LsonError> {
    let json = to_json_value(value);
    serde_json::to_string_pretty(&json).map_err(|e| LsonError::msg(format!("json: {e}")))
}

pub fn to_json_value(value: &Value) -> Json {
    match value {
        Value::Null => Json::Null,
        Value::Bool(b) => Json::Bool(*b),
        Value::Int(n) => Json::Number((*n).into()),
        Value::Float(n) => serde_json::Number::from_f64(*n)
            .map(Json::Number)
            .unwrap_or(Json::Null),
        Value::Str(s) => Json::String(s.clone()),
        Value::Bytes(b) => Json::Object(tagged("bytes", Json::String(hex::encode(b)))),
        Value::Date(d) => Json::Object(tagged("date", Json::String(d.clone()))),
        Value::DateTime(d) => Json::Object(tagged("datetime", Json::String(d.clone()))),
        Value::Array(items) => Json::Array(items.iter().map(to_json_value).collect()),
        Value::Object(fields) => {
            let mut map = serde_json::Map::new();
            for (k, v) in fields {
                map.insert(k.clone(), to_json_value(v));
            }
            Json::Object(map)
        }
        Value::Tagged { tag, payload } => {
            let mut map = serde_json::Map::new();
            map.insert("$tag".into(), Json::String(tag.clone()));
            map.insert("$value".into(), to_json_value(payload));
            Json::Object(map)
        }
        Value::Ref(name) => Json::Object(tagged("ref", Json::String(name.clone()))),
    }
}

fn tagged(kind: &str, value: Json) -> serde_json::Map<String, Json> {
    let mut map = serde_json::Map::new();
    map.insert("$type".into(), Json::String(kind.into()));
    map.insert("$value".into(), value);
    map
}

pub fn to_yaml(value: &Value) -> Result<String, LsonError> {
    let json = to_json_value(value);
    serde_yaml::to_string(&json).map_err(|e| LsonError::msg(format!("yaml: {e}")))
}

/// CBOR bytes of the same tagged JSON model, so YAML/JSON/CBOR agree on shape.
pub fn to_cbor(value: &Value) -> Result<Vec<u8>, LsonError> {
    to_cbor_value(value)
}

pub fn to_cbor_value(value: &Value) -> Result<Vec<u8>, LsonError> {
    let json = to_json_value(value);
    let mut buf = Vec::new();
    ciborium::into_writer(&json, &mut buf).map_err(|e| LsonError::msg(format!("cbor: {e}")))?;
    Ok(buf)
}

pub fn from_cbor_slice(bytes: &[u8]) -> Result<Value, LsonError> {
    let json: Json =
        ciborium::from_reader(bytes).map_err(|e| LsonError::msg(format!("cbor: {e}")))?;
    Ok(from_json_value(&json))
}

pub fn from_json_str(text: &str) -> Result<Value, LsonError> {
    let json: Json =
        serde_json::from_str(text).map_err(|e| LsonError::msg(format!("json: {e}")))?;
    Ok(from_json_value(&json))
}

pub fn from_yaml_str(text: &str) -> Result<Value, LsonError> {
    let json: Json =
        serde_yaml::from_str(text).map_err(|e| LsonError::msg(format!("yaml: {e}")))?;
    Ok(from_json_value(&json))
}

fn from_json_value(json: &Json) -> Value {
    match json {
        Json::Null => Value::Null,
        Json::Bool(b) => Value::Bool(*b),
        Json::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Int(i)
            } else if let Some(f) = n.as_f64() {
                Value::Float(f)
            } else {
                Value::Str(n.to_string())
            }
        }
        Json::String(s) => Value::Str(s.clone()),
        Json::Array(items) => Value::Array(items.iter().map(from_json_value).collect()),
        Json::Object(map) => {
            if let Some(Json::String(kind)) = map.get("$type") {
                if let Some(inner) = map.get("$value") {
                    return match kind.as_str() {
                        "bytes" => match inner {
                            Json::String(hexed) => {
                                Value::Bytes(hex::decode(hexed).unwrap_or_default())
                            }
                            _ => Value::Bytes(Vec::new()),
                        },
                        "date" => Value::Date(json_string(inner)),
                        "datetime" => Value::DateTime(json_string(inner)),
                        "ref" => Value::Ref(json_string(inner)),
                        _ => object_from_map(map),
                    };
                }
            }
            if let Some(Json::String(tag)) = map.get("$tag") {
                let payload = map
                    .get("$value")
                    .map(from_json_value)
                    .unwrap_or(Value::Null);
                return Value::Tagged {
                    tag: tag.clone(),
                    payload: Box::new(payload),
                };
            }
            object_from_map(map)
        }
    }
}

fn json_string(json: &Json) -> String {
    match json {
        Json::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn object_from_map(map: &serde_json::Map<String, Json>) -> Value {
    let mut fields = Vec::new();
    for (k, v) in map {
        fields.push((k.clone(), from_json_value(v)));
    }
    Value::Object(fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::parse_str;

    #[test]
    fn converts_to_json_yaml_and_cbor() {
        let doc = parse_str(
            r#"
name = "ada"
port = 1
blob = b"6869"
when = dt"2020-01-02T03:04:05Z"
"#,
        )
        .unwrap();
        let json = to_json(&doc.root).unwrap();
        assert!(json.contains("\"name\""));
        assert!(json.contains("ada"));
        assert!(json.contains("\"port\""));
        let back = from_json_str(&json).unwrap();
        assert_eq!(back.get("name"), Some(&Value::Str("ada".into())));
        assert_eq!(back.get("blob"), Some(&Value::Bytes(b"hi".to_vec())));

        let yaml = to_yaml(&doc.root).unwrap();
        assert!(yaml.contains("ada"));
        let from_yaml = from_yaml_str(&yaml).unwrap();
        assert_eq!(from_yaml.get("port"), Some(&Value::Int(1)));

        let cbor = to_cbor(&doc.root).unwrap();
        assert!(!cbor.is_empty());
        let from_cbor = from_cbor_slice(&cbor).unwrap();
        assert_eq!(from_cbor.get("name"), Some(&Value::Str("ada".into())));
        assert_eq!(
            from_cbor.get("when"),
            Some(&Value::DateTime("2020-01-02T03:04:05Z".into()))
        );
    }
}
