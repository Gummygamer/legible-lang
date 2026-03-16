/// HTTP server built-in functions for the Legible language.
///
/// Provides a synchronous HTTP server using `tiny_http`, following the same
/// thread-local state pattern as SDL builtins.
use std::cell::RefCell;

use tiny_http::{Header, Response, Server, StatusCode};

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

/// Thread-local HTTP server state.
struct HttpState {
    server: Server,
    current_request: Option<tiny_http::Request>,
}

thread_local! {
    static HTTP_STATE: RefCell<Option<HttpState>> = const { RefCell::new(None) };
}

fn http_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register all HTTP built-in functions in the given environment.
pub fn register_http_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("http_start", builtin_http_start),
        ("http_next_request", builtin_http_next_request),
        ("http_respond", builtin_http_respond),
        (
            "http_respond_with_headers",
            builtin_http_respond_with_headers,
        ),
        ("http_stop", builtin_http_stop),
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

fn with_http<F, R>(f: F) -> Result<R, LegibleError>
where
    F: FnOnce(&mut HttpState) -> Result<R, LegibleError>,
{
    HTTP_STATE.with(|state| {
        let mut borrow = state.borrow_mut();
        let http = borrow.as_mut().ok_or_else(|| {
            http_error(
                "HTTP server not started",
                "Call http_start(port) before using other HTTP functions",
            )
        })?;
        f(http)
    })
}

/// `http_start(port: integer): nothing`
fn builtin_http_start(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 1 {
        return Err(http_error(
            "http_start() expects 1 argument",
            "Usage: http_start(port)",
        ));
    }
    let port = match &args[0] {
        Value::Integer(n) => *n,
        _ => {
            return Err(http_error(
                "http_start() expects an integer port",
                "Pass an integer port number",
            ))
        }
    };

    let addr = format!("0.0.0.0:{port}");
    let server = Server::http(&addr).map_err(|e| {
        http_error(
            &format!("Failed to start HTTP server on port {port}: {e}"),
            "Check that the port is available and valid (1-65535)",
        )
    })?;

    eprintln!("Legible HTTP server listening on http://0.0.0.0:{port}");

    HTTP_STATE.with(|state| {
        *state.borrow_mut() = Some(HttpState {
            server,
            current_request: None,
        });
    });

    Ok(Value::None)
}

/// `http_next_request(): Request`
///
/// Blocks until a request arrives. Returns a Record with fields:
/// method, path, body, query, headers.
fn builtin_http_next_request(_args: &[Value]) -> Result<Value, LegibleError> {
    with_http(|http| {
        let mut request = http
            .server
            .recv()
            .map_err(|e| http_error(&format!("Failed to receive request: {e}"), "Check server state"))?;

        let method = request.method().to_string();
        let url = request.url().to_string();

        // Split path and query string
        let (path, query) = match url.split_once('?') {
            Some((p, q)) => (p.to_string(), q.to_string()),
            None => (url, String::new()),
        };

        // Read headers
        let mut headers = Vec::new();
        for header in request.headers() {
            headers.push((
                Value::Text(header.field.to_string().to_lowercase()),
                Value::Text(header.value.to_string()),
            ));
        }

        // Read body
        let mut body = String::new();
        let _ = request.as_reader().read_to_string(&mut body);

        // Store request for responding later
        http.current_request = Some(request);

        Ok(Value::Record {
            type_name: "Request".to_string(),
            fields: vec![
                ("method".to_string(), Value::Text(method)),
                ("path".to_string(), Value::Text(path)),
                ("body".to_string(), Value::Text(body)),
                ("query".to_string(), Value::Text(query)),
                ("headers".to_string(), Value::Mapping(headers)),
            ],
        })
    })
}

/// `http_respond(status: integer, body: text): nothing`
fn builtin_http_respond(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 2 {
        return Err(http_error(
            "http_respond() expects 2 arguments",
            "Usage: http_respond(status, body)",
        ));
    }
    let status = match &args[0] {
        Value::Integer(n) => *n as i32,
        _ => {
            return Err(http_error(
                "http_respond() expects an integer status code",
                "Pass an integer like 200, 404, etc.",
            ))
        }
    };
    let body = match &args[1] {
        Value::Text(s) => s.clone(),
        _ => {
            return Err(http_error(
                "http_respond() expects a text body",
                "Pass a text string as the response body",
            ))
        }
    };

    with_http(|http| {
        let request = http.current_request.take().ok_or_else(|| {
            http_error(
                "No current request to respond to",
                "Call http_next_request() before http_respond()",
            )
        })?;

        let response = Response::from_string(&body)
            .with_status_code(StatusCode(status as u16))
            .with_header(
                Header::from_bytes(&b"Content-Type"[..], &b"text/plain; charset=utf-8"[..])
                    .unwrap(),
            );

        request.respond(response).map_err(|e| {
            http_error(
                &format!("Failed to send response: {e}"),
                "Check connection state",
            )
        })?;

        Ok(Value::None)
    })
}

/// `http_respond_with_headers(status: integer, headers: a mapping from text to text, body: text): nothing`
fn builtin_http_respond_with_headers(args: &[Value]) -> Result<Value, LegibleError> {
    if args.len() != 3 {
        return Err(http_error(
            "http_respond_with_headers() expects 3 arguments",
            "Usage: http_respond_with_headers(status, headers, body)",
        ));
    }
    let status = match &args[0] {
        Value::Integer(n) => *n as i32,
        _ => {
            return Err(http_error(
                "Expected integer status code",
                "Pass an integer like 200, 404, etc.",
            ))
        }
    };
    let header_map = match &args[1] {
        Value::Mapping(entries) => entries.clone(),
        _ => {
            return Err(http_error(
                "Expected a mapping for headers",
                "Pass a mapping from text to text",
            ))
        }
    };
    let body = match &args[2] {
        Value::Text(s) => s.clone(),
        _ => {
            return Err(http_error(
                "Expected text body",
                "Pass a text string as the response body",
            ))
        }
    };

    with_http(|http| {
        let request = http.current_request.take().ok_or_else(|| {
            http_error(
                "No current request to respond to",
                "Call http_next_request() before responding",
            )
        })?;

        let mut response = Response::from_string(&body)
            .with_status_code(StatusCode(status as u16));

        for (key, val) in &header_map {
            if let (Value::Text(k), Value::Text(v)) = (key, val) {
                if let Ok(header) = Header::from_bytes(k.as_bytes(), v.as_bytes()) {
                    response = response.with_header(header);
                }
            }
        }

        request.respond(response).map_err(|e| {
            http_error(
                &format!("Failed to send response: {e}"),
                "Check connection state",
            )
        })?;

        Ok(Value::None)
    })
}

/// `http_stop(): nothing`
fn builtin_http_stop(_args: &[Value]) -> Result<Value, LegibleError> {
    HTTP_STATE.with(|state| {
        *state.borrow_mut() = None;
    });
    eprintln!("HTTP server stopped.");
    Ok(Value::None)
}
