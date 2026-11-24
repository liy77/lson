//! Schema inference, validation, and KModel generation.
//!
//! A [`Schema`] is a typed tree the validator walks against a [`Value`].
//! Inference builds one from an existing document. KModel text is the
//! human-readable form of that tree (and the form the CLI still accepts).

use crate::error::LsonError;
use crate::kson::{self, parse_str};
use crate::value::Value;
use std::fmt;

/// Constraint on a single field or on the root.
#[derive(Debug, Clone, PartialEq)]
pub struct Schema {
    pub ty: SchemaType,
    pub optional: bool,
    /// Inclusive bounds for Integer / Float.
    pub min: Option<f64>,
    pub max: Option<f64>,
    /// `field` must be present when this field is present (and vice-style
    /// listed in `depends_on`).
    pub depends_on: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SchemaType {
    Any,
    String,
    Integer,
    Float,
    Bool,
    Bytes,
    Date,
    DateTime,
    Array(Box<Schema>),
    Object(Vec<Field>),
    /// Tagged union. Empty `variants` accepts any tag.
    Tagged(Vec<Variant>),
    Ref,
    Null,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub schema: Schema,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub tag: String,
    pub payload: Schema,
}

impl Schema {
    pub fn required(ty: SchemaType) -> Self {
        Schema {
            ty,
            optional: false,
            min: None,
            max: None,
            depends_on: Vec::new(),
        }
    }

    pub fn optional(ty: SchemaType) -> Self {
        let mut s = Self::required(ty);
        s.optional = true;
        s
    }
}

impl fmt::Display for SchemaType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SchemaType::Any => write!(f, "Any"),
            SchemaType::String => write!(f, "String"),
            SchemaType::Integer => write!(f, "Integer"),
            SchemaType::Float => write!(f, "Float"),
            SchemaType::Bool => write!(f, "Bool"),
            SchemaType::Bytes => write!(f, "Bytes"),
            SchemaType::Date => write!(f, "Date"),
            SchemaType::DateTime => write!(f, "DateTime"),
            SchemaType::Array(inner) => write!(f, "Array<{}>", type_name(&inner.ty)),
            SchemaType::Object(_) => write!(f, "Object"),
            SchemaType::Tagged(_) => write!(f, "Tagged"),
            SchemaType::Ref => write!(f, "Ref"),
            SchemaType::Null => write!(f, "Null"),
        }
    }
}

fn type_name(ty: &SchemaType) -> String {
    ty.to_string()
}

/// Infer a schema from a document. Every present field is required.
/// Numbers that are whole become Integer; others become Float.
pub fn infer(value: &Value) -> Schema {
    Schema::required(infer_type(value))
}

fn infer_type(value: &Value) -> SchemaType {
    match value {
        Value::Int(_) => SchemaType::Integer,
        Value::Float(_) => SchemaType::Float,
        Value::Str(_) => SchemaType::String,
        Value::Bool(_) => SchemaType::Bool,
        Value::Bytes(_) => SchemaType::Bytes,
        Value::Date(_) => SchemaType::Date,
        Value::DateTime(_) => SchemaType::DateTime,
        Value::Null => SchemaType::Null,
        Value::Ref(_) => SchemaType::Ref,
        Value::Array(items) => {
            let inner = if let Some(first) = items.first() {
                infer(first)
            } else {
                Schema::required(SchemaType::Any)
            };
            SchemaType::Array(Box::new(inner))
        }
        Value::Object(fields) => {
            let fields = fields
                .iter()
                .map(|(name, v)| Field {
                    name: name.clone(),
                    schema: infer(v),
                })
                .collect();
            SchemaType::Object(fields)
        }
        Value::Tagged { tag, payload } => SchemaType::Tagged(vec![Variant {
            tag: tag.clone(),
            payload: infer(payload),
        }]),
    }
}

/// Render a schema as KModel text (`key: Type`, `$section`).
pub fn to_kmodel(schema: &Schema) -> String {
    let mut out = String::new();
    match &schema.ty {
        SchemaType::Object(fields) => write_object_fields(fields, 0, &mut out),
        other => {
            out.push_str("root: ");
            out.push_str(&kmodel_type(other, schema.optional));
            out.push('\n');
        }
    }
    out
}

fn write_object_fields(fields: &[Field], indent: usize, out: &mut String) {
    let pad = "    ".repeat(indent);
    for field in fields {
        match &field.schema.ty {
            SchemaType::Object(children) => {
                out.push_str(&pad);
                out.push('$');
                out.push_str(&field.name);
                if field.schema.optional {
                    out.push('?');
                }
                out.push('\n');
                write_object_fields(children, indent + 1, out);
            }
            other => {
                out.push_str(&pad);
                out.push_str(&field.name);
                out.push_str(": ");
                out.push_str(&kmodel_type(other, field.schema.optional));
                if let Some(min) = field.schema.min {
                    out.push_str(&format!(" min={min}"));
                }
                if let Some(max) = field.schema.max {
                    out.push_str(&format!(" max={max}"));
                }
                if !field.schema.depends_on.is_empty() {
                    out.push_str(" depends=");
                    out.push_str(&field.schema.depends_on.join(","));
                }
                out.push('\n');
            }
        }
    }
}

