/// Intent verification for the Legible language.
///
/// Uses heuristic keyword matching to verify that a function's intent
/// description is consistent with its body.
use crate::errors::{LegibleError, ErrorCode, Severity, SourceLocation};
use crate::parser::arena::Arena;
use crate::parser::ast::*;

/// Stop words filtered from intent text.
const STOP_WORDS: &[&str] = &[
    "a", "the", "an", "is", "to", "of", "that", "for", "and", "or", "it", "in", "by", "with",
    "from", "on", "at", "this", "be", "as", "its", "has", "have", "are", "was", "were", "will",
    "can", "should", "given", "into",
];

/// Generic intent keywords that always match (they're too vague to verify).
const GENERIC_KEYWORDS: &[&str] = &[
    "return", "produce", "compute", "calculate", "create", "make", "build", "generate",
    "process", "handle", "perform", "execute", "run", "do", "get", "set", "check",
];

/// Verify the intent of all functions in the AST. Returns warnings for mismatches.
pub fn verify_intents(arena: &Arena, root: NodeId) -> Vec<LegibleError> {
    let mut warnings = Vec::new();
    if let NodeKind::Program { ref statements } = arena.get(root).kind {
        for &stmt_id in statements {
            if let NodeKind::FunctionDecl {
                ref name,
                ref intent,
                ref body,
                ..
            } = arena.get(stmt_id).kind
            {
                if let Some(warning) = check_intent(arena, name, intent, body) {
                    warnings.push(warning);
                }
            }
        }
    }
    warnings
}

fn check_intent(
    arena: &Arena,
    function_name: &str,
    intent: &str,
    body: &[NodeId],
) -> Option<LegibleError> {
    let intent_keywords = tokenize_intent(intent);
    if intent_keywords.is_empty() {
        return None;
    }

    let body_signals = extract_body_signals(arena, body);

    let non_generic: Vec<&str> = intent_keywords
        .iter()
        .filter(|k| !GENERIC_KEYWORDS.contains(&k.as_str()))
        .map(String::as_str)
        .collect();

    if non_generic.is_empty() {
        return None;
    }

    let matched = non_generic
        .iter()
        .filter(|keyword| keyword_matches(keyword, &body_signals))
        .count();

    let match_ratio = matched as f64 / non_generic.len() as f64;
    if match_ratio < 0.3 {
        Some(LegibleError {
            code: ErrorCode::IntentMismatch,
            severity: Severity::Warning,
            location: SourceLocation::unknown(),
            message: format!(
                "Intent for function '{function_name}' may not match its body ({}% keyword match)",
                (match_ratio * 100.0) as u32
            ),
            context: format!("intent: {intent}"),
            suggestion: "Review the intent description to ensure it accurately describes what the function does".to_string(),
        })
    } else {
        None
    }
}

fn tokenize_intent(intent: &str) -> Vec<String> {
    intent
        .to_lowercase()
        .split_whitespace()
        .filter(|w| !STOP_WORDS.contains(w))
        .map(String::from)
        .collect()
}

/// Signals extracted from a function body for intent matching.
struct BodySignals {
    function_calls: Vec<String>,
    operators: Vec<String>,
    field_names: Vec<String>,
    identifiers: Vec<String>,
    string_literals: Vec<String>,
}

fn extract_body_signals(arena: &Arena, body: &[NodeId]) -> BodySignals {
    let mut signals = BodySignals {
        function_calls: Vec::new(),
        operators: Vec::new(),
        field_names: Vec::new(),
        identifiers: Vec::new(),
        string_literals: Vec::new(),
    };
    for &node_id in body {
        collect_signals(arena, node_id, &mut signals);
    }
    signals
}

