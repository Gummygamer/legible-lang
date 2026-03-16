/// File I/O and utility built-in functions for the Legible language.
///
/// Provides file reading/writing and time utilities needed for web backends.
use std::time::{SystemTime, UNIX_EPOCH};

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

fn io_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register file I/O and utility built-in functions in the given environment.
pub fn register_io_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("read_file", builtin_read_file),
        ("write_file", builtin_write_file),
        ("file_exists", builtin_file_exists),
        ("current_time_ms", builtin_current_time_ms),
        ("log", builtin_log),
    ];

    for (name, func) in builtins {
        env.borrow_mut().define(
            name.to_string(),
            Value::Function(Callable::Builtin {
                name: name.to_string(),
                func,
            }),
            false,
        );
    }
}

/// `read_file(path: text): text`
fn builtin_read_file(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(io_error(
            "read_file() expects 1 argument",
            "Usage: read_file(path)",
        ));
    }
    let path = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(io_error("read_file() expects a text path", "Pass a file path as text")),
    };

    let content = std::fs::read_to_string(path).map_err(|e| {
        io_error(
            &format!("Failed to read file '{path}': {e}"),
            "Check the file path exists and is readable",
        )
    })?;

    Ok(Value::Text(content))
}

/// `write_file(path: text, content: text): nothing`
fn builtin_write_file(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(io_error(
            "write_file() expects 2 arguments",
            "Usage: write_file(path, content)",
        ));
    }
    let path = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(io_error("write_file() expects a text path", "Pass a file path as text")),
    };
    let content = match &args[1] {
        Value::Text(s) => s,
        _ => return Err(io_error("write_file() expects text content", "Pass text content to write")),
    };

    std::fs::write(path, content).map_err(|e| {
        io_error(
            &format!("Failed to write file '{path}': {e}"),
            "Check the file path is writable",
        )
    })?;

    Ok(Value::None)
}

/// `file_exists(path: text): boolean`
fn builtin_file_exists(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(io_error(
            "file_exists() expects 1 argument",
            "Usage: file_exists(path)",
        ));
    }
    let path = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(io_error("file_exists() expects a text path", "Pass a file path as text")),
    };

    Ok(Value::Boolean(std::path::Path::new(path).exists()))
}

/// `current_time_ms(): integer`
fn builtin_current_time_ms(_args: &[Value]) -> Result<Value, LegibleError> {
    let ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| io_error(&format!("Time error: {e}"), "System clock issue"))?
        .as_millis() as i64;
    Ok(Value::Integer(ms))
}

/// `log(level: text, message: text): nothing`
fn builtin_log(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(io_error(
            "log() expects 2 arguments",
            "Usage: log(level, message)",
        ));
    }
    let level = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(io_error("log() expects text level", "Pass a text level like \"info\"")),
    };
    let message = match &args[1] {
        Value::Text(s) => s.clone(),
        v => v.to_string(),
    };

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);

    eprintln!("[{timestamp}] [{level}] {message}");
    Ok(Value::None)
}
