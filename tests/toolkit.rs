//! Integration coverage of the shipped library surface.
//!
//! Each test calls the public function from its real start state (source text
//! or a reader) and checks the value the toolkit returns.

use lson::{
    apply_patch, canonical, canonical_bytes, diff, generate, infer, infer_model, migrate,
    model_from_source, open_fields, parse_reader, parse_str, parse_with_imports, resolve_with_map,
    seal_fields, to_cbor, to_json, to_yaml, validate, KeyStore, Migration, Op, Patch, Step, Value,
};
use std::collections::HashMap;
use std::io::Cursor;

#[test]
fn public_parse_canonical_and_binary_round_trip() {
    let src = r#"
# line
/* block */
name = "ada"
count = 7
ratio = 0.5
flag = false
blob = b"00ff"
when = dt"2024-05-06T07:08:09Z"
"#;
    let doc = parse_str(src).expect("parse");
    assert_eq!(doc.root.get("blob"), Some(&Value::Bytes(vec![0x00, 0xff])));
    let text = canonical(&doc.root);
    let again = parse_str(&text).unwrap();
    assert_eq!(canonical(&again.root), text);
    let bytes = canonical_bytes(&doc.root);
    assert!(bytes.starts_with(b"KSONBIN1"));
    let decoded = lson::kson::decode_canonical_bytes(&bytes).unwrap();
    assert_eq!(decoded.get("name"), Some(&Value::Str("ada".into())));
}

#[test]
fn public_imports_anchors_tags() {
    let mut files: HashMap<String, String> = HashMap::new();
    files.insert("extra.kson".into(), "extra = 3\n".into());
    let doc = parse_with_imports(
        "@import(\"extra.kson\")\n&a = { n = 1 }\nb = *a\nmode = #Off\n",
        |p: &str| {
            files
                .get(p)
                .cloned()
                .ok_or_else(|| lson::LsonError::msg("missing"))
        },
    )
    .unwrap();
    assert_eq!(doc.root.get("extra"), Some(&Value::Int(3)));
    assert_eq!(
        doc.root.get("b").and_then(|v| v.get("n")),
        Some(&Value::Int(1))
    );
    assert!(matches!(doc.root.get("mode"), Some(Value::Tagged { tag, .. }) if tag == "Off"));
}

#[test]
fn public_schema_kmodel_validation() {
    let doc = parse_str("app = \"demo\"\nport = 80\n").unwrap();
    let inferred = infer(&doc.root);
    let model = generate(&inferred);
    assert!(model.source.contains("app: String"));
    model.validate_value(&doc.root).unwrap();
    let generated = infer_model(&doc.root);
    assert_eq!(generated.source, model.source);

    let schema = model_from_source(
        "app: String\nport: Integer min=1 max=100\ntls: Bool? depends=cert\ncert: String?\n",
    )
    .unwrap();
    validate(&doc.root, &schema.schema).unwrap();
    let bad = parse_str("app = 1\nport = 200\ntls = true\n").unwrap();
    let issues = validate(&bad.root, &schema.schema).unwrap_err();
    let joined = issues
        .iter()
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(" | ");
    assert!(joined.contains("expected String"), "{joined}");
    assert!(joined.contains("above max"), "{joined}");
    assert!(joined.contains("depends on 'cert'"), "{joined}");
    let missing = parse_str("port = 1\n").unwrap();
    let issues = validate(&missing.root, &schema.schema).unwrap_err();
    assert!(issues.iter().any(|i| i.path == "app"));
}

#[test]
fn public_migration_env_secrets_convert_diff_patch_stream() {
    let old = parse_str("schema_version = 1\ndb = \"postgres://x\"\n")
        .unwrap()
        .root;
    let migrated = migrate(
        &old,
        &Migration::new(
            1,
            2,
            vec![Step::Rename {
                from: "db".into(),
                to: "database.url".into(),
            }],
        ),
    )
    .unwrap();
    assert_eq!(migrated.get("schema_version"), Some(&Value::Int(2)));
    assert_eq!(
        migrated.get("database").and_then(|d| d.get("url")),
        Some(&Value::Str("postgres://x".into()))
    );

    let env_doc = parse_str("@env(HOST)\nhost = HOST\n").unwrap();
    let mut vars = HashMap::new();
    vars.insert("HOST".into(), "local".into());
    let resolved = resolve_with_map(&env_doc, &vars).unwrap();
    assert_eq!(resolved.get("host"), Some(&Value::Str("local".into())));

    let mut ks = KeyStore::new();
    ks.insert_passphrase_bytes("kid", b"separate-key-material");
    let sealed = seal_fields(&resolved, &["host"], "kid", &ks).unwrap();
    assert!(!format!("{}", sealed.get("host").unwrap()).contains("local"));
    let opened = open_fields(&sealed, &["host"], &ks).unwrap();
    assert_eq!(opened.get("host"), Some(&Value::Str("local".into())));
    let mut wrong = KeyStore::new();
    wrong.insert_passphrase_bytes("kid", b"other-key");
    assert!(open_fields(&sealed, &["host"], &wrong).is_err());

    let json = to_json(&opened).unwrap();
    assert!(json.contains("local"));
    let yaml = to_yaml(&opened).unwrap();
    assert!(yaml.contains("local"));
    let cbor = to_cbor(&opened).unwrap();
    let back = lson::convert::from_cbor_slice(&cbor).unwrap();
    assert_eq!(back.get("host"), Some(&Value::Str("local".into())));

    let other = parse_str("host = \"remote\"\nextra = 1\n").unwrap().root;
    let changes = diff(&opened, &other);
    assert!(changes.iter().any(|c| c.path() == "host"));
    assert!(changes.iter().any(|c| c.path() == "extra"));

    let patched = apply_patch(
        &opened,
        &Patch::new(vec![Op::Set {
            path: "host".into(),
            value: Value::Str("remote".into()),
        }]),
    )
    .unwrap();
    assert_eq!(patched.get("host"), Some(&Value::Str("remote".into())));

    let src = "a = 1\nb = 2\n$s\n    c = 3\n";
    let streamed = parse_reader(Cursor::new(src), 3).unwrap();
    assert_eq!(
        canonical(&streamed.root),
        canonical(&parse_str(src).unwrap().root)
    );
}
