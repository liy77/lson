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

**LSON** is a type-safe configuration toolkit built around three components:

| Component | Description |
|-----------|-------------|
| **KSON** | Human-friendly config format with env-var injection and nested sections |
| **KModel** | Schema file that enforces types on KSON properties |
| **LSON** | ChaCha20-Poly1305 encrypted form of a KSON file |

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

---

## KSON format

KSON is a readable key-value config format that supports environment variable injection and indentation-based nested sections.

```kson
# config.kson

# Declare the env vars this file uses
@env(DB_URL)
@env(API_TOKEN)

app_name   = "my-app"
debug      = false
max_conn   = 100
api_token  = API_TOKEN   # replaced with $API_TOKEN at parse time

$database
    url  = DB_URL
    pool = 10

    $replica
        url      = "postgres://replica:5432/db"
        readonly = true
```

---

## KModel — type schema

A `.kmodel` file enforces types on KSON properties. Reference it at compile time or inline with `@model(path)`.

```kmodel
# config.kmodel

app_name:  String
debug:     Bool
max_conn:  Integer
api_token: String?   # ? = optional

$database
    url:  String
    pool: Integer

    $replica
        url:      String
        readonly: Bool
```

**Available types:** `String` · `Integer` · `Float` · `Bool` · `Char` · `Any` · `Array<T>` · `T?` (optional)

---

## LSON — encrypted configuration

LSON seals a KSON file with authenticated encryption (ChaCha20-Poly1305 + Argon2id key derivation). The file embeds a SHA-256 fingerprint of the original KSON so drift can be detected without decrypting.

```
LSON/1
SALT:<hex>       ← random 16 bytes for Argon2id
NONCE:<hex>      ← random 12 bytes for ChaCha20-Poly1305
KSON-HASH:<hex>  ← SHA-256 of the plaintext (for drift detection)

<base64 ciphertext + Poly1305 auth tag>
```

> The passphrase is read from the `LSON_KEY` env var, the `--key` flag, or an interactive prompt.

---

## Usage

### Compile KSON → LSON (encrypted)
```sh
lson compile -f config.kson
# or with explicit key
LSON_KEY=secret lson compile -f config.kson -o config.lson
```

### Compile KSON → JSON
```sh
lson compile -f config.kson -t json -o config.json
```

### Compile with KModel validation
```sh
lson compile -f config.kson --kmodel config.kmodel -t json
```

### Decrypt an LSON file
```sh
lson parse config.lson
```

### Detect configuration drift
Check whether the source KSON has changed since the LSON was compiled — **no passphrase needed**:
```sh
lson verify -f config.kson --lson config.lson
# ✓ Source KSON matches the sealed hash — no drift detected.
# sha256: e3b0c44298fc1c149afbf4c8996fb92427ae41e4...
```

### Pin a resolved config (lockfile)
Writes canonical JSON + SHA-256 hash for reproducibility and diff-friendly version control:
```sh
lson lock -f config.kson -o config.lock
```

### Raw mode (programmatic / piping)
```sh
lson raw compile -f config.kson -t json
```

---

## Development

```sh
# Build
cargo build

# Run tests
cargo test

# Lint
cargo clippy
cargo fmt

# Cross-platform release build (Python)
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
