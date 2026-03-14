/// Error types and reporting for the Clarity language.
pub mod reporter;

pub use reporter::{ClarityError, ErrorCode, Severity, SourceLocation};
