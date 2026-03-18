/// Legible language interpreter library.
///
/// Re-exports all modules for testing and embedding.
pub mod analyzer;
pub mod errors;
pub mod formatter;
pub mod interpreter;
pub mod lexer;
pub mod parser;

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use errors::LegibleError;
use interpreter::environment::Environment;
use interpreter::builtins::register_builtins;
use interpreter::crypto_builtins::register_crypto_builtins;
use interpreter::db_builtins::register_db_builtins;
use interpreter::http_builtins::register_http_builtins;
use interpreter::http_client_builtins::register_http_client_builtins;
use interpreter::io_builtins::register_io_builtins;
use interpreter::json_builtins::register_json_builtins;
use interpreter::process_builtins::register_process_builtins;
use interpreter::sdl_builtins::register_sdl_builtins;
use interpreter::value::Value;
use parser::ast::NodeKind;

/// Run a Legible source string through the full pipeline and return stdout output.
///
/// This is the primary entry point for tests and benchmarks.
pub fn run_source(source: &str) -> Result<String, LegibleError> {
    run_source_with_filename(source, "<input>")
}

/// Run a Legible source string with a given filename for error reporting.
pub fn run_source_with_filename(source: &str, filename: &str) -> Result<String, LegibleError> {
    let tokens = lexer::scan(source)?;
    let mut parser_inst = parser::Parser::new(tokens, filename, source);
    let root = parser_inst.parse_program()?;
    let arena = parser_inst.arena;

    // Run static analysis
    let contract_errors = analyzer::contracts::check_contracts(&arena, root, source);
    for err in &contract_errors {
        err.emit_json();
    }
    if contract_errors
        .iter()
        .any(|e| matches!(e.severity, errors::Severity::Error))
    {
        return Err(contract_errors.into_iter().next().unwrap());
    }

    // Intent verification (warnings only)
    let intent_warnings = analyzer::intent::verify_intents(&arena, root);
    for warning in &intent_warnings {
        warning.emit_json();
    }

    // Set up environment with all builtins
    let env = Environment::new();
    register_builtins(&env);
    register_crypto_builtins(&env);
    register_sdl_builtins(&env);
    register_http_builtins(&env);
    register_http_client_builtins(&env);
    register_json_builtins(&env);
    register_io_builtins(&env);
    register_db_builtins(&env);
    register_process_builtins(&env);

    // Load modules referenced by `use` declarations
    let base_dir = Path::new(filename)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let arena_rc = Rc::new(arena);
    load_modules(&arena_rc, root, &base_dir, &env, &mut HashMap::new())?;

    // Evaluate
    let mut output = Vec::new();
    interpreter::evaluate_program_rc(&arena_rc, root, &env, &mut output)?;
    Ok(String::from_utf8_lossy(&output).to_string())
}

/// Run a Legible source string with streaming output to the given writer.
///
/// Unlike `run_source_with_filename`, this writes print output immediately
/// to the provided writer rather than buffering it. This is essential for
/// interactive programs like REPLs that need real-time output.
pub fn run_source_streaming(
    source: &str,
    filename: &str,
    output: &mut dyn std::io::Write,
) -> Result<(), LegibleError> {
    let tokens = lexer::scan(source)?;
    let mut parser_inst = parser::Parser::new(tokens, filename, source);
    let root = parser_inst.parse_program()?;
    let arena = parser_inst.arena;

    // Run static analysis
    let contract_errors = analyzer::contracts::check_contracts(&arena, root, source);
    for err in &contract_errors {
        err.emit_json();
    }
    if contract_errors
        .iter()
        .any(|e| matches!(e.severity, errors::Severity::Error))
    {
        return Err(contract_errors.into_iter().next().unwrap());
    }

    // Intent verification (warnings only)
    let intent_warnings = analyzer::intent::verify_intents(&arena, root);
    for warning in &intent_warnings {
        warning.emit_json();
    }

    // Set up environment with all builtins
    let env = Environment::new();
    register_builtins(&env);
    register_crypto_builtins(&env);
    register_sdl_builtins(&env);
    register_http_builtins(&env);
    register_http_client_builtins(&env);
    register_json_builtins(&env);
    register_io_builtins(&env);
    register_db_builtins(&env);
    register_process_builtins(&env);

    // Load modules referenced by `use` declarations
    let base_dir = Path::new(filename)
        .parent()
        .unwrap_or(Path::new("."))
        .to_path_buf();
    let arena_rc = Rc::new(arena);
    load_modules(&arena_rc, root, &base_dir, &env, &mut HashMap::new())?;

    // Evaluate with streaming output
    interpreter::evaluate_program_rc(&arena_rc, root, &env, output)?;
    Ok(())
}