fn kmodel_type(ty: &SchemaType, optional: bool) -> String {
    let base = match ty {
        SchemaType::Array(inner) => format!("Array<{}>", kmodel_type(&inner.ty, inner.optional)),
        other => other.to_string(),
    };
    if optional {
        format!("{base}?")
    } else {
        base
    }
}

/// Parse a `.kmodel` document into a [`Schema`].
///
/// Extends the historical `key: Type` / `$section` form with `min=`, `max=`,
/// and `depends=a,b` suffixes, plus Bytes / Date / DateTime / Tagged.
pub fn parse_kmodel(src: &str) -> Result<Schema, LsonError> {
    let mut root_fields: Vec<Field> = Vec::new();
    let mut stack: Vec<(usize, String)> = Vec::new();

    for (idx, raw) in src.lines().enumerate() {
        let line_no = idx + 1;
        let line = strip_hash_comment(raw);
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let indent = line.len() - line.trim_start().len();
        while stack.last().map(|(d, _)| *d >= indent).unwrap_or(false) {
            stack.pop();
        }
        if let Some(name) = trimmed.strip_prefix('$') {
            let optional = name.trim().ends_with('?');
            let name = name.trim().trim_end_matches('?').to_string();
            insert_field_at(
                &mut root_fields,
                &stack,
                Field {
                    name: name.clone(),
                    schema: Schema {
                        ty: SchemaType::Object(Vec::new()),
                        optional,
                        min: None,
                        max: None,
                        depends_on: Vec::new(),
                    },
                },
            )?;
            stack.push((indent, name));
            continue;
        }
        let (key, rest) = trimmed.split_once(':').ok_or_else(|| {
            LsonError::parse(line_no, format!("expected key: Type, got '{trimmed}'"))
        })?;
        let field = parse_field_spec(key.trim(), rest.trim(), line_no)?;
        insert_field_at(&mut root_fields, &stack, field)?;
    }

    Ok(Schema::required(SchemaType::Object(root_fields)))
}

fn strip_hash_comment(line: &str) -> String {
    let mut out = String::new();
    for c in line.chars() {
        if c == '#' {
            break;
        }
        out.push(c);
    }
    out
}

fn insert_field_at(
    root: &mut Vec<Field>,
    stack: &[(usize, String)],
    field: Field,
) -> Result<(), LsonError> {
    let mut fields = root;
    for (_, name) in stack {
        let slot = fields
            .iter_mut()
            .find(|f| &f.name == name)
            .ok_or_else(|| LsonError::msg(format!("missing section {name}")))?;
        match &mut slot.schema.ty {
            SchemaType::Object(children) => fields = children,
            _ => return Err(LsonError::msg(format!("{name} is not a section"))),
        }
    }
    fields.push(field);
    Ok(())
}

fn parse_field_spec(name: &str, rest: &str, line_no: usize) -> Result<Field, LsonError> {
    let mut type_tok = rest;
    let mut min = None;
    let mut max = None;
    let mut depends_on = Vec::new();
    for token in rest.split_whitespace().skip(1) {
        if let Some(v) = token.strip_prefix("min=") {
            min = Some(parse_bound(v, line_no)?);
        } else if let Some(v) = token.strip_prefix("max=") {
            max = Some(parse_bound(v, line_no)?);
        } else if let Some(v) = token.strip_prefix("depends=") {
            depends_on = v
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
        }
    }
    if let Some(space) = rest.find(char::is_whitespace) {
        type_tok = &rest[..space];
    }
    let optional = type_tok.ends_with('?');
    let type_tok = type_tok.trim_end_matches('?');
    let ty = parse_type_token(type_tok, line_no)?;
    Ok(Field {
        name: name.to_string(),
        schema: Schema {
            ty,
            optional,
            min,
            max,
            depends_on,
        },
    })
}

fn parse_bound(v: &str, line_no: usize) -> Result<f64, LsonError> {
    v.parse::<f64>()
        .map_err(|_| LsonError::parse(line_no, format!("bad bound '{v}'")))
}

