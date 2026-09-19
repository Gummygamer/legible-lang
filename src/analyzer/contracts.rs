/// Contract instrumentation for the Legible language.
///
/// Contracts (requires/ensures) are evaluated directly by the evaluator.
/// This module provides utilities for statically checking function-level
/// rules: the comprehension budget of [`crate::analyzer::complexity`], and
/// other static contract-related checks.
use crate::analyzer::complexity::{self, FunctionComplexity};
use crate::errors::reporter::offset_to_line_col;
use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::parser::arena::Arena;
use crate::parser::ast::{NodeId, NodeKind};

/// Check static contract rules across the program.
/// Returns a list of errors for violations.
#[must_use]
pub fn check_contracts(arena: &Arena, root: NodeId, source: &str) -> Vec<LegibleError> {
    let mut errors = Vec::new();
    if let NodeKind::Program { ref statements } = arena.get(root).kind {
        for &stmt_id in statements {
            if let NodeKind::FunctionDecl {
                ref name,
                ref params,
                ref body,
                ..
            } = arena.get(stmt_id).kind
            {
                if body.is_empty() {
                    continue;
                }
                let complexity = complexity::measure_function(arena, params, body);
                if !complexity.within_budget() {
                    errors.push(over_budget_error(arena, stmt_id, source, name, &complexity));
                }
            }
        }
    }
    errors
}

/// Build the `E_FUNCTION_TOO_LONG` diagnostic for a function that has spent
/// more than its comprehension budget.
///
/// The message names the dimension that was blown, the raw measurement, the
/// published threshold it is measured against and the source of that threshold,
/// so an agent can correct the function without re-deriving the rule.
fn over_budget_error(
    arena: &Arena,
    function_id: NodeId,
    source: &str,
    name: &str,
    complexity: &FunctionComplexity,
) -> LegibleError {
    let (dimension, load) = complexity.dominant();
    let span = arena.get(function_id).span;
    let (line, column) = offset_to_line_col(source, span.start);
    LegibleError {
        code: ErrorCode::FunctionTooLong,
        severity: Severity::Error,
        location: SourceLocation {
            file: "<unknown>".to_string(),
            line,
            column,
            end_line: line,
            end_column: column,
        },
        message: format!(
            "Function '{name}' is over its comprehension budget: {} is {} ({:.0}% of the limit from {}). \
             Measured: volume {:.0}/{:.0} bits, cyclomatic {}/{}, cognitive {}/{}, live bindings {}/{}.",
            dimension.label(),
            complexity.measurement(dimension),
            load * 100.0,
            dimension.citation(),
            complexity.halstead_volume,
            complexity::MAX_HALSTEAD_VOLUME,
            complexity.cyclomatic,
            complexity::MAX_CYCLOMATIC_COMPLEXITY,
            complexity.cognitive,
            complexity::MAX_COGNITIVE_COMPLEXITY,
            complexity.peak_live_bindings,
            complexity::MAX_LIVE_BINDINGS,
        ),
        context: source_line(source, line),
        suggestion: dimension.remedy().to_string(),
    }
}

/// The text of a 1-based source line, trimmed, for the error `context` field.
fn source_line(source: &str, line: usize) -> String {
    source
        .lines()
        .nth(line.saturating_sub(1))
        .unwrap_or_default()
        .trim()
        .to_string()
}
