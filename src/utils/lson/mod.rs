#![allow(dead_code)]
//! LSON — Locked/Secured Object Notation
//!
//! Encrypts KSON files using ChaCha20-Poly1305 authenticated encryption with
//! Argon2id key derivation. The file format carries a plaintext SHA-256 fingerprint
//! of the original KSON so drift can be detected without decryption.
//!
//! File format:
//! ```text
//! LSON/1
//! SALT:<32-hex-bytes>     (16 random bytes for Argon2id)
//! NONCE:<24-hex-bytes>    (12 random bytes for ChaCha20-Poly1305)
//! KSON-HASH:<64-hex>      (SHA-256 of the plaintext KSON — for drift detection)
//!
//! <base64-encoded-ciphertext-with-auth-tag>
//! ```

use argon2::{Algorithm, Argon2, Params, Version};
use base64::{engine::general_purpose, Engine as _};
use chacha20poly1305::{
    aead::{Aead, KeyInit},
    ChaCha20Poly1305, Key, Nonce,
};
use rand::{rngs::OsRng, RngCore};
use sha2::{Digest, Sha256};
use std::{fmt, fs, io};

const MAGIC: &str = "LSON/1";

// Argon2id tuning: 64 MB memory, 3 iterations, 4 lanes → ~1 s on modern hardware
const ARGON2_M_COST: u32 = 65536;
const ARGON2_T_COST: u32 = 3;
const ARGON2_P_COST: u32 = 4;

// ── Error type ────────────────────────────────────────────────────────────────

#[derive(Debug)]
pub enum LsonError {
    InvalidFormat(String),
    /// Decryption failed — wrong passphrase or file is corrupted.
    DecryptionFailed,
    Io(io::Error),
    KeyDerivation(String),
}

impl fmt::Display for LsonError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LsonError::InvalidFormat(m) => write!(f, "invalid LSON format: {m}"),
            LsonError::DecryptionFailed => {
                write!(f, "decryption failed — wrong passphrase or corrupted file")
            }
            LsonError::Io(e) => write!(f, "I/O error: {e}"),
            LsonError::KeyDerivation(m) => write!(f, "key derivation failed: {m}"),
        }
    }
}

impl From<io::Error> for LsonError {
    fn from(e: io::Error) -> Self {
        LsonError::Io(e)
    }
}

// ── Public API ────────────────────────────────────────────────────────────────

/// SHA-256 of `data`, returned as lowercase hex.
pub fn sha256_hex(data: &[u8]) -> String {
    let mut h = Sha256::new();
    h.update(data);
    hex::encode(h.finalize())
}

/// Resolve the passphrase used for encryption/decryption.
///
/// Priority:
/// 1. `explicit` — value passed directly (e.g. from `--key` flag)
/// 2. `LSON_KEY` environment variable
/// 3. Interactive terminal prompt (hidden input)
pub fn resolve_key(explicit: Option<&str>) -> Result<String, LsonError> {
    if let Some(k) = explicit {
        if !k.is_empty() {
            return Ok(k.to_string());
        }
    }
    if let Ok(k) = std::env::var("LSON_KEY") {
        if !k.is_empty() {
            return Ok(k);
        }
    }
    rpassword::prompt_password("🔑 LSON passphrase: ").map_err(|e| {
        LsonError::Io(io::Error::other(format!(
            "cannot read passphrase: {e} — set LSON_KEY or use --key"
        )))
    })
}

/// Encrypt a KSON string and return the full LSON file content.
pub fn encrypt(plaintext: &str, passphrase: &str) -> Result<String, LsonError> {
    let mut salt = [0u8; 16];
    let mut nonce_bytes = [0u8; 12];
    OsRng.fill_bytes(&mut salt);
    OsRng.fill_bytes(&mut nonce_bytes);

    let key_bytes = derive_key(passphrase, &salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&nonce_bytes);

    // encrypt() appends the 16-byte Poly1305 authentication tag automatically.
    let ciphertext = cipher
        .encrypt(nonce, plaintext.as_bytes())
        .map_err(|_| LsonError::DecryptionFailed)?;

    let kson_hash = sha256_hex(plaintext.as_bytes());

    Ok(format!(
        "{magic}\nSALT:{salt}\nNONCE:{nonce}\nKSON-HASH:{hash}\n\n{data}\n",
        magic = MAGIC,
        salt = hex::encode(salt),
        nonce = hex::encode(nonce_bytes),
        hash = kson_hash,
        data = general_purpose::STANDARD.encode(&ciphertext),
    ))
}

