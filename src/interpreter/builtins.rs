/// Built-in functions for the Legible standard library.
use crate::errors::{LegibleError, ErrorCode, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

/// Register all built-in functions in the given environment.
pub fn register_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        // I/O (print handled specially in evaluator)
        ("read_line", builtin_read_line),
        // List operations
        ("length", builtin_length),
        ("append", builtin_append),
        ("concat", builtin_concat_list),
        ("contains", builtin_contains),
        ("range", builtin_range),
        // Text operations
        ("split", builtin_split),
        ("join", builtin_join),
        ("trim", builtin_trim),
        ("uppercase", builtin_uppercase),
        ("lowercase", builtin_lowercase),
        ("starts_with", builtin_starts_with),
        ("ends_with", builtin_ends_with),
        ("text_length", builtin_text_length),
        ("to_text", builtin_to_text),
        // Mapping operations
        ("keys", builtin_keys),
        ("values", builtin_values),
        ("has_key", builtin_has_key),
        ("get", builtin_get),
        ("put", builtin_put),
        // Optional operations
        ("unwrap", builtin_unwrap),
        ("unwrap_or", builtin_unwrap_or),
        ("is_some", builtin_is_some),
        ("is_none", builtin_is_none),
        // Math
        ("abs", builtin_abs),
        ("max", builtin_max),
        ("min", builtin_min),
        ("floor", builtin_floor),
        ("ceil", builtin_ceil),
        ("round", builtin_round),
        // Type conversion
        ("to_integer", builtin_to_integer),
        ("to_decimal", builtin_to_decimal),
        // Additional text operations
        ("replace", builtin_replace),
        ("substring", builtin_substring),
        ("contains_text", builtin_contains_text),
        ("index_of", builtin_index_of),
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

    // `print` is registered as a builtin but handled specially in evaluator
    // so it can write to the output writer. We register a placeholder here.
    env.borrow_mut().define(
        "print".to_string(),
        Value::Function(Callable::Builtin {
            name: "print".to_string(),
            func: builtin_print_placeholder,
        }),
        false,
    );
}

fn builtin_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

fn builtin_print_placeholder(_args: &[Value]) -> Result<Value, LegibleError> {
    // This should never be called directly; the evaluator intercepts print calls.
    Ok(Value::None)
}

fn builtin_read_line(_args: &[Value]) -> Result<Value, LegibleError> {
    let mut line = String::new();
    std::io::stdin()
        .read_line(&mut line)
        .map_err(|e| builtin_error(&format!("Failed to read line: {e}"), "Check stdin availability"))?;
    Ok(Value::Text(line.trim_end_matches('\n').to_string()))
}

fn builtin_length(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::List(items)) => Ok(Value::Integer(items.len() as i64)),
        Some(Value::Text(s)) => Ok(Value::Integer(s.len() as i64)),
        Some(Value::Mapping(entries)) => Ok(Value::Integer(entries.len() as i64)),
        _ => Err(builtin_error(
            "length() expects a list, text, or mapping",
            "Pass a list, text, or mapping to length()",
        )),
    }
}

fn builtin_append(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("append() expects 2 arguments", "Usage: append(list, item)"));
    }
    match &args[0] {
        Value::List(items) => {
            let mut new_items = items.clone();
            new_items.push(args[1].clone());
            Ok(Value::List(new_items))
        }
        _ => Err(builtin_error("append() expects a list as first argument", "Pass a list")),
    }
}

fn builtin_concat_list(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("concat() expects 2 arguments", "Usage: concat(list_a, list_b)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(a), Value::List(b)) => {
            let mut result = a.clone();
            result.extend(b.iter().cloned());
            Ok(Value::List(result))
        }
        _ => Err(builtin_error("concat() expects two lists", "Pass two lists")),
    }
}

