/// High-volume list operations implemented outside the tree-walking runtime.
use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

fn list_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register list built-ins whose Rust implementations avoid repeated list cloning.
pub fn register_list_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("sort_integers", builtin_sort_integers),
        ("sort_unique_integers", builtin_sort_unique_integers),
        ("sort_text", builtin_sort_text),
        ("field_values", builtin_field_values),
        ("last_index_of", builtin_last_index_of),
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

fn require_arity(args: &[Value], name: &str, count: usize) -> Result<(), LegibleError> {
    if args.len() == count {
        Ok(())
    } else {
        Err(list_error(
            &format!(
                "{name}() expects {count} argument{}",
                if count == 1 { "" } else { "s" }
            ),
            &format!("Usage: {name}(...)"),
        ))
    }
}

fn expect_list<'a>(args: &'a [Value], index: usize, name: &str) -> Result<&'a [Value], LegibleError> {
    match args.get(index) {
        Some(Value::List(values)) => Ok(values),
        _ => Err(list_error(
            &format!("{name}() expects a list argument"),
            "Pass a list",
        )),
    }
}

fn expect_text<'a>(args: &'a [Value], index: usize, name: &str) -> Result<&'a str, LegibleError> {
    match args.get(index) {
        Some(Value::Text(value)) => Ok(value),
        _ => Err(list_error(
            &format!("{name}() expects a text argument"),
            "Pass a text value",
        )),
    }
}

/// `sort_integers(values: a list of integer): a list of integer`
fn builtin_sort_integers(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "sort_integers", 1)?;
    let mut values: Vec<i64> = (expect_list(args, 0, "sort_integers")?)
        .iter()
        .map(|value| match value {
            Value::Integer(integer) => Ok(*integer),
            _ => Err(list_error(
                "sort_integers() list must contain only integers",
                "Pass a list of integers",
            )),
        })
        .collect::<Result<_, _>>()?;
    values.sort();
    Ok(Value::List(values.into_iter().map(Value::Integer).collect()))
}

/// `sort_unique_integers(values: a list of integer): a list of integer`
fn builtin_sort_unique_integers(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "sort_unique_integers", 1)?;
    let mut values: Vec<i64> = (expect_list(args, 0, "sort_unique_integers")?)
        .iter()
        .map(|value| match value {
            Value::Integer(integer) => Ok(*integer),
            _ => Err(list_error(
                "sort_unique_integers() list must contain only integers",
                "Pass a list of integers",
            )),
        })
        .collect::<Result<_, _>>()?;
    values.sort();
    values.dedup();
    Ok(Value::List(values.into_iter().map(Value::Integer).collect()))
}

/// `sort_text(values: a list of text): a list of text`
fn builtin_sort_text(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "sort_text", 1)?;
    let mut values: Vec<String> = (expect_list(args, 0, "sort_text")?)
        .iter()
        .map(|value| match value {
            Value::Text(text) => Ok(text.clone()),
            _ => Err(list_error(
                "sort_text() list must contain only text values",
                "Pass a list of text values",
            )),
        })
        .collect::<Result<_, _>>()?;
    values.sort();
    Ok(Value::List(values.into_iter().map(Value::Text).collect()))
}

/// `field_values(items: a list of a mapping, key: text): a list`
fn builtin_field_values(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "field_values", 2)?;
    let key = expect_text(args, 1, "field_values")?;
    let values = (expect_list(args, 0, "field_values")?)
        .iter()
        .map(|item| Ok(match item {
            Value::Mapping(entries) => entries
                .iter()
                .find_map(|(candidate, value)| match candidate {
                    Value::Text(candidate) if candidate == key => Some(value.clone()),
                    _ => None,
                })
                .unwrap_or(Value::None),
            _ => {
                return Err(list_error(
                    "field_values() list must contain only mappings",
                    "Pass a list of mappings",
                ));
            }
        }))
        .collect::<Result<Vec<_>, _>>()?;
    Ok(Value::List(values))
}

/// `last_index_of(values: a list, wanted): an optional integer`
fn builtin_last_index_of(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "last_index_of", 2)?;
    let values = expect_list(args, 0, "last_index_of")?;
    Ok(values
        .iter()
        .rposition(|value| value == &args[1])
        .map_or(Value::None, |index| Value::Integer(index as i64)))
}
