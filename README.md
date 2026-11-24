<div align="center">
  <img src="./assets/lson-logo.svg" alt="LSON" width="480"/>
  <br/><br/>
  <img src="./assets/kson-logo.svg" alt="KSON" width="480"/>
  &nbsp;&nbsp;
  <img src="./assets/kmodel-logo.svg" alt="KModel" width="480"/>
  <br/><br/>

  [![CI](https://github.com/liy77/lson/workflows/CI/badge.svg)](https://github.com/liy77/lson/actions)
  [![Release](https://github.com/liy77/lson/workflows/Release/badge.svg)](https://github.com/liy77/lson/releases)
  [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
</div>

---

**LSON** is a typed configuration toolkit. **KSON** is the document format. **KModel** is the schema. The crate around them infers schemas, validates, migrates, resolves environment variables, encrypts secret fields, converts, diffs, and patches. It is not a JSON clone: values keep their type (bytes, dates, tags, references) all the way through parse and canonical serialization.

| Piece | What it does |
|-------|----------------|
| **KSON** | Typed document: scalars, bytes, dates, comments, multiline strings, anchors, tags, imports |
| **KModel** | Schema, inference from an existing file, validation, generated model text |
| **LSON** | Toolkit plus whole-file ChaCha20-Poly1305 seals (`LSON/1`) |

```
LSON
├── KSON        lexer, parser, canonical serializer, binary encoding
├── KModel      type system, schema, validation, code generation
├── KSecurity   field encryption, key store, whole-file seal/open
└── KTools      diff, patch, migration, JSON / YAML / CBOR conversion
```

---

## Installation

### Pre-built binaries

Download the latest release from the [Releases page](https://github.com/liy77/lson/releases):

| Platform | Binary |
|----------|--------|
| Linux x86_64 | `lson-linux-x86_64` |
| Windows x86_64 | `lson-windows-x86_64.exe` |
| macOS Apple Silicon | `lson-macos-arm64` |

**Linux / macOS**
```sh
chmod +x lson-*
sudo mv lson-* /usr/local/bin/lson
```

### From source
```sh
git clone https://github.com/liy77/lson.git
cd lson
cargo build --release
```

As a library, depend on the crate and call the functions in [Library](#library). The CLI is a thin wrapper over the same API.

---

## KSON format

A document is `key = value` plus indentation sections. Comments, blank lines, and the order of keys do not change the value tree.

```kson
# examples/app.kson
# Line comments start with "# " (hash, then a space or end of line).

@env(DB_URL)
@env(APP_SECRET)

app_name    = "lson-demo"
version     = "1.0.0"
debug       = false
max_workers = 4
ratio       = 1.5

$server
    host = "0.0.0.0"
    port = 8080
    tls  = true

$database
    url      = DB_URL          # bare name declared in @env
    pool_min = 2
    pool_max = 20

    $replica
        url      = "postgres://replica:5432/db"
        readonly = true

$auth
    secret   = APP_SECRET
    expires  = 3600
    provider = "local"
```

`$database` becomes an object. A nested `$replica` is an object inside it. `@env(NAME)` declares a variable; a bare `NAME` on the right-hand side stays a reference until [environment resolution](#environment-resolver).

### Comments

```kson
# this whole line is a comment
port = 8080   # trailing comment

/* block comments may
   span lines and are stripped
   before parsing */
host = "local"
```

`#Ok` and `#Off` are tagged unions, not comments. A line comment is `#` followed by whitespace (or nothing).

### Typed values

| Form | Value |
|------|--------|
| `42` | integer |
| `1.5` | float |
| `"text"` | string, with `\n` `\t` `\\` `\"` |
| `true` / `false` | bool |
| `null` | null |
| `b"6869"` | bytes, hex (`hi`) |
| `b64"aGk="` | bytes, standard base64 |
| `d"2020-01-02"` | date |
| `dt"2020-01-02T03:04:05Z"` | datetime, trailing `Z` |
| `[1, "a", true]` | array |
| `{ host = "db", port = 5432 }` | inline object |
| `#Ok(1)` / `#Off` | tagged union |
| `*name` | reference to an anchor |

```kson
blob   = b"6869"
raw    = b64"aGk="
born   = d"2020-01-02"
when   = dt"2020-01-02T03:04:05Z"
peers  = ["10.0.0.1", "10.0.0.2"]
status = #Ok(1)
mode   = #Off
```

### Multiline strings

Triple quotes keep newlines. The opening newline after `"""` is dropped, so the value is the body, not a leading blank line.

```kson
note = """
hello
world
"""
```

That parses as the string `hello\nworld`.

### Anchors and references

`&name` stores a value. `*name` reuses that same value later in the file. The reference is resolved at parse time, so the tree does not contain a second copy of the literal.

```kson
&base = { host = "db", port = 5432 }
primary = *base
# primary.host == "db", primary is equal to the anchor
```

### Imports

`@import("path")` pulls another KSON file into the document. Keys already present in the parent win. A cycle (`a` imports `b` imports `a`) is an error, not a crash.

```kson
@import("db.kson")
app = "demo"
```

```kson
# db.kson
$database
    url = "postgres://local"
```

From the library, pass a resolver. Tests inject a map; an application reads the disk.

```rust
let doc = lson::parse_with_imports(src, |path: &str| {
    std::fs::read_to_string(path).map_err(|e| lson::LsonError::msg(e.to_string()))
})?;
```

### Canonical form

`lson::canonical` sorts keys and uses one encoding per type, so two documents that mean the same thing produce the same text. `lson::canonical_bytes` writes the same tree as length-prefixed `KSONBIN1` bytes.

```sh
lson raw compile -f config.kson -t canonical
```

```
app_name = "lson-demo"
$database
    pool = 10
    url = "postgres://local"
debug = false
```

---

## KModel

A `.kmodel` file is the human-readable schema: `key: Type` and `$section`. Optional types end with `?`.

```kmodel
# config.kmodel

app_name:    String
version:     String
debug:       Bool
max_workers: Integer
ratio:       Float?
blob:        Bytes?
born:        Date?

$server
    host: String
    port: Integer min=1 max=65535
    tls:  Bool

$database
    url:      String
    pool_min: Integer
    pool_max: Integer
    timeout:  Integer

$auth?
    secret:   String
    expires:  Integer
    provider: String
```

**Types:** `String` · `Integer` · `Float` · `Bool` · `Char` · `Bytes` · `Date` · `DateTime` · `Any` · `Array<T>` · `Tagged` · `T?`

Limits and dependencies are suffixes. `depends` names sibling fields that must also be present:

```kmodel
port: Integer min=1 max=65535
tls:  Bool? depends=cert
cert: String?
tags: Array<String>
```

Validate from the CLI (a failing document exits non-zero; the library returns the issue list):

```sh
lson compile -f config.kson --kmodel config.kmodel -t json
```

### Inference and generation

Give the library a parsed document and it writes the KModel that describes it. Every field that is present becomes required.

```rust
let doc = lson::parse_str(src)?;
let model = lson::infer_model(&doc.root);
println!("{}", model.source);
// app_name: String
// debug: Bool
// $server
//     host: String
//     port: Integer

model.validate_value(&doc.root)?;
```

`lson::infer` returns the schema tree. `lson::generate` turns a schema you built by hand back into KModel text. `lson::model_from_source` parses a `.kmodel` file.

### What validation rejects

| Case | Example | Result |
|------|---------|--------|
| Wrong type | `app = 1` against `app: String` | `expected String, got Integer` |
| Out of range | `port = 70000` with `max=65535` | `70000 is above max 65535` |
| Missing required | no `app` | `app: missing required field` |
| Broken dependency | `tls = true` and no `cert` | `depends on 'cert', which is missing` |

A document that satisfies the schema returns `Ok(())`.

---

## Toolkit

### Environment resolver

Declared `@env` names and `${NAME}` / `$NAME` inside strings are substituted through a lookup you supply. A missing name is an error. The lookup is a closure, so tests pass a map and never touch the process environment.

```kson
@env(DB_URL)
@env(PORT)
url  = DB_URL
port = PORT
note = "connect ${DB_URL}"
```

```rust
use std::collections::HashMap;

let doc = lson::parse_str(src)?;
let mut vars = HashMap::new();
vars.insert("DB_URL".into(), "postgres://db".into());
vars.insert("PORT".into(), "5432".into());
let resolved = lson::resolve_with_map(&doc, &vars)?;
// url  -> "postgres://db"
// port -> 5432          integers in the env value stay integers
// note -> "connect postgres://db"
```

`lson::resolve(&doc, |name| std::env::var(name).ok())` is the process-environment lookup.

### Secret fields

ChaCha20-Poly1305 on individual fields. The document stores an envelope (`alg`, `kid`, `nonce`, `ct`). The key bytes live in a `KeyStore` and are never written into the file. A wrong key fails closed.

This is separate from the whole-file `LSON/1` seal below. Field keys are not derived with Argon2id; pass a 32-byte key, or any shorter secret (it is expanded with SHA-256).

```rust
let doc = lson::parse_str("user = \"ada\"\ntoken = \"s3cret\"\n")?;
let mut keys = lson::KeyStore::new();
keys.insert_passphrase_bytes("k1", b"correct horse");

let sealed = lson::seal_fields(&doc.root, &["token"], "k1", &keys)?;
// sealed token is { alg, kid, nonce, ct } — "s3cret" is not in the tree

let opened = lson::open_fields(&sealed, &["token"], &keys)?;
assert_eq!(opened.get("token"), Some(&lson::Value::Str("s3cret".into())));
```

### Migration

A migration is an ordered list of steps from one schema version to the next. `schema_version` is set to `to_version` when the steps finish.

```rust
use lson::{migrate, Migration, Step, Value};

let old = lson::parse_str(
    "schema_version = 1\nname = \"ada\"\ndb_url = \"postgres://old\"\nworkers = 2\n",
)?.root;

let migration = Migration::new(1, 2, vec![
    Step::Rename { from: "db_url".into(), to: "database.url".into() },
    Step::AddInt { path: "workers".into(), n: 1 },
    Step::Remove { path: "name".into() },
    Step::Set { path: "database.pool".into(), value: Value::Int(4) },
]);

let next = migrate(&old, &migration)?;
// schema_version = 2
// workers = 3
// database.url = "postgres://old"
// database.pool = 4
// name is gone
```

Steps: `Rename`, `Remove`, `Set`, `Copy`, `AddInt`. Paths are dotted (`database.url`).

### Diff and patch

Diff compares values, not source text. Key order and whitespace do not show up as changes.

```rust
let before = lson::parse_str("b = 1\na = \"x\"\n$server\n    port = 1\n")?.root;
let after = lson::parse_str("a = \"x\"\nb = 2\n$server\n    port = 9\n    host = \"h\"\n")?.root;

for change in lson::diff(&before, &after) {
    println!("{}", change.path());
}
// b
// server.port
// server.host
```

A patch names the fields to change and leaves the rest alone.

```rust
use lson::{apply_patch, Op, Patch, Value};

let doc = lson::parse_str("name = \"ada\"\nport = 1\nflag = true\n")?.root;
let patched = apply_patch(&doc, &Patch::new(vec![
    Op::Set { path: "port".into(), value: Value::Int(8080) },
    Op::Set { path: "server.host".into(), value: Value::Str("local".into()) },
    Op::Remove { path: "flag".into() },
]))?;
// name is still "ada"; port is 8080; flag is gone; server.host is "local"
```

### JSON, YAML, CBOR

Plain scalars stay plain. Bytes, dates, datetimes, tags, and refs become small tagged objects so the conversion can be read back.

```rust
let doc = lson::parse_str("name = \"ada\"\nport = 1\nblob = b\"6869\"\n")?;
let json = lson::to_json(&doc.root)?;
let yaml = lson::to_yaml(&doc.root)?;
let cbor = lson::to_cbor(&doc.root)?;
let back = lson::convert::from_cbor_slice(&cbor)?;
```

```sh
lson raw compile -f config.kson -t json
lson raw compile -f config.kson -t yaml
lson raw compile -f config.kson -t cbor > config.cbor
```

`lson::compile_to_json` parses and emits canonical (sorted) JSON in one call. That is what `lson raw compile -t json` prints.

### Streaming parser

`lson::parse_reader` reads any `Read` in chunks and yields the same tree as `parse_str`. The caller does not assemble the file into one `String` first. A chunk may end inside a multibyte character; those bytes are held until the sequence completes. Block comments that cross a chunk boundary are skipped, same as in a full parse.

```rust
let file = std::fs::File::open("config.kson")?;
let streamed = lson::parse_reader(file, 64)?;
let full = lson::parse_str(&std::fs::read_to_string("config.kson")?)?;
assert_eq!(
    lson::canonical(&streamed.root),
    lson::canonical(&full.root)
);
```

---

## Whole-file seal

`lson compile` (output type `lson`, the default) seals the **source text** with ChaCha20-Poly1305. The key is derived with Argon2id. The header carries a SHA-256 of that plaintext so drift can be checked without decrypting. Env references are not substituted before the seal; resolve them in the library if the sealed document should already contain the values.

```
LSON/1
SALT:<hex>       random 16 bytes for Argon2id
NONCE:<hex>      random 12 bytes for ChaCha20-Poly1305
KSON-HASH:<hex>  SHA-256 of the plaintext

<base64 ciphertext + Poly1305 tag>
```

The passphrase comes from `--key`, then `LSON_KEY`, then an interactive prompt. Two seals of the same plaintext differ, because salt and nonce are random.

```sh
LSON_KEY=secret lson compile -f config.kson -o config.lson
LSON_KEY=secret lson parse config.lson
lson verify -f config.kson --lson config.lson
```

```rust
let sealed = lson::utils::lson::encrypt(src, "secret")?;
assert!(sealed.starts_with("LSON/1\n"));
let opened = lson::utils::lson::decrypt(&sealed, "secret")?;
let hash = lson::utils::lson::kson_hash_from_lson(&sealed)?;
```

---

## CLI

```sh
# Seal (default output type)
lson compile -f config.kson
LSON_KEY=secret lson compile -f config.kson -o config.lson

# Textual targets. json and canonical are deterministic.
lson compile -f config.kson -t json -o config.json
lson compile -f config.kson -t yaml -o config.yaml
lson compile -f config.kson -t canonical -o config.kson.txt
lson compile -f config.kson --kmodel config.kmodel -t json

# Decrypt
lson parse config.lson

# Drift check, no passphrase
lson verify -f config.kson --lson config.lson

# Lockfile: canonical JSON plus the KSON hash
lson lock -f config.kson -o config.lock

# stdout, for pipes
lson raw compile -f config.kson -t json
lson raw compile --text 'port = 8080' -t json
```

`--text` compiles a snippet. `--output` is required when `--text` is used with `lson compile` (the file-less form has no default path).

---

## Library

```toml
[dependencies]
lson = { path = "." }
```

Public entry points:

| Function | Role |
|----------|------|
| `parse_str` | Parse one document. `@import` without a resolver is an error. |
| `parse_with_imports` | Parse, loading `@import` through a closure. |
| `parse_reader` | Same tree, read in chunks. |
| `canonical` / `canonical_bytes` | Deterministic text and `KSONBIN1`. |
| `infer` / `infer_model` / `generate` | Schema from a value, or KModel text from a schema. |
| `model_from_source` / `validate` | Load a `.kmodel` and check a value. |
| `resolve` / `resolve_with_map` | Substitute `@env` and `$NAME`. |
| `migrate` | Apply `Migration` steps. |
| `seal_fields` / `open_fields` | ChaCha20-Poly1305 envelopes. |
| `diff` / `apply_patch` | Semantic diff and field patch. |
| `to_json` / `to_yaml` / `to_cbor` | Conversion. |
| `compile_to_json` | Parse + sorted JSON. |
| `utils::lson::encrypt` / `decrypt` | Whole-file `LSON/1` seal. |

Errors are `lson::LsonError`. Validation returns `Result<(), Vec<ValidationIssue>>`. Nothing in the library calls `process::exit`.

A short path that exercises parse, env, and JSON:

```rust
fn main() -> Result<(), lson::LsonError> {
    let src = r#"
        @env(PORT)
        name = "demo"
        port = PORT
    "#;
    let doc = lson::parse_str(src)?;
    let resolved = lson::resolve(&doc, |name| match name {
        "PORT" => Some("8080".into()),
        _ => None,
    })?;
    println!("{}", lson::to_json(&resolved)?);
    Ok(())
}
```

---

## Development

```sh
cargo build
cargo test
cargo clippy --all-targets -- -D warnings
cargo fmt -- --check

python build.py              # all targets
python build.py linux        # only linux
python build.py windows macos-arm64
```

### Release
```sh
git tag v1.0.0
git push origin v1.0.0
# GitHub Actions builds and publishes the release automatically
```

---

## License

MIT — see [LICENSE](LICENSE).
