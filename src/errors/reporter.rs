/// Structured error reporting for Clarity.
///
/// All errors are emitted as JSON to stderr. When `--human` is passed,
/// a human-readable rendering is also shown.
use serde::Serialize;

/// A structured error produced by any phase of the Clarity pipeline.
#[derive(Debug, Clone, Serialize)]
pub struct ClarityError {
    pub code: ErrorCode,
    pub severity: Severity,
    pub location: SourceLocation,
    pub message: String,
    pub context: String,
    pub suggestion: String,
}

impl std::fmt::Display for ClarityError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "[{}] {}:{}: {}",
            self.code, self.location.file, self.location.line, self.message
        )
    }
}

impl std::error::Error for ClarityError {}

/// Source location for error reporting.
#[derive(Debug, Clone, Serialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

impl SourceLocation {
    /// Create a default/unknown location.
    pub fn unknown() -> Self {
        Self {
            file: "<unknown>".to_string(),
            line: 0,
            column: 0,
            end_line: 0,
            end_column: 0,
        }
    }
}

/// Error severity level.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Error => write!(f, "error"),
            Self::Warning => write!(f, "warning"),
        }
    }
}

/// Error code identifying the kind of error.
#[derive(Debug, Clone, Serialize)]
pub enum ErrorCode {
    #[serde(rename = "E_SYNTAX")]
    Syntax,
    #[serde(rename = "E_UNEXPECTED_TOKEN")]
    UnexpectedToken,
    #[serde(rename = "E_TYPE_MISMATCH")]
    TypeMismatch,
    #[serde(rename = "E_UNDEFINED_VARIABLE")]
    UndefinedVariable,
    #[serde(rename = "E_UNDEFINED_FUNCTION")]
    UndefinedFunction,
    #[serde(rename = "E_IMMUTABLE_REASSIGN")]
    ImmutableReassign,
    #[serde(rename = "E_FUNCTION_TOO_LONG")]
    FunctionTooLong,
    #[serde(rename = "E_MISSING_INTENT")]
    MissingIntent,
    #[serde(rename = "E_MISSING_RETURN_TYPE")]
    MissingReturnType,
    #[serde(rename = "E_EXHAUSTIVENESS")]
    Exhaustiveness,
    #[serde(rename = "E_DUPLICATE_DEFINITION")]
    DuplicateDefinition,
    #[serde(rename = "E_IMPORT_NOT_FOUND")]
    ImportNotFound,
    #[serde(rename = "E_DIVISION_BY_ZERO")]
    DivisionByZero,
    #[serde(rename = "E_UNWRAP_NONE")]
    UnwrapNone,
    #[serde(rename = "E_CONTRACT_REQUIRES")]
    ContractRequires,
    #[serde(rename = "E_CONTRACT_ENSURES")]
    ContractEnsures,
    #[serde(rename = "E_INDEX_OUT_OF_BOUNDS")]
    IndexOutOfBounds,
    #[serde(rename = "E_INTENT_MISMATCH")]
    IntentMismatch,
}

impl std::fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let s = serde_json::to_value(self)
            .ok()
            .and_then(|v| v.as_str().map(String::from))
            .unwrap_or_else(|| format!("{self:?}"));
        write!(f, "{s}")
    }
}

impl ClarityError {
    /// Emit this error as JSON to stderr.
    pub fn emit_json(&self) {
        if let Ok(json) = serde_json::to_string(self) {
            eprintln!("{json}");
        }
    }
}

/// Convert a byte offset to a (line, column) pair (both 1-based).
pub fn offset_to_line_col(source: &str, offset: usize) -> (usize, usize) {
    let mut line = 1;
    let mut col = 1;
    for (i, ch) in source.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 1;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Build a `SourceLocation` from a span and source text.
pub fn location_from_span(
    file: &str,
    source: &str,
    start: usize,
    end: usize,
) -> SourceLocation {
    let (line, column) = offset_to_line_col(source, start);
    let (end_line, end_column) = offset_to_line_col(source, end);
    SourceLocation {
        file: file.to_string(),
        line,
        column,
        end_line,
        end_column,
    }
}
