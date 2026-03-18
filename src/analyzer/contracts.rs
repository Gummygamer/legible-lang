/// Contract instrumentation for the Legible language.
///
/// Contracts (requires/ensures) are evaluated directly by the evaluator.
/// This module provides utilities for checking function body length
/// and other static contract-related checks.
use crate::errors::{LegibleError, ErrorCode, Severity, SourceLocation};
use crate::errors::reporter::offset_to_line_col;
use crate::parser::arena::Arena;
use crate::parser::ast::*;

/// Maximum allowed source lines in a function body.
const MAX_FUNCTION_BODY_LINES: usize = 40;

/// Check static contract rules across the program.
/// Returns a list of errors for violations.
pub fn check_contracts(arena: &Arena, root: NodeId, source: &str) -> Vec<LegibleError> {
    let mut errors = Vec::new();
    if let NodeKind::Program { ref statements } = arena.get(root).kind {
        for &stmt_id in statements {
            if let NodeKind::FunctionDecl {
                ref name,
                ref body,
                ..
            } = arena.get(stmt_id).kind
            {
                if body.is_empty() {
                    continue;
                }
                let first_span = arena.get(*body.first().unwrap()).span;
                let last_span = arena.get(*body.last().unwrap()).span;
                let (start_line, _) = offset_to_line_col(source, first_span.start);
                let (end_line, _) = offset_to_line_col(source, last_span.end);
                let line_count = end_line.saturating_sub(start_line) + 1;

                if line_count > MAX_FUNCTION_BODY_LINES {
                    let fn_span = arena.get(stmt_id).span;
                    let (fn_line, fn_col) = offset_to_line_col(source, fn_span.start);
                    errors.push(LegibleError {
                        code: ErrorCode::FunctionTooLong,
                        severity: Severity::Error,
                        location: SourceLocation {
                            file: "<unknown>".to_string(),
                            line: fn_line,
                            column: fn_col,
                            end_line: fn_line,
                            end_column: fn_col,
                        },
                        message: format!(
                            "Function '{}' body spans {} lines, exceeding the maximum of {}",
                            name, line_count, MAX_FUNCTION_BODY_LINES
                        ),
                        context: String::new(),
                        suggestion: "Break the function into smaller functions".to_string(),
                    });
                }
            }
        }
    }
    errors
}