/// Decrypt an LSON file and return the original KSON plaintext.
pub fn decrypt(lson_content: &str, passphrase: &str) -> Result<String, LsonError> {
    let parsed = parse_lson(lson_content)?;
    let key_bytes = derive_key(passphrase, &parsed.salt)?;
    let cipher = ChaCha20Poly1305::new(Key::from_slice(&key_bytes));
    let nonce = Nonce::from_slice(&parsed.nonce);

    let plaintext = cipher
        .decrypt(nonce, parsed.ciphertext.as_ref())
        .map_err(|_| LsonError::DecryptionFailed)?;

    String::from_utf8(plaintext)
        .map_err(|_| LsonError::InvalidFormat("decrypted content is not valid UTF-8".into()))
}

/// Read the sealed KSON-HASH from an LSON file **without decrypting**.
/// Use this to detect source drift cheaply.
pub fn kson_hash_from_lson(lson_content: &str) -> Result<String, LsonError> {
    Ok(parse_lson(lson_content)?.kson_hash)
}

pub fn encrypt_file(path: &str, passphrase: &str) -> Result<String, LsonError> {
    let text = fs::read_to_string(path)?;
    encrypt(&text, passphrase)
}

pub fn decrypt_file(path: &str, passphrase: &str) -> Result<String, LsonError> {
    let text = fs::read_to_string(path)?;
    decrypt(&text, passphrase)
}

// ── Internals ─────────────────────────────────────────────────────────────────

struct LsonParsed {
    salt: [u8; 16],
    nonce: [u8; 12],
    kson_hash: String,
    ciphertext: Vec<u8>,
}

fn derive_key(passphrase: &str, salt: &[u8; 16]) -> Result<[u8; 32], LsonError> {
    let params = Params::new(ARGON2_M_COST, ARGON2_T_COST, ARGON2_P_COST, Some(32))
        .map_err(|e| LsonError::KeyDerivation(e.to_string()))?;
    let argon2 = Argon2::new(Algorithm::Argon2id, Version::V0x13, params);
    let mut key = [0u8; 32];
    argon2
        .hash_password_into(passphrase.as_bytes(), salt, &mut key)
        .map_err(|e| LsonError::KeyDerivation(e.to_string()))?;
    Ok(key)
}

fn parse_lson(content: &str) -> Result<LsonParsed, LsonError> {
    let mut lines = content.lines();

    match lines.next() {
        Some(m) if m == MAGIC => {}
        Some(m) => {
            return Err(LsonError::InvalidFormat(format!(
                "expected magic '{}', got '{}'",
                MAGIC, m
            )))
        }
        None => return Err(LsonError::InvalidFormat("empty file".into())),
    }

    let mut salt_hex: Option<String> = None;
    let mut nonce_hex: Option<String> = None;
    let mut kson_hash: Option<String> = None;
    let mut data_lines: Vec<&str> = Vec::new();
    let mut in_data = false;

    for line in lines {
        if in_data {
            if !line.is_empty() {
                data_lines.push(line);
            }
            continue;
        }
        if line.is_empty() {
            in_data = true;
            continue;
        }
        if let Some(v) = line.strip_prefix("SALT:") {
            salt_hex = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("NONCE:") {
            nonce_hex = Some(v.to_string());
        } else if let Some(v) = line.strip_prefix("KSON-HASH:") {
            kson_hash = Some(v.to_string());
        }
    }

    let require = |opt: Option<String>, header: &str| {
        opt.ok_or_else(|| LsonError::InvalidFormat(format!("missing {header} header")))
    };

    let salt_bytes = hex::decode(require(salt_hex, "SALT")?)
        .map_err(|e| LsonError::InvalidFormat(format!("bad SALT hex: {e}")))?;
    let nonce_bytes = hex::decode(require(nonce_hex, "NONCE")?)
        .map_err(|e| LsonError::InvalidFormat(format!("bad NONCE hex: {e}")))?;
    let kson_hash = require(kson_hash, "KSON-HASH")?;

    if salt_bytes.len() != 16 {
        return Err(LsonError::InvalidFormat("SALT must be 16 bytes".into()));
    }
    if nonce_bytes.len() != 12 {
        return Err(LsonError::InvalidFormat("NONCE must be 12 bytes".into()));
    }

    let ciphertext = general_purpose::STANDARD
        .decode(data_lines.join(""))
        .map_err(|e| LsonError::InvalidFormat(format!("bad base64 data: {e}")))?;

    let mut salt = [0u8; 16];
    let mut nonce = [0u8; 12];
    salt.copy_from_slice(&salt_bytes);
    nonce.copy_from_slice(&nonce_bytes);

    Ok(LsonParsed {
        salt,
        nonce,
        kson_hash,
        ciphertext,
    })
}