/// Scan a program's AST for `use` declarations and load the referenced modules.
///
/// Each module is loaded from `base_dir/module_name.lbl`. Its public functions
/// and all record/union declarations are made available as a Record value bound
/// to the module name in the environment. This enables `module_name.function()`
/// access via field access on the record.
fn load_modules(
    arena: &Rc<parser::arena::Arena>,
    root: parser::ast::NodeId,
    base_dir: &Path,
    env: &interpreter::environment::Env,
    loaded: &mut HashMap<String, Value>,
) -> Result<(), LegibleError> {
    let statements = match &arena.get(root).kind {
        NodeKind::Program { statements } => statements.clone(),
        _ => return Ok(()),
    };

    for &stmt_id in &statements {
        if let NodeKind::UseDecl { module_name } = &arena.get(stmt_id).kind {
            let module_name = module_name.clone();

            // If already loaded, reuse the cached module record
            if let Some(cached_record) = loaded.get(&module_name) {
                env.borrow_mut()
                    .define(module_name, cached_record.clone(), false);
                continue;
            }

            let module_path = find_module_file(&module_name, base_dir)?;
            let module_source = std::fs::read_to_string(&module_path).map_err(|e| {
                LegibleError {
                    code: errors::ErrorCode::ImportNotFound,
                    severity: errors::Severity::Error,
                    location: errors::SourceLocation::unknown(),
                    message: format!("Cannot read module '{module_name}': {e}"),
                    context: String::new(),
                    suggestion: format!(
                        "Create file '{}.lbl' in the same directory as the main file",
                        module_name
                    ),
                }
            })?;

            // Parse the module
            let mod_tokens = lexer::scan(&module_source)?;
            let mut mod_parser = parser::Parser::new(
                mod_tokens,
                module_path.to_str().unwrap_or(&module_name),
                &module_source,
            );
            let mod_root = mod_parser.parse_program()?;
            let mod_arena = Rc::new(mod_parser.arena);

            // Set up a fresh env for the module (with builtins)
            let mod_env = Environment::new();
            register_builtins(&mod_env);
            register_crypto_builtins(&mod_env);
            register_sdl_builtins(&mod_env);
            register_http_builtins(&mod_env);
            register_json_builtins(&mod_env);
            register_io_builtins(&mod_env);
            register_db_builtins(&mod_env);

            // Recursively load the module's own dependencies
            let mod_base_dir = module_path.parent().unwrap_or(base_dir).to_path_buf();
            load_modules(&mod_arena, mod_root, &mod_base_dir, &mod_env, loaded)?;

            // Register the module's declarations (don't call main)
            if let NodeKind::Program { ref statements } =
                mod_arena.get(mod_root).kind.clone()
            {
                for &stmt_id in statements {
                    register_module_declaration(&mod_arena, stmt_id, &mod_env)?;
                }
            }

            // Collect public symbols into a Record
            let mut module_fields: Vec<(String, Value)> = Vec::new();
            if let NodeKind::Program { ref statements } =
                mod_arena.get(mod_root).kind.clone()
            {
                for &stmt_id in statements {
                    if let NodeKind::FunctionDecl {
                        name, is_public, ..
                    } = &mod_arena.get(stmt_id).kind
                    {
                        if *is_public {
                            if let Some((val, _)) = mod_env.borrow().get(name) {
                                module_fields.push((name.clone(), val));
                            }
                        }
                    }
                }
            }

            let module_record = Value::Record {
                type_name: module_name.clone(),
                fields: module_fields,
            };

            // Cache the module record for reuse by other modules
            loaded.insert(module_name.clone(), module_record.clone());

            env.borrow_mut()
                .define(module_name, module_record, false);
        }
    }

    Ok(())
}

/// Register a module declaration in the module's environment.
fn register_module_declaration(
    arena: &Rc<parser::arena::Arena>,
    node_id: parser::ast::NodeId,
    env: &interpreter::environment::Env,
) -> Result<(), LegibleError> {
    match &arena.get(node_id).kind.clone() {
        NodeKind::FunctionDecl {
            name,
            params,
            intent,
            requires,
            ensures,
            body,
            ..
        } => {
            let callable = interpreter::value::Callable::UserDefined {
                name: name.clone(),
                params: params.clone(),
                intent: intent.clone(),
                requires: requires.clone(),
                ensures: ensures.clone(),
                body: body.clone(),
                closure_env: interpreter::environment::Env::clone(env),
                source_arena: Rc::clone(arena),
            };
            env.borrow_mut()
                .define(name.clone(), Value::Function(callable), false);
            Ok(())
        }
        _ => Ok(()),
    }
}

/// Find a module file given its name and base directory.
///
/// Search order:
/// 1. `base_dir/module_name.lbl` (same directory)
/// 2. `base_dir/lib/module_name.lbl` (lib subdirectory)
/// 3. `base_dir/../lib/module_name.lbl` (sibling lib directory)
fn find_module_file(module_name: &str, base_dir: &Path) -> Result<PathBuf, LegibleError> {
    let candidates = [
        base_dir.join(format!("{module_name}.lbl")),
        base_dir.join("lib").join(format!("{module_name}.lbl")),
        base_dir.join("..").join("lib").join(format!("{module_name}.lbl")),
    ];

    for path in &candidates {
        if path.exists() {
            return Ok(path.clone());
        }
    }

    Err(LegibleError {
        code: errors::ErrorCode::ImportNotFound,
        severity: errors::Severity::Error,
        location: errors::SourceLocation::unknown(),
        message: format!("Module '{module_name}' not found"),
        context: String::new(),
        suggestion: format!(
            "Create '{module_name}.lbl' in the same directory, in 'lib/', or in '../lib/'"
        ),
    })
}