fn parse_type_token(tok: &str, line_no: usize) -> Result<SchemaType, LsonError> {
    if let Some(inner) = tok.strip_prefix("Array<").and_then(|s| s.strip_suffix('>')) {
        let optional = inner.ends_with('?');
        let inner = inner.trim_end_matches('?');
        let ty = parse_type_token(inner, line_no)?;
        return Ok(SchemaType::Array(Box::new(Schema {
            ty,
            optional,
            min: None,
            max: None,
            depends_on: Vec::new(),
        })));
    }
    Ok(match tok {
        "Any" => SchemaType::Any,
        "String" => SchemaType::String,
        "Char" => SchemaType::String,
        "Integer" => SchemaType::Integer,
        "Float" => SchemaType::Float,
        "Bool" => SchemaType::Bool,
        "Bytes" => SchemaType::Bytes,
        "Date" => SchemaType::Date,
        "DateTime" => SchemaType::DateTime,
        "Tagged" => SchemaType::Tagged(Vec::new()),
        "Ref" => SchemaType::Ref,
        "Null" => SchemaType::Null,
        "Object" => SchemaType::Object(Vec::new()),
        other => {
            return Err(LsonError::parse(line_no, format!("unknown type '{other}'")));
        }
    })
}

/// One validation failure. The engine collects every problem it can.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationIssue {
    pub path: String,
    pub message: String,
}

impl fmt::Display for ValidationIssue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.path.is_empty() {
            write!(f, "{}", self.message)
        } else {
            write!(f, "{}: {}", self.path, self.message)
        }
    }
}

/// Validate `value` against `schema`.
///
/// `Ok(())` means the document conforms. `Err` lists every issue found
/// (wrong type, out of range, missing required field, broken dependency).
pub fn validate(value: &Value, schema: &Schema) -> Result<(), Vec<ValidationIssue>> {
    let mut issues = Vec::new();
    check_value(value, schema, "", &mut issues);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(issues)
    }
}

fn check_value(value: &Value, schema: &Schema, path: &str, issues: &mut Vec<ValidationIssue>) {
    if matches!(value, Value::Null) && schema.optional {
        return;
    }
    if !type_matches(value, &schema.ty) {
        issues.push(ValidationIssue {
            path: path.to_string(),
            message: format!("expected {}, got {}", schema.ty, value.type_name()),
        });
        return;
    }
    match (&schema.ty, value) {
        (SchemaType::Integer, Value::Int(n)) => check_bounds(*n as f64, schema, path, issues),
        (SchemaType::Float, Value::Float(n)) => check_bounds(*n, schema, path, issues),
        (SchemaType::Float, Value::Int(n)) => check_bounds(*n as f64, schema, path, issues),
        (SchemaType::Array(inner), Value::Array(items)) => {
            for (i, item) in items.iter().enumerate() {
                let child = join(path, &format!("[{i}]"));
                check_value(item, inner, &child, issues);
            }
        }
        (SchemaType::Object(fields), Value::Object(present)) => {
            for field in fields {
                let child = join(path, &field.name);
                match present.iter().find(|(k, _)| k == &field.name) {
                    Some((_, v)) => check_value(v, &field.schema, &child, issues),
                    None if !field.schema.optional => issues.push(ValidationIssue {
                        path: child,
                        message: "missing required field".into(),
                    }),
                    None => {}
                }
            }
            check_dependencies(fields, present, path, issues);
        }
        (SchemaType::Tagged(variants), Value::Tagged { tag, payload }) if !variants.is_empty() => {
            match variants.iter().find(|v| &v.tag == tag) {
                Some(variant) => {
                    check_value(payload, &variant.payload, &join(path, tag), issues);
                }
                None => issues.push(ValidationIssue {
                    path: path.to_string(),
                    message: format!("unknown tag '{tag}'"),
                }),
            }
        }
        _ => {}
    }
}

fn check_bounds(n: f64, schema: &Schema, path: &str, issues: &mut Vec<ValidationIssue>) {
    if let Some(min) = schema.min {
        if n < min {
            issues.push(ValidationIssue {
                path: path.to_string(),
                message: format!("{n} is below min {min}"),
            });
        }
    }
    if let Some(max) = schema.max {
        if n > max {
            issues.push(ValidationIssue {
                path: path.to_string(),
                message: format!("{n} is above max {max}"),
            });
        }
    }
}

fn check_dependencies(
    fields: &[Field],
    present: &[(String, Value)],
    path: &str,
    issues: &mut Vec<ValidationIssue>,
) {
    let has = |name: &str| present.iter().any(|(k, _)| k == name);
    for field in fields {
        if field.schema.depends_on.is_empty() || !has(&field.name) {
            continue;
        }
        for dep in &field.schema.depends_on {
            if !has(dep) {
                issues.push(ValidationIssue {
                    path: join(path, &field.name),
                    message: format!("depends on '{dep}', which is missing"),
                });
            }
        }
    }
}