fn builtin_contains(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("contains() expects 2 arguments", "Usage: contains(list, item)"));
    }
    match &args[0] {
        Value::List(items) => Ok(Value::Boolean(items.contains(&args[1]))),
        _ => Err(builtin_error("contains() expects a list as first argument", "Pass a list")),
    }
}

fn builtin_range(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("range() expects 2 arguments", "Usage: range(start, end_exclusive)"));
    }
    match (&args[0], &args[1]) {
        (Value::Integer(start), Value::Integer(end)) => {
            let items: Vec<Value> = (*start..*end).map(Value::Integer).collect();
            Ok(Value::List(items))
        }
        _ => Err(builtin_error("range() expects two integers", "Pass two integers")),
    }
}

fn builtin_split(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("split() expects 2 arguments", "Usage: split(str, delimiter)"));
    }
    match (&args[0], &args[1]) {
        (Value::Text(s), Value::Text(d)) => {
            let parts: Vec<Value> = s.split(d.as_str()).map(|p| Value::Text(p.to_string())).collect();
            Ok(Value::List(parts))
        }
        _ => Err(builtin_error("split() expects two text arguments", "Pass two text values")),
    }
}

fn builtin_join(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("join() expects 2 arguments", "Usage: join(parts, separator)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(parts), Value::Text(sep)) => {
            let strings: Result<Vec<String>, _> = parts
                .iter()
                .map(|v| match v {
                    Value::Text(s) => Ok(s.clone()),
                    _ => Err(builtin_error("join() list must contain only text", "Ensure all elements are text")),
                })
                .collect();
            Ok(Value::Text(strings?.join(sep)))
        }
        _ => Err(builtin_error("join() expects a list and text separator", "Pass a list and separator")),
    }
}

fn builtin_trim(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Text(s)) => Ok(Value::Text(s.trim().to_string())),
        _ => Err(builtin_error("trim() expects text", "Pass a text value")),
    }
}

fn builtin_uppercase(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Text(s)) => Ok(Value::Text(s.to_uppercase())),
        _ => Err(builtin_error("uppercase() expects text", "Pass a text value")),
    }
}

fn builtin_lowercase(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Text(s)) => Ok(Value::Text(s.to_lowercase())),
        _ => Err(builtin_error("lowercase() expects text", "Pass a text value")),
    }
}

fn builtin_starts_with(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("starts_with() expects 2 arguments", "Usage: starts_with(str, prefix)"));
    }
    match (&args[0], &args[1]) {
        (Value::Text(s), Value::Text(p)) => Ok(Value::Boolean(s.starts_with(p.as_str()))),
        _ => Err(builtin_error("starts_with() expects two text arguments", "Pass two text values")),
    }
}

fn builtin_ends_with(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("ends_with() expects 2 arguments", "Usage: ends_with(str, suffix)"));
    }
    match (&args[0], &args[1]) {
        (Value::Text(s), Value::Text(suffix)) => Ok(Value::Boolean(s.ends_with(suffix.as_str()))),
        _ => Err(builtin_error("ends_with() expects two text arguments", "Pass two text values")),
    }
}

fn builtin_text_length(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Text(s)) => Ok(Value::Integer(s.len() as i64)),
        _ => Err(builtin_error("text_length() expects text", "Pass a text value")),
    }
}

fn builtin_to_text(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(v) => Ok(Value::Text(v.to_string())),
        _ => Err(builtin_error("to_text() expects 1 argument", "Pass a value to convert")),
    }
}

fn builtin_keys(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Mapping(entries)) => {
            let keys: Vec<Value> = entries.iter().map(|(k, _)| k.clone()).collect();
            Ok(Value::List(keys))
        }
        _ => Err(builtin_error("keys() expects a mapping", "Pass a mapping")),
    }
}

fn builtin_values(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Mapping(entries)) => {
            let vals: Vec<Value> = entries.iter().map(|(_, v)| v.clone()).collect();
            Ok(Value::List(vals))
        }
        _ => Err(builtin_error("values() expects a mapping", "Pass a mapping")),
    }
}

