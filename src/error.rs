//! Shared error type for the LSON toolkit.
//!
//! Library paths return [`LsonError`]. The CLI may map that to a non-zero exit;
//! tests never depend on process abort.

use std::io;

/// Recoverable failure from parse, validation, migration, crypto, or conversion.
#[derive(Debug, thiserror::Error)]
pub enum LsonError {
    #[error("parse error at line {line}: {message}")]
    Parse { line: usize, message: String },

    #[error("validation error: {0}")]
    Validation(String),

    #[error("{0}")]
    Message(String),

    #[error("decryption failed — wrong key or corrupted envelope")]
    DecryptionFailed,

    #[error("I/O error: {0}")]
    Io(#[from] io::Error),
}

impl LsonError {
    pub fn parse(line: usize, message: impl Into<String>) -> Self {
        LsonError::Parse {
            line,
            message: message.into(),
        }
    }

    pub fn msg(message: impl Into<String>) -> Self {
        LsonError::Message(message.into())
    }
}
