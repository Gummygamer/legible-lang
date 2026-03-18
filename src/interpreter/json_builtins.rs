/// JSON built-in functions for the Legible language.
///
/// Provides JSON parsing and encoding using `serde_json`, converting between
/// JSON values and Legible runtime values.
use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

fn json_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register JSON built-in functions in the given environment.
pub fn register_json_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("json_parse", builtin_json_parse),
        ("json_encode", builtin_json_encode),
        ("json_valid", builtin_json_valid),
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

/// Convert a `serde_json::Value` into a Legible `Value`.
fn json_to_legible(json: &serde_json::Value) -> Value {
    match json {
        serde_json::Value::Null => Value::None,
        serde_json::Value::Bool(b) => Value::Boolean(*b),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Value::Integer(i)
            } else if let Some(f) = n.as_f64() {
                Value::Decimal(f)
            } else {
                Value::None
            }
        }
        serde_json::Value::String(s) => Value::Text(s.clone()),
        serde_json::Value::Array(arr) => {
            Value::List(arr.iter().map(json_to_legible).collect())
        }
        serde_json::Value::Object(obj) => {
            let entries: Vec<(Value, Value)> = obj
                .iter()
                .map(|(k, v)| (Value::Text(k.clone()), json_to_legible(v)))
                .collect();
            Value::Mapping(entries)
        }
    }
}

/// Convert a Legible `Value` into a `serde_json::Value`.
fn legible_to_json(value: &Value) -> serde_json::Value {
    match value {
        Value::None => serde_json::Value::Null,
        Value::Boolean(b) => serde_json::Value::Bool(*b),
        Value::Integer(n) => serde_json::Value::Number((*n).into()),
        Value::Decimal(n) => {
            serde_json::Number::from_f64(*n)
                .map(serde_json::Value::Number)
                .unwrap_or(serde_json::Value::Null)
        }
        Value::Text(s) => serde_json::Value::String(s.clone()),
        Value::List(items) => {
            serde_json::Value::Array(items.iter().map(legible_to_json).collect())
        }
        Value::Mapping(entries) => {
            let obj: serde_json::Map<String, serde_json::Value> = entries
                .iter()
                .map(|(k, v)| (k.to_string(), legible_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::Record { fields, .. } => {
            let obj: serde_json::Map<String, serde_json::Value> = fields
                .iter()
                .map(|(k, v)| (k.clone(), legible_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        Value::UnionVariant {
            variant_name,
            fields,
            ..
        } => {
            if fields.is_empty() {
                serde_json::Value::String(variant_name.clone())
            } else {
                let mut obj = serde_json::Map::new();
                obj.insert(
                    "type".to_string(),
                    serde_json::Value::String(variant_name.clone()),
                );
                for (k, v) in fields {
                    obj.insert(k.clone(), legible_to_json(v));
                }
                serde_json::Value::Object(obj)
            }
        }
        Value::Function(_) => serde_json::Value::String("<function>".to_string()),
    }
}

/// `json_parse(str: text): Value`
fn builtin_json_parse(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(json_error(
            "json_parse() expects 1 argument",
            "Usage: json_parse(json_string)",
        ));
    }
    let text = match &args[0] {
        Value::Text(s) => s,
        _ => {
            return Err(json_error(
                "json_parse() expects a text argument",
                "Pass a JSON string to parse",
            ))
        }
    };

    let parsed: serde_json::Value = serde_json::from_str(text).map_err(|e| {
        json_error(
            &format!("Invalid JSON: {e}"),
            "Check that the string is valid JSON",
        )
    })?;

    Ok(json_to_legible(&parsed))
}

/// `json_encode(value: T): text`
fn builtin_json_encode(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(json_error(
            "json_encode() expects 1 argument",
            "Usage: json_encode(value)",
        ));
    }

    let json = legible_to_json(&args[0]);
    let text = serde_json::to_string(&json).map_err(|e| {
        json_error(
            &format!("JSON encoding failed: {e}"),
            "Check the value can be represented as JSON",
        )
    })?;

    Ok(Value::Text(text))
}

/// `json_valid(str: text): boolean`
fn builtin_json_valid(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(json_error(
            "json_valid() expects 1 argument",
            "Usage: json_valid(json_string)",
        ));
    }
    match &args[0] {
        Value::Text(s) => Ok(Value::Boolean(serde_json::from_str::<serde_json::Value>(s).is_ok())),
        _ => Err(json_error("json_valid() expects a text argument", "Pass a JSON string")),
    }
}
