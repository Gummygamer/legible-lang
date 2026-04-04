/// HTTP client built-in functions for the Legible language.
///
/// Provides synchronous HTTP client operations using `ureq` for making
/// outbound HTTP requests (GET, POST with JSON, etc.).
use std::time::Duration;

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

fn http_agent() -> ureq::Agent {
    ureq::AgentBuilder::new()
        .timeout_read(Duration::from_secs(600))
        .timeout_write(Duration::from_secs(10))
        .build()
}

fn client_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register HTTP client built-in functions in the given environment.
pub fn register_http_client_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("http_client_get", builtin_http_client_get),
        ("http_client_post", builtin_http_client_post),
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

/// `http_client_get(url: text, headers: a mapping from text to text): a mapping from text to text`
///
/// Makes a GET request and returns a mapping with "status", "body", and "headers" keys.
fn builtin_http_client_get(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(client_error(
            "http_client_get() expects 2 arguments",
            "Usage: http_client_get(url, headers)",
        ));
    }

    let url = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(client_error("http_client_get() expects a text URL", "Pass a URL string")),
    };

    let header_map = match &args[1] {
        Value::Mapping(entries) => entries.clone(),
        _ => return Err(client_error(
            "http_client_get() expects a mapping for headers",
            "Pass a mapping from text to text",
        )),
    };

    let agent = http_agent();
    let mut request = agent.get(&url);
    for (k, v) in &header_map {
        if let (Value::Text(key), Value::Text(val)) = (k, v) {
            request = request.set(key, val);
        }
    }

    match request.call() {
        Ok(response) => {
            let status = response.status().to_string();
            let body = response.into_string().unwrap_or_default();
            Ok(Value::Mapping(vec![
                (Value::Text("status".to_string()), Value::Text(status)),
                (Value::Text("body".to_string()), Value::Text(body)),
            ]))
        }
        Err(ureq::Error::Status(code, response)) => {
            let body = response.into_string().unwrap_or_default();
            Ok(Value::Mapping(vec![
                (Value::Text("status".to_string()), Value::Text(code.to_string())),
                (Value::Text("body".to_string()), Value::Text(body)),
            ]))
        }
        Err(e) => Err(client_error(
            &format!("HTTP GET failed: {e}"),
            "Check the URL and network connectivity",
        )),
    }
}

/// `http_client_post(url: text, headers: a mapping from text to text, body: text): a mapping from text to text`
///
/// Makes a POST request with a text body and returns a mapping with "status" and "body" keys.
fn builtin_http_client_post(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(client_error(
            "http_client_post() expects 3 arguments",
            "Usage: http_client_post(url, headers, body)",
        ));
    }

    let url = match &args[0] {
        Value::Text(s) => s.clone(),
        _ => return Err(client_error("http_client_post() expects a text URL", "Pass a URL string")),
    };

    let header_map = match &args[1] {
        Value::Mapping(entries) => entries.clone(),
        _ => return Err(client_error(
            "http_client_post() expects a mapping for headers",
            "Pass a mapping from text to text",
        )),
    };

    let body = match &args[2] {
        Value::Text(s) => s.clone(),
        _ => return Err(client_error(
            "http_client_post() expects a text body",
            "Pass a text string as the request body",
        )),
    };

    let agent = http_agent();
    let mut request = agent.post(&url);
    for (k, v) in &header_map {
        if let (Value::Text(key), Value::Text(val)) = (k, v) {
            request = request.set(key, val);
        }
    }

    match request.send_string(&body) {
        Ok(response) => {
            let status = response.status().to_string();
            let resp_body = response.into_string().unwrap_or_default();
            Ok(Value::Mapping(vec![
                (Value::Text("status".to_string()), Value::Text(status)),
                (Value::Text("body".to_string()), Value::Text(resp_body)),
            ]))
        }
        Err(ureq::Error::Status(code, response)) => {
            let resp_body = response.into_string().unwrap_or_default();
            Ok(Value::Mapping(vec![
                (Value::Text("status".to_string()), Value::Text(code.to_string())),
                (Value::Text("body".to_string()), Value::Text(resp_body)),
            ]))
        }
        Err(e) => Err(client_error(
            &format!("HTTP POST failed: {e}"),
            "Check the URL, headers, and network connectivity",
        )),
    }
}
