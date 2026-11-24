//! Transparent ChaCha20-Poly1305 encryption of individual fields.
//!
//! The document stores an envelope (algorithm, key id, nonce, ciphertext).
//! The key bytes live in a [`KeyStore`] the caller supplies — they are never
//! written into the document. Whole-file `LSON/1` seals stay in
//! `utils::lson` and are a different envelope.

use crate::error::LsonError;
use crate::value::Value;
use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chacha20poly1305::aead::{Aead, KeyInit};
use chacha20poly1305::{ChaCha20Poly1305, Key, Nonce};
use rand::{rngs::OsRng, RngCore};
use std::collections::HashMap;

pub const ALG: &str = "chacha20poly1305";

/// Key material kept outside the document. The map key is the key id.
#[derive(Debug, Clone, Default)]
pub struct KeyStore {
    keys: HashMap<String, [u8; 32]>,
}

impl KeyStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: impl Into<String>, key: [u8; 32]) {
        self.keys.insert(id.into(), key);
    }

    pub fn insert_passphrase_bytes(&mut self, id: impl Into<String>, raw: &[u8]) {
        // Direct 32-byte keys are stored as-is. Shorter material is expanded
        // with SHA-256 so tests and the CLI can pass a passphrase-sized secret
        // without going through Argon2 (that KDF stays on whole-file seals).
        let key = if raw.len() == 32 {
            let mut buf = [0u8; 32];
            buf.copy_from_slice(raw);
            buf
        } else {
            use sha2::{Digest, Sha256};
            let digest = Sha256::digest(raw);
            let mut buf = [0u8; 32];
            buf.copy_from_slice(&digest);
            buf
        };
        self.insert(id, key);
    }

    pub fn get(&self, id: &str) -> Option<&[u8; 32]> {
        self.keys.get(id)
    }
}

/// Encrypt `plaintext` under `key_id`. The returned value is an object envelope.
pub fn encrypt_field(
    plaintext: &Value,
    key_id: &str,
    store: &KeyStore,
) -> Result<Value, LsonError> {
    let key = store
        .get(key_id)
        .ok_or_else(|| LsonError::msg(format!("unknown key id '{key_id}'")))?;
    let encoded = crate::convert::to_cbor_value(plaintext)?;
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut nonce_bytes);
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let ciphertext = cipher
        .encrypt(nonce, encoded.as_ref())
        .map_err(|_| LsonError::DecryptionFailed)?;
    Ok(envelope(key_id, &nonce_bytes, &ciphertext))
}

/// Decrypt an envelope. A wrong key or a tampered ciphertext fails closed.
pub fn decrypt_field(envelope: &Value, store: &KeyStore) -> Result<Value, LsonError> {
    let obj = envelope
        .as_object()
        .ok_or_else(|| LsonError::msg("secret field is not an envelope"))?;
    let get = |k: &str| {
        obj.iter()
            .find(|(name, _)| name == k)
            .map(|(_, v)| v)
            .ok_or_else(|| LsonError::msg(format!("envelope missing '{k}'")))
    };
    let alg = match get("alg")? {
        Value::Str(s) if s == ALG => s,
        _ => return Err(LsonError::msg("unsupported secret algorithm")),
    };
    let _ = alg;
    let key_id = match get("kid")? {
        Value::Str(s) => s.as_str(),
        _ => return Err(LsonError::msg("envelope kid must be a string")),
    };
    let nonce_b64 = match get("nonce")? {
        Value::Str(s) => s.as_str(),
        _ => return Err(LsonError::msg("envelope nonce must be a string")),
    };
    let ct_b64 = match get("ct")? {
        Value::Str(s) => s.as_str(),
        _ => return Err(LsonError::msg("envelope ct must be a string")),
    };
    let key = store
        .get(key_id)
        .ok_or_else(|| LsonError::msg(format!("unknown key id '{key_id}'")))?;
    let nonce_bytes = B64
        .decode(nonce_b64)
        .map_err(|e| LsonError::msg(format!("bad nonce: {e}")))?;
    if nonce_bytes.len() != 12 {
        return Err(LsonError::msg("nonce must be 12 bytes"));
    }
    let ciphertext = B64
        .decode(ct_b64)
        .map_err(|e| LsonError::msg(format!("bad ciphertext: {e}")))?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(key));
    let nonce = Nonce::from_slice(&nonce_bytes);
    let plain = cipher
        .decrypt(nonce, ciphertext.as_ref())
        .map_err(|_| LsonError::DecryptionFailed)?;
    crate::convert::from_cbor_slice(&plain)
}

fn envelope(key_id: &str, nonce: &[u8], ciphertext: &[u8]) -> Value {
    Value::Object(vec![
        ("alg".into(), Value::Str(ALG.into())),
        ("kid".into(), Value::Str(key_id.into())),
        ("nonce".into(), Value::Str(B64.encode(nonce))),
        ("ct".into(), Value::Str(B64.encode(ciphertext))),
    ])
}

/// Encrypt every field listed in `paths`. Other fields are copied unchanged.
pub fn seal_fields(
    doc: &Value,
    paths: &[&str],
    key_id: &str,
    store: &KeyStore,
) -> Result<Value, LsonError> {
    let mut out = doc.clone();
    for path in paths {
        let current = out
            .pointer(path)
            .cloned()
            .ok_or_else(|| LsonError::msg(format!("missing secret field '{path}'")))?;
        let sealed = encrypt_field(&current, key_id, store)?;
        out.insert_path(path, sealed)?;
    }
    Ok(out)
}

/// Decrypt every envelope found under `paths`.
pub fn open_fields(doc: &Value, paths: &[&str], store: &KeyStore) -> Result<Value, LsonError> {
    let mut out = doc.clone();
    for path in paths {
        let current = out
            .pointer(path)
            .cloned()
            .ok_or_else(|| LsonError::msg(format!("missing secret field '{path}'")))?;
        let opened = decrypt_field(&current, store)?;
        out.insert_path(path, opened)?;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::kson::parse_str;

    fn store(id: &str, secret: &str) -> KeyStore {
        let mut ks = KeyStore::new();
        ks.insert_passphrase_bytes(id, secret.as_bytes());
        ks
    }

    #[test]
    fn secret_field_round_trip_and_wrong_key_fails_closed() {
        let doc = parse_str("user = \"ada\"\ntoken = \"s3cret\"\n")
            .unwrap()
            .root;
        let ks = store("k1", "correct horse");
        let sealed = seal_fields(&doc, &["token"], "k1", &ks).unwrap();
        // The document does not contain the key or the plaintext.
        let token = sealed.get("token").unwrap();
        let rendered = format!("{token}");
        assert!(!rendered.contains("s3cret"));
        assert!(!rendered.contains("correct horse"));
        assert_eq!(token.get("kid"), Some(&Value::Str("k1".into())));
        assert_eq!(token.get("alg"), Some(&Value::Str(ALG.into())));

        let opened = open_fields(&sealed, &["token"], &ks).unwrap();
        assert_eq!(opened.get("token"), Some(&Value::Str("s3cret".into())));
        assert_eq!(opened.get("user"), Some(&Value::Str("ada".into())));

        let wrong = store("k1", "nope");
        let err = open_fields(&sealed, &["token"], &wrong).unwrap_err();
        assert!(matches!(err, LsonError::DecryptionFailed));
    }
}
