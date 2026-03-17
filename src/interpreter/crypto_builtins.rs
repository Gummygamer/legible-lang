/// Cryptographic builtins for the Legible language.
///
/// Provides password hashing (argon2id) and verification.
use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use rand_core::OsRng;

fn runtime_error(message: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::RuntimeError,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: "Check the input values passed to the crypto builtin.".to_string(),
    }
}

/// `password_hash(password: text): text`
///
/// Hashes a plaintext password using argon2id with a random salt.
/// Returns a self-describing PHC string that includes the algorithm,
/// parameters, salt, and hash — safe to store directly in the database.
fn builtin_password_hash(args: &[Value]) -> Result<Value, LegibleError> {
    let password = match args.first() {
        Some(Value::Text(s)) => s.as_bytes().to_vec(),
        _ => return Err(runtime_error("password_hash requires a text argument")),
    };
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default()
        .hash_password(&password, &salt)
        .map_err(|e| runtime_error(&format!("password_hash failed: {e}")))?
        .to_string();
    Ok(Value::Text(hash))
}

/// `password_verify(password: text, hash: text): boolean`
///
/// Returns true if the plaintext password matches the argon2 hash string.
/// Returns false for any mismatch or malformed hash (never errors on bad input).
fn builtin_password_verify(args: &[Value]) -> Result<Value, LegibleError> {
    let (password, hash_str) = match (args.first(), args.get(1)) {
        (Some(Value::Text(p)), Some(Value::Text(h))) => (p.as_bytes().to_vec(), h.clone()),
        _ => {
            return Err(runtime_error(
                "password_verify requires two text arguments: (password, hash)",
            ))
        }
    };
    let parsed = match PasswordHash::new(&hash_str) {
        Ok(h) => h,
        Err(_) => return Ok(Value::Boolean(false)),
    };
    let matches = Argon2::default()
        .verify_password(&password, &parsed)
        .is_ok();
    Ok(Value::Boolean(matches))
}

/// Register all crypto builtins into the environment.
pub fn register_crypto_builtins(env: &Env) {
    let builtins: &[(&str, fn(&[Value]) -> Result<Value, LegibleError>)] = &[
        ("password_hash", builtin_password_hash),
        ("password_verify", builtin_password_verify),
    ];
    for (name, func) in builtins {
        env.borrow_mut().define(
            name.to_string(),
            Value::Function(Callable::Builtin {
                name: name.to_string(),
                func: *func,
            }),
            false,
        );
    }
}
