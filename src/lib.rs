//! LSON toolkit.
//!
//! KSON is the typed document format. LSON is the toolkit around it:
//! schemas and KModel generation, validation, migration, environment
//! resolution, ChaCha20-Poly1305 secret fields, conversion, diff, patch,
//! and a streaming parser. Whole-file `LSON/1` seals live in [`utils::lson`].

#![allow(clippy::too_many_arguments)]
#![allow(clippy::match_like_matches_macro)]
#![allow(clippy::to_string_trait_impl)]
#![allow(clippy::expect_fun_call)]
#![allow(clippy::manual_strip)]
#![allow(clippy::collapsible_if)]
#![allow(clippy::unnecessary_unwrap)]
#![allow(clippy::upper_case_acronyms)]
#![allow(clippy::needless_borrow)]
#![allow(clippy::single_match)]
#![allow(clippy::if_same_then_else)]
#![allow(clippy::search_is_some)]
#![allow(clippy::map_clone)]
#![allow(clippy::redundant_pattern_matching)]
#![allow(clippy::nonminimal_bool)]

pub mod convert;
pub mod diff;
pub mod envresolve;
pub mod error;
pub mod kson;
pub mod migrate;
pub mod patch;
pub mod schema;
pub mod secret;
pub mod stream;
pub mod utils;
pub mod value;

pub use convert::{to_cbor, to_json, to_yaml};
pub use diff::{diff, Change};
pub use envresolve::{resolve, resolve_with_map};
pub use error::LsonError;
pub use kson::{canonical, canonical_bytes, parse_str, parse_with_imports};
pub use migrate::{migrate, Migration, Step};
pub use patch::{apply as apply_patch, Op, Patch};
pub use schema::{generate, infer, infer_model, model_from_source, validate, KModel, Schema};
pub use secret::{open_fields, seal_fields, KeyStore};
pub use stream::parse_reader;
pub use value::{Document, Value};

/// Compile KSON source to canonical JSON text. This is the deterministic
/// command the CLI `raw compile -t json` path uses.
pub fn compile_to_json(src: &str) -> Result<String, LsonError> {
    let doc = parse_str(src)?;
    let sorted = value::sort_value(&doc.root);
    to_json(&sorted)
}