fn collect_signals(arena: &Arena, node_id: NodeId, signals: &mut BodySignals) {
    match &arena.get(node_id).kind {
        NodeKind::FunctionCall { callee, arguments } => {
            if let NodeKind::Identifier(name) = &arena.get(*callee).kind {
                signals.function_calls.push(name.to_lowercase());
            }
            for &arg in arguments {
                collect_signals(arena, arg, signals);
            }
        }
        NodeKind::BinaryOp { left, op, right } => {
            let op_str = match op {
                BinaryOperator::Add => "add",
                BinaryOperator::Sub => "subtract",
                BinaryOperator::Mul => "multiply",
                BinaryOperator::Div => "divide",
                BinaryOperator::Mod => "modulo",
                BinaryOperator::Concat => "concat",
                BinaryOperator::Eq | BinaryOperator::NotEq => "compare",
                BinaryOperator::Gt | BinaryOperator::Lt | BinaryOperator::GtEq | BinaryOperator::LtEq => "compare",
                BinaryOperator::And | BinaryOperator::Or => "logic",
            };
            signals.operators.push(op_str.to_string());
            collect_signals(arena, *left, signals);
            collect_signals(arena, *right, signals);
        }
        NodeKind::FieldAccess { object, field } => {
            signals.field_names.push(field.to_lowercase());
            collect_signals(arena, *object, signals);
        }
        NodeKind::Identifier(name) => {
            signals.identifiers.push(name.to_lowercase());
        }
        NodeKind::TextLit(s) => {
            signals.string_literals.push(s.to_lowercase());
        }
        NodeKind::IfExpr { condition, then_branch, else_branch } => {
            collect_signals(arena, *condition, signals);
            for &s in then_branch {
                collect_signals(arena, s, signals);
            }
            if let Some(branch) = else_branch {
                for &s in branch {
                    collect_signals(arena, s, signals);
                }
            }
        }
        NodeKind::Pipeline { left, right } => {
            collect_signals(arena, *left, signals);
            collect_signals(arena, *right, signals);
        }
        NodeKind::ReturnExpr { value } => {
            if let Some(v) = value {
                collect_signals(arena, *v, signals);
            }
        }
        NodeKind::ExprStatement { expr } => {
            collect_signals(arena, *expr, signals);
        }
        NodeKind::LetBinding { value, .. } => {
            collect_signals(arena, *value, signals);
        }
        NodeKind::ForLoop { iterable, body, .. } => {
            collect_signals(arena, *iterable, signals);
            for &s in body {
                collect_signals(arena, s, signals);
            }
        }
        NodeKind::RecordUpdate { base, updates } => {
            collect_signals(arena, *base, signals);
            for (name, val) in updates {
                signals.field_names.push(name.to_lowercase());
                collect_signals(arena, *val, signals);
            }
        }
        NodeKind::RecordConstruct { fields, .. } => {
            for (name, val) in fields {
                signals.field_names.push(name.to_lowercase());
                collect_signals(arena, *val, signals);
            }
        }
        NodeKind::Lambda { body, .. } => {
            collect_signals(arena, *body, signals);
        }
        NodeKind::UnaryOp { operand, .. } => {
            collect_signals(arena, *operand, signals);
        }
        NodeKind::MatchExpr { subject, arms } => {
            collect_signals(arena, *subject, signals);
            for arm in arms {
                for &s in &arm.body {
                    collect_signals(arena, s, signals);
                }
            }
        }
        _ => {}
    }
}

fn keyword_matches(keyword: &str, signals: &BodySignals) -> bool {
    // Direct function call match
    if signals.function_calls.iter().any(|f| f.contains(keyword)) {
        return true;
    }

    // Operator matches
    match keyword {
        "add" | "sum" | "total" | "plus" | "increment" => {
            if signals.operators.iter().any(|o| o == "add") {
                return true;
            }
        }
        "subtract" | "minus" | "remove" | "decrease" | "decrement" => {
            if signals.operators.iter().any(|o| o == "subtract") {
                return true;
            }
        }
        "multiply" | "product" | "times" => {
            if signals.operators.iter().any(|o| o == "multiply") {
                return true;
            }
        }
        "divide" | "division" | "ratio" => {
            if signals.operators.iter().any(|o| o == "divide") {
                return true;
            }
        }
        "filter" | "filtered" | "select" | "where" => {
            if signals.function_calls.contains(&"filter".to_string()) {
                return true;
            }
        }
        "sort" | "sorted" | "order" | "ordered" => {
            if signals.function_calls.contains(&"sort_by".to_string()) {
                return true;
            }
        }
        "greeting" | "hello" | "greet" => {
            if signals.string_literals.iter().any(|s| s.contains("hello") || s.contains("hi")) {
                return true;
            }
            if signals.function_calls.iter().any(|f| f.contains("greet")) {
                return true;
            }
        }
        "print" | "display" | "output" | "show" => {
            if signals.function_calls.contains(&"print".to_string()) {
                return true;
            }
        }
        "map" | "transform" | "convert" => {
            if signals.function_calls.contains(&"map".to_string()) {
                return true;
            }
        }
        "reduce" | "fold" | "accumulate" | "aggregate" => {
            if signals.function_calls.contains(&"reduce".to_string()) {
                return true;
            }
        }
        "string" | "text" | "concatenate" | "concat" | "join" => {
            if signals.operators.iter().any(|o| o == "concat") {
                return true;
            }
        }
        "compare" | "equal" | "equals" | "same" => {
            if signals.operators.iter().any(|o| o == "compare") {
                return true;
            }
        }
        _ => {}
    }

    // Field name match
    if signals.field_names.iter().any(|f| f.contains(keyword)) {
        return true;
    }

    // Identifier match
    if signals.identifiers.iter().any(|i| i.contains(keyword)) {
        return true;
    }

    // String literal contains keyword
    if signals.string_literals.iter().any(|s| s.contains(keyword)) {
        return true;
    }

    false
}
