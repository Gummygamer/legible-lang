/// SQLite database built-in functions for the Legible language.
///
/// Provides a simple SQLite API:
///   db_open(path)                          — open/create a database file
///   db_exec(sql)                           — run SQL with no result rows
///   db_exec_params(sql, params)            — parameterised exec
///   db_query(sql)                          — run SQL, return list of row mappings
///   db_query_params(sql, params)           — parameterised query
///   db_close()                             — close the connection
use std::cell::RefCell;

use rusqlite::{Connection, ToSql, types::ValueRef};

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

thread_local! {
    static DB_STATE: RefCell<Option<Connection>> = const { RefCell::new(None) };
}

fn db_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

fn with_db<F, R>(f: F) -> Result<R, LegibleError>
where
    F: FnOnce(&Connection) -> Result<R, LegibleError>,
{
    DB_STATE.with(|state| {
        let borrow = state.borrow();
        let conn = borrow.as_ref().ok_or_else(|| {
            db_error(
                "Database not open",
                "Call db_open(path) before using other db functions",
            )
        })?;
        f(conn)
    })
}

/// Register all database built-in functions in the given environment.
pub fn register_db_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("db_open", builtin_db_open),
        ("db_close", builtin_db_close),
        ("db_exec", builtin_db_exec),
        ("db_exec_params", builtin_db_exec_params),
        ("db_query", builtin_db_query),
        ("db_query_params", builtin_db_query_params),
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

/// Convert a Legible Value to a rusqlite-compatible boxed ToSql.
fn value_to_sql(v: &Value) -> Result<Box<dyn ToSql>, LegibleError> {
    match v {
        Value::Text(s) => Ok(Box::new(s.clone())),
        Value::Integer(n) => Ok(Box::new(*n)),
        Value::Decimal(f) => Ok(Box::new(*f)),
        Value::Boolean(b) => Ok(Box::new(if *b { 1i64 } else { 0i64 })),
        Value::None => Ok(Box::new(rusqlite::types::Null)),
        other => Err(db_error(
            &format!("Cannot bind value of type {} as SQL parameter", other.type_name()),
            "Use text, integer, decimal, boolean, or none as SQL parameters",
        )),
    }
}

/// Convert a rusqlite ValueRef to a Legible Value (always text for simplicity).
fn sql_to_value(v: ValueRef<'_>) -> Value {
    match v {
        ValueRef::Null => Value::None,
        ValueRef::Integer(n) => Value::Text(n.to_string()),
        ValueRef::Real(f) => Value::Text(f.to_string()),
        ValueRef::Text(b) => Value::Text(String::from_utf8_lossy(b).into_owned()),
        ValueRef::Blob(b) => Value::Text(format!("<blob {} bytes>", b.len())),
    }
}

/// `db_open(path: text): nothing`
fn builtin_db_open(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(db_error("db_open() expects 1 argument", "Usage: db_open(path)"));
    }
    let path = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(db_error("db_open() expects a text path", "Pass a file path as text")),
    };

    // Create parent directories if needed
    if let Some(parent) = std::path::Path::new(&path).parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|e| {
                db_error(&format!("Failed to create directory for database: {e}"), "Check path is writable")
            })?;
        }
    }

    let conn = Connection::open(&path).map_err(|e| {
        db_error(&format!("Failed to open database '{path}': {e}"), "Check the path is valid and writable")
    })?;

    // Enable WAL mode for better concurrent read performance
    conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")
        .map_err(|e| db_error(&format!("Failed to configure database: {e}"), ""))?;

    DB_STATE.with(|state| {
        *state.borrow_mut() = Some(conn);
    });

    Ok(Value::None)
}

/// `db_close(): nothing`
fn builtin_db_close(_args: &[Value]) -> Result<Value, LegibleError> {
    DB_STATE.with(|state| {
        *state.borrow_mut() = None;
    });
    Ok(Value::None)
}