fn builtin_has_key(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("has_key() expects 2 arguments", "Usage: has_key(map, key)"));
    }
    match &args[0] {
        Value::Mapping(entries) => {
            let found = entries.iter().any(|(k, _)| k == &args[1]);
            Ok(Value::Boolean(found))
        }
        _ => Err(builtin_error("has_key() expects a mapping as first argument", "Pass a mapping")),
    }
}

fn builtin_get(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("get() expects 2 arguments", "Usage: get(map, key)"));
    }
    match &args[0] {
        Value::Mapping(entries) => {
            for (k, v) in entries {
                if k == &args[1] {
                    return Ok(v.clone());
                }
            }
            Ok(Value::None)
        }
        _ => Err(builtin_error("get() expects a mapping as first argument", "Pass a mapping")),
    }
}

fn builtin_put(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(builtin_error("put() expects 3 arguments", "Usage: put(map, key, value)"));
    }
    match &args[0] {
        Value::Mapping(entries) => {
            let mut new_entries = entries.clone();
            // Update existing or add new
            let mut found = false;
            for entry in &mut new_entries {
                if entry.0 == args[1] {
                    entry.1 = args[2].clone();
                    found = true;
                    break;
                }
            }
            if !found {
                new_entries.push((args[1].clone(), args[2].clone()));
            }
            Ok(Value::Mapping(new_entries))
        }
        _ => Err(builtin_error("put() expects a mapping as first argument", "Pass a mapping")),
    }
}

fn builtin_unwrap(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::None) => Err(LegibleError {
            code: ErrorCode::UnwrapNone,
            severity: Severity::Error,
            location: SourceLocation::unknown(),
            message: "Called unwrap() on a none value".to_string(),
            context: String::new(),
            suggestion: "Use unwrap_or() for a safe default, or check with is_some() first".to_string(),
        }),
        Some(v) => Ok(v.clone()),
        _ => Err(builtin_error("unwrap() expects 1 argument", "Pass an optional value")),
    }
}

fn builtin_unwrap_or(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("unwrap_or() expects 2 arguments", "Usage: unwrap_or(opt, default)"));
    }
    match &args[0] {
        Value::None => Ok(args[1].clone()),
        v => Ok(v.clone()),
    }
}

fn builtin_is_some(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::None) => Ok(Value::Boolean(false)),
        Some(_) => Ok(Value::Boolean(true)),
        _ => Err(builtin_error("is_some() expects 1 argument", "Pass an optional value")),
    }
}

fn builtin_is_none(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::None) => Ok(Value::Boolean(true)),
        Some(_) => Ok(Value::Boolean(false)),
        _ => Err(builtin_error("is_none() expects 1 argument", "Pass an optional value")),
    }
}

fn builtin_abs(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Decimal(n)) => Ok(Value::Decimal(n.abs())),
        Some(Value::Integer(n)) => Ok(Value::Decimal((*n as f64).abs())),
        _ => Err(builtin_error("abs() expects a number", "Pass a decimal or integer")),
    }
}

fn builtin_max(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("max() expects 2 arguments", "Usage: max(a, b)"));
    }
    match (&args[0], &args[1]) {
        (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Decimal(a.max(*b))),
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(*a.max(b))),
        (Value::Integer(a), Value::Decimal(b)) | (Value::Decimal(b), Value::Integer(a)) => {
            Ok(Value::Decimal((*a as f64).max(*b)))
        }
        _ => Err(builtin_error("max() expects two numbers", "Pass two numbers")),
    }
}

fn builtin_min(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("min() expects 2 arguments", "Usage: min(a, b)"));
    }
    match (&args[0], &args[1]) {
        (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Decimal(a.min(*b))),
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(*a.min(b))),
        (Value::Integer(a), Value::Decimal(b)) | (Value::Decimal(b), Value::Integer(a)) => {
            Ok(Value::Decimal((*a as f64).min(*b)))
        }
        _ => Err(builtin_error("min() expects two numbers", "Pass two numbers")),
    }
}

