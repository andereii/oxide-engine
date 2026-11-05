use thiserror::Error;

/// Errors returned by the oxide-data parsers.
#[derive(Debug, Error)]
pub enum DataError {
    #[error("JSON parse error: {0}")]
    Parse(String),
    #[error("Schema validation error for {kind}: {message}")]
    Schema { kind: &'static str, message: String },
    #[error("Input is not valid UTF-8")]
    NotUtf8,
}
