/// Process and system built-in functions for the Legible language.
///
/// Provides environment variable access, shell command execution,
/// command-line argument access, directory operations, and process control.
use std::process::Command;

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

fn process_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register process built-in functions in the given environment.
pub fn register_process_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("env_get", builtin_env_get),
        ("shell_exec", builtin_shell_exec),
        ("get_args", builtin_get_args),
        ("exit_process", builtin_exit_process),
        ("list_dir", builtin_list_dir),
        ("create_dir", builtin_create_dir),
        ("get_cwd", builtin_get_cwd),
        ("path_join", builtin_path_join),
        ("is_dir", builtin_is_dir),
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

/// `env_get(name: text): an optional text`
fn builtin_env_get(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(process_error("env_get() expects 1 argument", "Usage: env_get(name)"));
    }
    let name = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(process_error("env_get() expects a text name", "Pass an environment variable name")),
    };

    match std::env::var(name) {
        Ok(val) => Ok(Value::Text(val)),
        Err(_) => Ok(Value::None),
    }
}

/// `shell_exec(command: text): a mapping from text to text`
///
/// Runs a shell command and returns a mapping with "stdout", "stderr", and "exit_code" keys.
fn builtin_shell_exec(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(process_error("shell_exec() expects 1 argument", "Usage: shell_exec(command)"));
    }
    let cmd = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(process_error("shell_exec() expects a text command", "Pass a shell command string")),
    };

    let output = Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .output()
        .map_err(|e| process_error(
            &format!("Failed to execute command: {e}"),
            "Check the command and system availability",
        ))?;

    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    let exit_code = output.status.code().unwrap_or(-1);

    Ok(Value::Mapping(vec![
        (Value::Text("stdout".to_string()), Value::Text(stdout)),
        (Value::Text("stderr".to_string()), Value::Text(stderr)),
        (Value::Text("exit_code".to_string()), Value::Integer(exit_code as i64)),
    ]))
}

/// `get_args(): a list of text`
fn builtin_get_args(args: &[Value]) -> Result<Value, LegibleError> {
    if !args.is_empty() {
        return Err(process_error("get_args() expects no arguments", "Usage: get_args()"));
    }
    let cli_args: Vec<Value> = std::env::args()
        .skip(2) // Skip "legible" and "run"
        .collect::<Vec<String>>()
        .into_iter()
        .map(Value::Text)
        .collect();
    Ok(Value::List(cli_args))
}

/// `exit_process(code: integer): nothing`
fn builtin_exit_process(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(process_error("exit_process() expects 1 argument", "Usage: exit_process(code)"));
    }
    let code = match &args[0] {
        Value::Integer(n) => *n as i32,
        _ => return Err(process_error("exit_process() expects an integer", "Pass an exit code")),
    };
    std::process::exit(code);
}

/// `list_dir(path: text): a list of text`
fn builtin_list_dir(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(process_error("list_dir() expects 1 argument", "Usage: list_dir(path)"));
    }
    let path = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(process_error("list_dir() expects a text path", "Pass a directory path")),
    };

    let entries = std::fs::read_dir(path).map_err(|e| {
        process_error(
            &format!("Failed to read directory '{path}': {e}"),
            "Check the directory path exists",
        )
    })?;

    let mut files: Vec<Value> = Vec::new();
    for entry in entries {
        if let Ok(entry) = entry {
            files.push(Value::Text(
                entry.file_name().to_string_lossy().to_string(),
            ));
        }
    }
    files.sort_by(|a, b| a.to_string().cmp(&b.to_string()));
    Ok(Value::List(files))
}

/// `create_dir(path: text): nothing`
fn builtin_create_dir(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(process_error("create_dir() expects 1 argument", "Usage: create_dir(path)"));
    }
    let path = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(process_error("create_dir() expects a text path", "Pass a directory path")),
    };

    std::fs::create_dir_all(path).map_err(|e| {
        process_error(
            &format!("Failed to create directory '{path}': {e}"),
            "Check the path is valid and writable",
        )
    })?;

    Ok(Value::None)
}

/// `get_cwd(): text`
fn builtin_get_cwd(_args: &[Value]) -> Result<Value, LegibleError> {
    let cwd = std::env::current_dir().map_err(|e| {
        process_error(&format!("Failed to get current directory: {e}"), "Check permissions")
    })?;
    Ok(Value::Text(cwd.to_string_lossy().to_string()))
}

/// `path_join(parts: a list of text): text`
fn builtin_path_join(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() < 2 {
        return Err(process_error("path_join() expects at least 2 arguments", "Usage: path_join(base, segment, ...)"));
    }
    let mut path = std::path::PathBuf::new();
    for arg in args {
        match arg {
            Value::Text(s) => path.push(s),
            _ => return Err(process_error("path_join() expects text arguments", "Pass text path segments")),
        }
    }
    Ok(Value::Text(path.to_string_lossy().to_string()))
}

/// `is_dir(path: text): boolean`
fn builtin_is_dir(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(process_error("is_dir() expects 1 argument", "Usage: is_dir(path)"));
    }
    let path = match &args[0] {
        Value::Text(s) => s,
        _ => return Err(process_error("is_dir() expects a text path", "Pass a path")),
    };
    Ok(Value::Boolean(std::path::Path::new(path).is_dir()))
}
