/// Legible language interpreter library.
///
/// Re-exports all modules for testing and embedding.
pub mod analyzer;
pub mod errors;
pub mod formatter;
pub mod interpreter;
pub mod lexer;
pub mod parser;

use errors::LegibleError;
use interpreter::environment::Environment;
use interpreter::builtins::register_builtins;
use interpreter::sdl_builtins::register_sdl_builtins;

/// Run a Legible source string through the full pipeline and return stdout output.
///
/// This is the primary entry point for tests and benchmarks.
pub fn run_source(source: &str) -> Result<String, LegibleError> {
    run_source_with_filename(source, "<input>")
}

/// Run a Legible source string with a given filename for error reporting.
pub fn run_source_with_filename(source: &str, filename: &str) -> Result<String, LegibleError> {
    let tokens = lexer::scan(source)?;
    let mut parser = parser::Parser::new(tokens, filename, source);
    let root = parser.parse_program()?;
    let arena = parser.arena;

    // Run static analysis
    let contract_errors = analyzer::contracts::check_contracts(&arena, root);
    for err in &contract_errors {
        err.emit_json();
    }
    if contract_errors.iter().any(|e| matches!(e.severity, errors::Severity::Error)) {
        return Err(contract_errors.into_iter().next().unwrap());
    }

    // Intent verification (warnings only)
    let intent_warnings = analyzer::intent::verify_intents(&arena, root);
    for warning in &intent_warnings {
        warning.emit_json();
    }

    // Evaluate
    let env = Environment::new();
    register_builtins(&env);
    register_sdl_builtins(&env);
    let mut output = Vec::new();
    interpreter::evaluate_program(&arena, root, &env, &mut output)?;
    Ok(String::from_utf8_lossy(&output).to_string())
}
