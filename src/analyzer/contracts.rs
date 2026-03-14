/// Contract instrumentation for the Clarity language.
///
/// Contracts (requires/ensures) are evaluated directly by the evaluator.
/// This module provides utilities for checking function body length
/// and other static contract-related checks.
use crate::errors::{ClarityError, ErrorCode, Severity, SourceLocation};
use crate::parser::arena::Arena;
use crate::parser::ast::*;

/// Maximum allowed lines in a function body.
const MAX_FUNCTION_BODY_LINES: usize = 40;

/// Check static contract rules across the program.
/// Returns a list of errors for violations.
pub fn check_contracts(arena: &Arena, root: NodeId) -> Vec<ClarityError> {
    let mut errors = Vec::new();
    if let NodeKind::Program { ref statements } = arena.get(root).kind {
        for &stmt_id in statements {
            if let NodeKind::FunctionDecl {
                ref name,
                ref body,
                ..
            } = arena.get(stmt_id).kind
            {
                if body.len() > MAX_FUNCTION_BODY_LINES {
                    errors.push(ClarityError {
                        code: ErrorCode::FunctionTooLong,
                        severity: Severity::Error,
                        location: SourceLocation::unknown(),
                        message: format!(
                            "Function '{}' has {} statements, exceeding the maximum of {}",
                            name,
                            body.len(),
                            MAX_FUNCTION_BODY_LINES
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
