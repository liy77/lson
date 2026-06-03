# Examples

Complete walkthrough using `app.kson` + `app.kmodel`.

## Setup

```sh
cp .env.example .env
# edit .env with your values
```

## 1 — Compile KSON → JSON (with KModel validation)

```sh
lson compile -f app.kson -t json -o app.json --kmodel app.kmodel
```

Output: canonical JSON with env vars resolved and types validated.

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

## Files

| File | Description |
|------|-------------|
| `app.kson` | Source configuration with env var injection |
| `app.kmodel` | Type schema — validated at compile time |
| `app.json` | Generated canonical JSON (gitignore or commit) |
| `app.lson` | Encrypted configuration — safe to commit |
| `app.lock` | Pinned canonical JSON for reproducibility |
| `.env.example` | Environment variable template |
