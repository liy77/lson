# Examples

Walkthrough of `app.kson` + `app.kmodel`, plus the typed forms in the root README.

`lson compile -t json` prints the parsed tree. A bare name declared with `@env` stays a reference (`{"$type":"ref","$value":"DB_URL"}`) until something calls `lson::resolve`. The CLI does not substitute env vars by itself.

## Setup

```sh
cp .env.example .env
# edit .env with your values
```

## 1 — Compile KSON → JSON (with KModel validation)

```sh
lson compile -f app.kson -t json -o app.json --kmodel app.kmodel
```

Output: canonical JSON. `database.url` and `auth.secret` are still references. Validation checks the types in `app.kmodel` and exits non-zero on a mismatch.

To substitute, resolve in the library and then convert:

```rust
let src = std::fs::read_to_string("app.kson")?;
let doc = lson::parse_str(&src)?;
let resolved = lson::resolve(&doc, |name| std::env::var(name).ok())?;
let json = lson::to_json(&resolved)?;
```

## 2 — Compile KSON → LSON (encrypted)

```sh
lson compile -f app.kson -t lson -o app.lson
# prompts for passphrase, or reads LSON_KEY env var
```

The resulting `.lson` file is ChaCha20-Poly1305 encrypted.  
Commit it safely — the key is never stored in the file.

## 3 — Decrypt LSON

```sh
lson parse app.lson
```

## 4 — Detect config drift (no key needed)

```sh
lson verify -f app.kson --lson app.lson
# ✓ Source KSON matches the sealed hash — no drift detected.
```

Useful in CI to confirm no one edited the source without re-encrypting.

## 5 — Pin config as a lockfile

```sh
lson lock -f app.kson -o app.lock
```

Commits the resolved, canonical JSON + source hash.  
Diff-friendly for pull request reviews.

## 6 — Typed snippet on stdin

```sh
lson raw compile --text 'born = d"2020-01-02"
note = """
hello
world
"""
status = #Ok(1)
' -t json
```

## Files

| File | Description |
|------|-------------|
| `app.kson` | Source configuration with `@env` declarations and `$section`s |
| `app.kmodel` | Type schema — checked by `--kmodel` |
| `app.json` | Generated JSON (commit or gitignore) |
| `app.lson` | Sealed source — safe to commit; the key is not in the file |
| `app.lock` | Pinned canonical JSON plus the KSON hash |
| `verify-sample.kson` | Small file used to check that two compiles match |
| `.env.example` | Names to export before calling `lson::resolve` |
