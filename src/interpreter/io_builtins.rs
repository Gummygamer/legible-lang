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
        ("url_decode", builtin_url_decode),
        ("random_hex", builtin_random_hex),
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

/// `url_decode(str: text): text`
///
/// Percent-decode a URL-encoded string (e.g., form body values).
/// Converts `+` to space and `%XX` hex sequences to their byte values.
fn builtin_url_decode(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(io_error(
            "url_decode() expects 1 argument",
            "Usage: url_decode(encoded_string)",
        ));
    }
    let input = match &args[0] {
        Value::Text(s) => s,
        _ => {
            return Err(io_error(
                "url_decode() expects a text argument",
                "Pass a URL-encoded text string",
            ))
        }
    };

    let mut result = Vec::with_capacity(input.len());
    let bytes = input.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'+' => {
                result.push(b' ');
                i += 1;
            }
            b'%' if i + 2 < bytes.len() => {
                let hi = bytes[i + 1];
                let lo = bytes[i + 2];
                if let (Some(h), Some(l)) = (hex_val(hi), hex_val(lo)) {
                    result.push(h << 4 | l);
                    i += 3;
                } else {
                    result.push(b'%');
                    i += 1;
                }
            }
            b => {
                result.push(b);
                i += 1;
            }
        }
    }

    let decoded = String::from_utf8(result).unwrap_or_else(|_| input.clone());
    Ok(Value::Text(decoded))
}

fn hex_val(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

/// `random_hex(length: integer): text`
///
/// Generate a random hexadecimal string of the given length.
/// Uses `/dev/urandom` on Unix systems for cryptographic randomness.
fn builtin_random_hex(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(io_error(
            "random_hex() expects 1 argument",
            "Usage: random_hex(length)",
        ));
    }
    let length = match &args[0] {
        Value::Integer(n) if *n > 0 => *n as usize,
        Value::Integer(_) => {
            return Err(io_error(
                "random_hex() expects a positive integer",
                "Pass a positive integer for the hex string length",
            ))
        }
        _ => {
            return Err(io_error(
                "random_hex() expects an integer",
                "Pass an integer for the hex string length",
            ))
        }
    };

    // Read random bytes and convert to hex
    let num_bytes = (length + 1) / 2;
    let random_bytes = {
        use std::io::Read;
        let mut bytes = vec![0u8; num_bytes];
        std::fs::File::open("/dev/urandom")
            .and_then(|mut f| f.read_exact(&mut bytes).map(|_| bytes))
    }
    .or_else(|_| {
        // Fallback: use system time + counter for basic randomness
        let seed = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(42);
        let mut bytes = Vec::with_capacity(num_bytes);
        let mut state = seed;
        for _ in 0..num_bytes {
            state = state.wrapping_mul(6364136223846793005).wrapping_add(1);
            bytes.push((state >> 33) as u8);
        }
        Ok(bytes)
    })
    .map_err(|e: std::io::Error| {
        io_error(
            &format!("Failed to generate random bytes: {e}"),
            "System randomness unavailable",
        )
    })?;

    let hex: String = random_bytes
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();

    Ok(Value::Text(hex[..length].to_string()))
}
