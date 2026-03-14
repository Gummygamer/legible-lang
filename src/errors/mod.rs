/// Error types and reporting for the Legible language.
pub mod reporter;

pub use reporter::{LegibleError, ErrorCode, Severity, SourceLocation};
