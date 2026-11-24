//! KSON format: lexer, parser, canonical serializer, binary encoding, imports.
//!
//! The grammar extends the historical indentation syntax (`key = value`,
//! `$section`, `@env`) so existing files still parse, and adds typed forms
//! (bytes, dates, anchors, tags, imports, block comments, multiline strings).

mod lex;
mod parse;

pub use lex::strip_comments;
pub use parse::{
    canonical, canonical_bytes, decode_canonical_bytes, parse_str, parse_str_with,
    parse_with_imports, ImportResolver, ParserOptions,
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::value::Value;
    use std::collections::HashMap;

    fn root_fields(src: &str) -> Vec<(String, Value)> {
        match parse_str(src).unwrap().root {
            Value::Object(fields) => fields,
            other => panic!("expected object, got {other:?}"),
        }
    }

    fn field<'a>(fields: &'a [(String, Value)], key: &str) -> &'a Value {
        fields
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v)
            .unwrap_or_else(|| panic!("missing field {key}"))
    }

    #[test]
    fn typed_scalars_bytes_and_round_trip() {
        let src = r#"
name = "ada"
count = 42
ratio = 1.5
flag = true
blob = b"6869"
"#;
        let doc = parse_str(src).unwrap();
        let fields = match &doc.root {
            Value::Object(f) => f,
            _ => panic!("object"),
        };
        assert_eq!(field(fields, "name"), &Value::Str("ada".into()));
        assert_eq!(field(fields, "count"), &Value::Int(42));
        assert_eq!(field(fields, "ratio"), &Value::Float(1.5));
        assert_eq!(field(fields, "flag"), &Value::Bool(true));
        assert_eq!(field(fields, "blob"), &Value::Bytes(b"hi".to_vec()));

        let again = parse_str(&canonical(&doc.root)).unwrap();
        assert_eq!(sort_obj(&again.root), sort_obj(&doc.root));
        let bytes = canonical_bytes(&doc.root);
        let decoded = decode_canonical_bytes(&bytes).unwrap();
        assert_eq!(sort_obj(&decoded), sort_obj(&doc.root));
    }

    fn sort_obj(v: &Value) -> Value {
        crate::value::sort_value(v)
    }

    #[test]
    fn comments_line_and_block_are_ignored() {
        let src = "a = 1 # trailing\n/* block\n comment */\nb = 2\n";
        let fields = root_fields(src);
        assert_eq!(fields.len(), 2);
        assert_eq!(field(&fields, "b"), &Value::Int(2));
    }

    #[test]
    fn multiline_strings() {
        let src = "note = \"\"\"\nhello\nworld\n\"\"\"\n";
        let fields = root_fields(src);
        assert_eq!(field(&fields, "note"), &Value::Str("hello\nworld".into()));
    }

    #[test]
    fn binary_literals_base64_and_hex() {
        let src = "h = b\"6869\"\nb64 = b64\"aGk=\"\n";
        let fields = root_fields(src);
        assert_eq!(field(&fields, "h"), &Value::Bytes(b"hi".to_vec()));
        assert_eq!(field(&fields, "b64"), &Value::Bytes(b"hi".to_vec()));
    }

    #[test]
    fn date_and_datetime() {
        let src = "born = d\"2020-01-02\"\nwhen = dt\"2020-01-02T03:04:05Z\"\n";
        let fields = root_fields(src);
        assert_eq!(field(&fields, "born"), &Value::Date("2020-01-02".into()));
        assert_eq!(
            field(&fields, "when"),
            &Value::DateTime("2020-01-02T03:04:05Z".into())
        );
    }

    #[test]
    fn anchors_and_references_reuse_without_duplication() {
        let src = r#"
&base = { host = "db", port = 5432 }
primary = *base
"#;
        let doc = parse_str(src).unwrap();
        assert!(doc.anchors.contains_key("base"));
        let primary = doc.root.get("primary").unwrap();
        let host = primary.get("host").unwrap();
        assert_eq!(host, &Value::Str("db".into()));
        // The anchor value and the resolved reference are equal (reused, not a second literal).
        assert_eq!(primary, doc.anchors.get("base").unwrap());
    }

    #[test]
    fn tagged_unions() {
        let src = "status = #Ok(1)\nmode = #Off\n";
        let fields = root_fields(src);
        assert_eq!(
            field(&fields, "status"),
            &Value::Tagged {
                tag: "Ok".into(),
                payload: Box::new(Value::Int(1)),
            }
        );
        assert_eq!(
            field(&fields, "mode"),
            &Value::Tagged {
                tag: "Off".into(),
                payload: Box::new(Value::Null),
            }
        );
    }

    #[test]
    fn imports_pull_another_file() {
        let mut files = HashMap::new();
        files.insert(
            "db.kson".to_string(),
            "$database\n    url = \"postgres://local\"\n".to_string(),
        );
        let src = "@import(\"db.kson\")\napp = \"demo\"\n";
        let doc = parse_with_imports(src, |path: &str| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| crate::error::LsonError::msg(format!("missing {path}")))
        })
        .unwrap();
        assert_eq!(doc.imports, vec!["db.kson".to_string()]);
        let db = doc.root.get("database").unwrap();
        assert_eq!(db.get("url"), Some(&Value::Str("postgres://local".into())));
        assert_eq!(doc.root.get("app"), Some(&Value::Str("demo".into())));
    }

    #[test]
    fn cyclic_import_returns_an_error() {
        let mut files = HashMap::new();
        files.insert("a.kson".to_string(), "@import(\"b.kson\")\n".to_string());
        files.insert("b.kson".to_string(), "@import(\"a.kson\")\n".to_string());
        let err = parse_with_imports("@import(\"a.kson\")\n", |path: &str| {
            files
                .get(path)
                .cloned()
                .ok_or_else(|| crate::error::LsonError::msg(format!("missing {path}")))
        })
        .expect_err("a cycle must be an error, not a crash");
        let rendered = err.to_string();
        assert!(
            rendered.contains("cyclic import"),
            "expected cyclic import error, got {rendered}"
        );
    }

    #[test]
    fn existing_env_and_section_files_still_parse() {
        let src = r#"
# comment
@env(DB_URL)
app_name = "lson-demo"
debug = false
max_workers = 4

$database
    url = DB_URL
    pool_min = 2
"#;
        let doc = parse_str(src).unwrap();
        assert_eq!(doc.env_decls, vec!["DB_URL".to_string()]);
        assert_eq!(
            doc.root.get("app_name"),
            Some(&Value::Str("lson-demo".into()))
        );
        let db = doc.root.get("database").unwrap();
        // Bare identifier matching a declared @env stays a reference until resolve_env.
        assert_eq!(db.get("url"), Some(&Value::Ref("DB_URL".into())));
        assert_eq!(db.get("pool_min"), Some(&Value::Int(2)));
    }

    #[test]
    fn canonical_is_deterministic() {
        let a = parse_str("b = 1\na = 2\n$z\n    y = 1\n    x = 2\n").unwrap();
        let b = parse_str("$z\n    x = 2\n    y = 1\na = 2\nb = 1\n").unwrap();
        assert_eq!(canonical(&a.root), canonical(&b.root));
        assert_eq!(canonical_bytes(&a.root), canonical_bytes(&b.root));
    }
}