fn type_matches(value: &Value, ty: &SchemaType) -> bool {
    match (ty, value) {
        (SchemaType::Any, _) => true,
        (SchemaType::String, Value::Str(_)) => true,
        (SchemaType::Integer, Value::Int(_)) => true,
        (SchemaType::Float, Value::Float(_) | Value::Int(_)) => true,
        (SchemaType::Bool, Value::Bool(_)) => true,
        (SchemaType::Bytes, Value::Bytes(_)) => true,
        (SchemaType::Date, Value::Date(_)) => true,
        (SchemaType::DateTime, Value::DateTime(_)) => true,
        (SchemaType::Array(_), Value::Array(_)) => true,
        (SchemaType::Object(_), Value::Object(_)) => true,
        (SchemaType::Tagged(_), Value::Tagged { .. }) => true,
        (SchemaType::Ref, Value::Ref(_)) => true,
        (SchemaType::Null, Value::Null) => true,
        _ => false,
    }
}

fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else if child.starts_with('[') {
        format!("{parent}{child}")
    } else {
        format!("{parent}.{child}")
    }
}

/// A generated model: the schema plus the KModel source a caller can write to disk.
#[derive(Debug, Clone, PartialEq)]
pub struct KModel {
    pub schema: Schema,
    pub source: String,
}

impl KModel {
    pub fn validate_value(&self, value: &Value) -> Result<(), Vec<ValidationIssue>> {
        validate(value, &self.schema)
    }

    pub fn validate_kson(&self, src: &str) -> Result<(), LsonError> {
        let doc = parse_str(src)?;
        self.validate_value(&doc.root).map_err(|issues| {
            LsonError::Validation(
                issues
                    .iter()
                    .map(|i| i.to_string())
                    .collect::<Vec<_>>()
                    .join("; "),
            )
        })
    }
}

/// Generate a KModel from a schema (the code-generation step of the toolkit).
pub fn generate(schema: &Schema) -> KModel {
    let source = to_kmodel(schema);
    KModel {
        schema: schema.clone(),
        source,
    }
}

/// Infer a schema and emit its KModel source in one step.
pub fn infer_model(value: &Value) -> KModel {
    generate(&infer(value))
}

/// Load a KModel from source text.
pub fn model_from_source(src: &str) -> Result<KModel, LsonError> {
    let schema = parse_kmodel(src)?;
    Ok(KModel {
        source: src.to_string(),
        schema,
    })
}

/// Convenience used by the CLI: parse KSON text and validate it.
pub fn validate_source(
    kson_src: &str,
    kmodel_src: &str,
) -> Result<crate::value::Document, LsonError> {
    let doc = kson::parse_str(kson_src)?;
    let model = model_from_source(kmodel_src)?;
    model.validate_value(&doc.root).map_err(|issues| {
        LsonError::Validation(
            issues
                .iter()
                .map(|i| i.to_string())
                .collect::<Vec<_>>()
                .join("; "),
        )
    })?;
    Ok(doc)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Value {
        parse_str(
            r#"
app = "demo"
port = 8080
$server
    host = "local"
"#,
        )
        .unwrap()
        .root
    }

    #[test]
    fn inference_and_kmodel_generation() {
        let value = sample();
        let model = infer_model(&value);
        assert!(model.source.contains("app: String"));
        assert!(model.source.contains("port: Integer"));
        assert!(model.source.contains("$server"));
        assert!(model.source.contains("host: String"));
        // Generated model accepts the document it was inferred from.
        model.validate_value(&value).unwrap();
    }

    #[test]
    fn validation_accepts_conforming_and_rejects_type_limit_required_dependency() {
        let src = r#"
app: String
port: Integer min=1 max=65535
tls: Bool? depends=cert
cert: String?
"#;
        let schema = parse_kmodel(src).unwrap();

        let ok = parse_str("app = \"demo\"\nport = 80\n").unwrap().root;
        validate(&ok, &schema).unwrap();

        let bad_type = parse_str("app = 1\nport = 80\n").unwrap().root;
        let err = validate(&bad_type, &schema).unwrap_err();
        assert!(err.iter().any(|i| i.message.contains("expected String")));

        let bad_limit = parse_str("app = \"demo\"\nport = 70000\n").unwrap().root;
        let err = validate(&bad_limit, &schema).unwrap_err();
        assert!(err.iter().any(|i| i.message.contains("above max")));

        let missing = parse_str("port = 80\n").unwrap().root;
        let err = validate(&missing, &schema).unwrap_err();
        assert!(err
            .iter()
            .any(|i| i.path == "app" && i.message.contains("required")));

        let broken = parse_str("app = \"demo\"\nport = 80\ntls = true\n")
            .unwrap()
            .root;
        let err = validate(&broken, &schema).unwrap_err();
        assert!(err.iter().any(|i| i.message.contains("depends on 'cert'")));
    }
}
