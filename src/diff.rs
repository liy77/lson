//! Semantic diff of two configuration trees.
//!
//! Comparison walks values, not source text: key order and insignificant
//! whitespace do not produce a change. A missing key, a type change, and a
//! different scalar do.

use crate::value::{sort_value, Value};

#[derive(Debug, Clone, PartialEq)]
pub enum Change {
    Added {
        path: String,
        value: Value,
    },
    Removed {
        path: String,
        value: Value,
    },
    Changed {
        path: String,
        from: Value,
        to: Value,
    },
}

impl Change {
    pub fn path(&self) -> &str {
        match self {
            Change::Added { path, .. }
            | Change::Removed { path, .. }
            | Change::Changed { path, .. } => path,
        }
    }
}

/// Diff `before` against `after`. Empty means the documents are equivalent.
pub fn diff(before: &Value, after: &Value) -> Vec<Change> {
    let mut out = Vec::new();
    walk(before, after, "", &mut out);
    out
}

fn walk(before: &Value, after: &Value, path: &str, out: &mut Vec<Change>) {
    match (before, after) {
        (Value::Object(a), Value::Object(b)) => {
            let a = object_map(a);
            let b = object_map(b);
            let mut keys: Vec<String> = a.keys().chain(b.keys()).map(|k| (*k).clone()).collect();
            keys.sort();
            keys.dedup();
            for key in &keys {
                let child = join(path, key);
                match (a.get(key), b.get(key)) {
                    (Some(left), Some(right)) => walk(left, right, &child, out),
                    (Some(left), None) => out.push(Change::Removed {
                        path: child,
                        value: (*left).clone(),
                    }),
                    (None, Some(right)) => out.push(Change::Added {
                        path: child,
                        value: (*right).clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (Value::Array(a), Value::Array(b)) => {
            let n = a.len().max(b.len());
            for i in 0..n {
                let child = format!("{path}[{i}]");
                match (a.get(i), b.get(i)) {
                    (Some(left), Some(right)) => walk(left, right, &child, out),
                    (Some(left), None) => out.push(Change::Removed {
                        path: child,
                        value: left.clone(),
                    }),
                    (None, Some(right)) => out.push(Change::Added {
                        path: child,
                        value: right.clone(),
                    }),
                    (None, None) => {}
                }
            }
        }
        (left, right) if values_eq(left, right) => {}
        (left, right) => out.push(Change::Changed {
            path: path.to_string(),
            from: left.clone(),
            to: right.clone(),
        }),
    }
}

fn values_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Float(x), Value::Float(y)) => x.to_bits() == y.to_bits() || (x - y).abs() < 1e-12,
        (Value::Float(x), Value::Int(y)) => (*x - *y as f64).abs() < 1e-12,
        (Value::Int(x), Value::Float(y)) => (*x as f64 - *y).abs() < 1e-12,
        _ => sort_value(a) == sort_value(b),
    }
}

fn object_map(fields: &[(String, Value)]) -> std::collections::BTreeMap<&String, &Value> {
    fields.iter().map(|(k, v)| (k, v)).collect()
}

fn join(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_string()
    } else {
        format!("{parent}.{child}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::{canonical, parse_str};

    #[test]
    fn semantic_diff_ignores_key_order_and_reports_real_changes() {
        let a = parse_str("b = 1\na = \"x\"\n$server\n    port = 1\n")
            .unwrap()
            .root;
        let b = parse_str("$server\n    port = 1\na = \"x\"\nb = 1\n")
            .unwrap()
            .root;
        assert!(diff(&a, &b).is_empty());
        assert_eq!(canonical(&a), canonical(&b));

        let c = parse_str("b = 2\na = \"x\"\n$server\n    port = 9\n    host = \"h\"\n")
            .unwrap()
            .root;
        let changes = diff(&a, &c);
        let paths: Vec<&str> = changes.iter().map(|c| c.path()).collect();
        assert!(paths.contains(&"b"));
        assert!(paths.contains(&"server.port"));
        assert!(paths.contains(&"server.host"));
        assert!(!paths.contains(&"a"));
    }
}