fn builtin_floor(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Decimal(n)) => Ok(Value::Integer(n.floor() as i64)),
        Some(Value::Integer(n)) => Ok(Value::Integer(*n)),
        _ => Err(builtin_error("floor() expects a number", "Pass a decimal")),
    }
}

fn builtin_ceil(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Decimal(n)) => Ok(Value::Integer(n.ceil() as i64)),
        Some(Value::Integer(n)) => Ok(Value::Integer(*n)),
        _ => Err(builtin_error("ceil() expects a number", "Pass a decimal")),
    }
}

fn builtin_round(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Decimal(n)) => Ok(Value::Integer(n.round() as i64)),
        Some(Value::Integer(n)) => Ok(Value::Integer(*n)),
        _ => Err(builtin_error("round() expects a number", "Pass a decimal")),
    }
}

fn builtin_to_integer(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Text(s)) => match s.parse::<i64>() {
            Ok(n) => Ok(Value::Integer(n)),
            Err(_) => Ok(Value::None),
        },
        Some(Value::Decimal(n)) => Ok(Value::Integer(*n as i64)),
        Some(Value::Integer(n)) => Ok(Value::Integer(*n)),
        _ => Err(builtin_error("to_integer() expects text or number", "Pass a text or number value")),
    }
}

fn builtin_to_decimal(args: &[Value]) -> Result<Value, LegibleError> {
    match args.first() {
        Some(Value::Text(s)) => match s.parse::<f64>() {
            Ok(n) => Ok(Value::Decimal(n)),
            Err(_) => Ok(Value::None),
        },
        Some(Value::Integer(n)) => Ok(Value::Decimal(*n as f64)),
        Some(Value::Decimal(n)) => Ok(Value::Decimal(*n)),
        _ => Err(builtin_error("to_decimal() expects text or number", "Pass a text or number value")),
    }
}

fn builtin_replace(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(builtin_error("replace() expects 3 arguments", "Usage: replace(str, from, to)"));
    }
    match (&args[0], &args[1], &args[2]) {
        (Value::Text(s), Value::Text(from), Value::Text(to)) => {
            Ok(Value::Text(s.replace(from.as_str(), to.as_str())))
        }
        _ => Err(builtin_error("replace() expects three text arguments", "Pass three text values")),
    }
}

fn builtin_substring(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(builtin_error("substring() expects 3 arguments", "Usage: substring(str, start, length)"));
    }
    match (&args[0], &args[1], &args[2]) {
        (Value::Text(s), Value::Integer(start), Value::Integer(length)) => {
            let start = *start as usize;
            let length = *length as usize;
            let chars: Vec<char> = s.chars().collect();
            let end = (start + length).min(chars.len());
            let start = start.min(chars.len());
            let result: String = chars[start..end].iter().collect();
            Ok(Value::Text(result))
        }
        _ => Err(builtin_error("substring() expects text, integer, integer", "Pass text and two integers")),
    }
}

fn builtin_contains_text(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("contains_text() expects 2 arguments", "Usage: contains_text(str, substr)"));
    }
    match (&args[0], &args[1]) {
        (Value::Text(s), Value::Text(sub)) => Ok(Value::Boolean(s.contains(sub.as_str()))),
        _ => Err(builtin_error("contains_text() expects two text arguments", "Pass two text values")),
    }
}

fn builtin_index_of(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(builtin_error("index_of() expects 2 arguments", "Usage: index_of(str, substr)"));
    }
    match (&args[0], &args[1]) {
        (Value::Text(s), Value::Text(sub)) => {
            match s.find(sub.as_str()) {
                Some(pos) => Ok(Value::Integer(pos as i64)),
                None => Ok(Value::None),
            }
        }
        _ => Err(builtin_error("index_of() expects two text arguments", "Pass two text values")),
    }
}
