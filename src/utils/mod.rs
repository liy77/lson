pub mod debug;
pub mod json;
pub mod kson;
pub mod lson;

/// Bridge the historical string-tree parser to the typed document.
///
/// Existing callers (`read`, `read_file`, `kson_items_to_json`) keep working.
/// New code should use [`crate::kson`] and [`crate::schema`] directly.
pub fn read_document(text: &str) -> Result<crate::value::Document, crate::error::LsonError> {
    crate::kson::parse_str(text)
}