/// `db_exec(sql: text): nothing`
fn builtin_db_exec(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(db_error("db_exec() expects 1 argument", "Usage: db_exec(sql)"));
    }
    let sql = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(db_error("db_exec() expects a text SQL string", "Pass SQL as text")),
    };

    with_db(|conn| {
        conn.execute_batch(&sql).map_err(|e| {
            db_error(&format!("SQL error: {e}"), "Check your SQL syntax")
        })?;
        Ok(Value::None)
    })
}

/// `db_exec_params(sql: text, params: a list of text): nothing`
fn builtin_db_exec_params(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(db_error("db_exec_params() expects 2 arguments", "Usage: db_exec_params(sql, params)"));
    }
    let sql = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(db_error("db_exec_params() expects a text SQL string", "Pass SQL as text")),
    };
    let param_values = match &args[1] {
        Value::List(items) => items.clone(),
        _ => return Err(db_error("db_exec_params() expects a list of params", "Pass a list of values")),
    };

    let sql_params: Vec<Box<dyn ToSql>> = param_values
        .iter()
        .map(value_to_sql)
        .collect::<Result<Vec<_>, _>>()?;

    with_db(|conn| {
        let params_refs: Vec<&dyn ToSql> = sql_params.iter().map(|b| b.as_ref()).collect();
        conn.execute(&sql, params_refs.as_slice()).map_err(|e| {
            db_error(&format!("SQL error: {e}"), "Check your SQL syntax and parameter types")
        })?;
        Ok(Value::None)
    })
}

/// `db_query(sql: text): a list of a mapping from text to text`
fn builtin_db_query(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(db_error("db_query() expects 1 argument", "Usage: db_query(sql)"));
    }
    let sql = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(db_error("db_query() expects a text SQL string", "Pass SQL as text")),
    };

    with_db(|conn| {
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            db_error(&format!("SQL error: {e}"), "Check your SQL syntax")
        })?;

        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let rows = collect_rows(&mut stmt, &col_names, rusqlite::params![])?;
        Ok(Value::List(rows))
    })
}

/// `db_query_params(sql: text, params: a list of text): a list of a mapping from text to text`
fn builtin_db_query_params(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(db_error("db_query_params() expects 2 arguments", "Usage: db_query_params(sql, params)"));
    }
    let sql = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(db_error("db_query_params() expects a text SQL string", "Pass SQL as text")),
    };
    let param_values = match &args[1] {
        Value::List(items) => items.clone(),
        _ => return Err(db_error("db_query_params() expects a list of params", "Pass a list of values")),
    };

    let sql_params: Vec<Box<dyn ToSql>> = param_values
        .iter()
        .map(value_to_sql)
        .collect::<Result<Vec<_>, _>>()?;

    with_db(|conn| {
        let mut stmt = conn.prepare(&sql).map_err(|e| {
            db_error(&format!("SQL error: {e}"), "Check your SQL syntax")
        })?;

        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();
        let params_refs: Vec<&dyn ToSql> = sql_params.iter().map(|b| b.as_ref()).collect();
        let rows = collect_rows(&mut stmt, &col_names, params_refs.as_slice())?;
        Ok(Value::List(rows))
    })
}

/// Execute a prepared statement with the given params, returning rows as Legible values.
fn collect_rows(
    stmt: &mut rusqlite::Statement<'_>,
    col_names: &[String],
    params: &[&dyn ToSql],
) -> Result<Vec<Value>, LegibleError> {
    let mut rows_out = Vec::new();

    let mut rows = stmt.query(params).map_err(|e| {
        db_error(&format!("Query error: {e}"), "Check SQL and parameter types")
    })?;

    while let Some(row) = rows.next().map_err(|e| {
        db_error(&format!("Error reading row: {e}"), "")
    })? {
        let mut fields: Vec<(Value, Value)> = Vec::new();
        for (i, col) in col_names.iter().enumerate() {
            let raw = row.get_ref(i).map_err(|e| {
                db_error(&format!("Error reading column '{col}': {e}"), "")
            })?;
            fields.push((Value::Text(col.clone()), sql_to_value(raw)));
        }
        rows_out.push(Value::Mapping(fields));
    }

    Ok(rows_out)
}
