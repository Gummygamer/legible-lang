/// Opaque byte-buffer and bitwise integer built-in functions.
///
/// Byte buffers live in a process-global Rust registry and are represented in
/// Legible by positive integer handles. This keeps large binary files out of
/// the tree-walking runtime value graph.
use std::sync::{Mutex, OnceLock};

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

static BUFFERS: OnceLock<Mutex<Vec<Option<Vec<u8>>>>> = OnceLock::new();

fn buffers() -> &'static Mutex<Vec<Option<Vec<u8>>>> {
    BUFFERS.get_or_init(|| Mutex::new(Vec::new()))
}

fn bytes_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register opaque byte-buffer and bitwise integer built-ins in the given environment.
pub fn register_bytes_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("read_file_bytes", builtin_read_file_bytes),
        ("bytes_new", builtin_bytes_new),
        ("bytes_from_hex", builtin_bytes_from_hex),
        ("bytes_from_text", builtin_bytes_from_text),
        ("bytes_concat", builtin_bytes_concat),
        ("bytes_length", builtin_bytes_length),
        ("bytes_get", builtin_bytes_get),
        ("bytes_slice", builtin_bytes_slice),
        ("bytes_inflate", builtin_bytes_inflate),
        ("bytes_to_text", builtin_bytes_to_text),
        ("bytes_read_u32_le", builtin_bytes_read_u32_le),
        ("bytes_read_u16_le", builtin_bytes_read_u16_le),
        ("bytes_set", builtin_bytes_set),
        ("bytes_fill", builtin_bytes_fill),
        ("bytes_write_bytes", builtin_bytes_write_bytes),
        ("bytes_write_u32_le", builtin_bytes_write_u32_le),
        ("bytes_index_of", builtin_bytes_index_of),
        ("bytes_scan_words", builtin_bytes_scan_words),
        ("write_file_bytes", builtin_write_file_bytes),
        ("bytes_free", builtin_bytes_free),
        ("bit_and", builtin_bit_and),
        ("bit_or", builtin_bit_or),
        ("bit_xor", builtin_bit_xor),
        ("bit_not", builtin_bit_not),
        ("shift_left", builtin_shift_left),
        ("shift_right", builtin_shift_right),
        ("shift_right_unsigned", builtin_shift_right_unsigned),
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

fn expect_integer<'a>(args: &'a [Value], index: usize, name: &str) -> Result<i64, LegibleError> {
    match args.get(index) {
        Some(Value::Integer(value)) => Ok(*value),
        _ => Err(bytes_error(
            &format!("{name}() expects an integer argument"),
            "Pass integer arguments",
        )),
    }
}

fn expect_text<'a>(args: &'a [Value], index: usize, name: &str) -> Result<&'a str, LegibleError> {
    match args.get(index) {
        Some(Value::Text(value)) => Ok(value),
        _ => Err(bytes_error(
            &format!("{name}() expects a text argument"),
            "Pass text arguments",
        )),
    }
}

fn require_arity(args: &[Value], name: &str, count: usize) -> Result<(), LegibleError> {
    if args.len() == count {
        Ok(())
    } else {
        Err(bytes_error(
            &format!(
                "{name}() expects {count} argument{}",
                if count == 1 { "" } else { "s" }
            ),
            &format!("Usage: {name}(...)"),
        ))
    }
}

fn store_buffer(buffer: Vec<u8>) -> Result<Value, LegibleError> {
    let mut registry = buffers().lock().map_err(|_| {
        bytes_error(
            "Byte buffer registry is unavailable",
            "Try again in a new process",
        )
    })?;
    registry.push(Some(buffer));
    Ok(Value::Integer(registry.len() as i64))
}

fn handle_index(handle: i64) -> Result<usize, LegibleError> {
    if handle <= 0 {
        return Err(bytes_error(
            "Invalid byte buffer handle",
            "Use a handle returned by a bytes builtin",
        ));
    }
    usize::try_from(handle - 1).map_err(|_| {
        bytes_error(
            "Invalid byte buffer handle",
            "Use a handle returned by a bytes builtin",
        )
    })
}

pub(crate) fn with_buffer<T>(
    handle: i64,
    action: impl FnOnce(&[u8]) -> Result<T, LegibleError>,
) -> Result<T, LegibleError> {
    let index = handle_index(handle)?;
    let registry = buffers().lock().map_err(|_| {
        bytes_error(
            "Byte buffer registry is unavailable",
            "Try again in a new process",
        )
    })?;
    let buffer = registry
        .get(index)
        .and_then(Option::as_deref)
        .ok_or_else(|| {
            bytes_error(
                "Unknown or freed byte buffer handle",
                "Use a live handle returned by a bytes builtin",
            )
        })?;
    action(buffer)
}

fn with_buffer_mut<T>(
    handle: i64,
    action: impl FnOnce(&mut Vec<u8>) -> Result<T, LegibleError>,
) -> Result<T, LegibleError> {
    let index = handle_index(handle)?;
    let mut registry = buffers().lock().map_err(|_| {
        bytes_error(
            "Byte buffer registry is unavailable",
            "Try again in a new process",
        )
    })?;
    let buffer = registry
        .get_mut(index)
        .and_then(Option::as_mut)
        .ok_or_else(|| {
            bytes_error(
                "Unknown or freed byte buffer handle",
                "Use a live handle returned by a bytes builtin",
            )
        })?;
    action(buffer)
}

fn nonnegative_index(value: i64, name: &str) -> Result<usize, LegibleError> {
    if value < 0 {
        return Err(bytes_error(
            &format!("{name}() index values must be non-negative"),
            "Pass a non-negative byte offset or length",
        ));
    }
    Ok(value as usize)
}

/// `read_file_bytes(path: text): integer`
fn builtin_read_file_bytes(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "read_file_bytes", 1)?;
    let path = expect_text(args, 0, "read_file_bytes")?;
    let content = std::fs::read(path).map_err(|error| {
        bytes_error(
            &format!("Failed to read file '{path}': {error}"),
            "Check the file path exists and is readable",
        )
    })?;
    store_buffer(content)
}

/// `bytes_new(length: integer): integer`
fn builtin_bytes_new(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_new", 1)?;
    let length = nonnegative_index(expect_integer(args, 0, "bytes_new")?, "bytes_new")?;
    store_buffer(vec![0; length])
}

/// `bytes_from_hex(content: text): integer`
fn builtin_bytes_from_hex(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_from_hex", 1)?;
    let text = expect_text(args, 0, "bytes_from_hex")?;
    let digits: Vec<u8> = text
        .bytes()
        .filter(|byte| !byte.is_ascii_whitespace())
        .collect();
    if digits.len() % 2 != 0 {
        return Err(bytes_error(
            "bytes_from_hex() requires an even number of hex digits",
            "Pass pairs of hexadecimal digits",
        ));
    }
    let mut buffer = Vec::with_capacity(digits.len() / 2);
    for pair in digits.chunks_exact(2) {
        let high = hex_digit(pair[0]).ok_or_else(|| {
            bytes_error(
                "bytes_from_hex() received a non-hex character",
                "Pass only hexadecimal digits and whitespace",
            )
        })?;
        let low = hex_digit(pair[1]).ok_or_else(|| {
            bytes_error(
                "bytes_from_hex() received a non-hex character",
                "Pass only hexadecimal digits and whitespace",
            )
        })?;
        buffer.push((high << 4) | low);
    }
    store_buffer(buffer)
}

fn hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

/// `bytes_from_text(content: text): integer`
fn builtin_bytes_from_text(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_from_text", 1)?;
    Ok(store_buffer(
        expect_text(args, 0, "bytes_from_text")?.as_bytes().to_vec(),
    )?)
}

/// `bytes_concat(a: integer, b: integer): integer`
fn builtin_bytes_concat(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_concat", 2)?;
    let first = expect_integer(args, 0, "bytes_concat")?;
    let second = expect_integer(args, 1, "bytes_concat")?;
    let mut combined = with_buffer(first, |buffer| Ok(buffer.to_vec()))?;
    with_buffer(second, |buffer| {
        combined.extend_from_slice(buffer);
        Ok(())
    })?;
    store_buffer(combined)
}

/// `bytes_length(handle: integer): integer`
fn builtin_bytes_length(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_length", 1)?;
    let handle = expect_integer(args, 0, "bytes_length")?;
    with_buffer(handle, |buffer| Ok(Value::Integer(buffer.len() as i64)))
}

/// `bytes_get(handle: integer, index: integer): integer`
fn builtin_bytes_get(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_get", 2)?;
    let handle = expect_integer(args, 0, "bytes_get")?;
    let index = nonnegative_index(expect_integer(args, 1, "bytes_get")?, "bytes_get")?;
    with_buffer(handle, |buffer| {
        buffer
            .get(index)
            .map(|byte| Value::Integer(*byte as i64))
            .ok_or_else(|| {
                bytes_error(
                    "bytes_get() index is out of bounds",
                    "Use an index smaller than bytes_length(handle)",
                )
            })
    })
}

/// `bytes_slice(handle: integer, start: integer, length: integer): integer`
fn builtin_bytes_slice(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_slice", 3)?;
    let handle = expect_integer(args, 0, "bytes_slice")?;
    let start = nonnegative_index(expect_integer(args, 1, "bytes_slice")?, "bytes_slice")?;
    let length = nonnegative_index(expect_integer(args, 2, "bytes_slice")?, "bytes_slice")?;
    let slice = with_buffer(handle, |buffer| {
        let end = start.checked_add(length).ok_or_else(|| {
            bytes_error(
                "bytes_slice() range is out of bounds",
                "Use a range within the buffer",
            )
        })?;
        buffer
            .get(start..end)
            .map(|part| part.to_vec())
            .ok_or_else(|| {
                bytes_error(
                    "bytes_slice() range is out of bounds",
                    "Use a range within the buffer",
                )
            })
    })?;
    store_buffer(slice)
}

/// `bytes_inflate(source_handle: integer, expected_size: integer): integer`
fn builtin_bytes_inflate(args: &[Value]) -> Result<Value, LegibleError> {
    use std::io::Read;

    require_arity(args, "bytes_inflate", 2)?;
    let handle = expect_integer(args, 0, "bytes_inflate")?;
    let expected = expect_integer(args, 1, "bytes_inflate")?;
    let compressed = with_buffer(handle, |buffer| Ok(buffer.to_vec()))?;
    let mut output = Vec::new();
    flate2::read::DeflateDecoder::new(&compressed[..])
        .read_to_end(&mut output)
        .map_err(|error| {
            bytes_error(
                &format!("bytes_inflate() could not decompress the DEFLATE stream: {error}"),
                "Pass a valid raw-DEFLATE (ZIP method 8) byte buffer",
            )
        })?;
    if expected > 0 && output.len() as i64 != expected {
        return Err(bytes_error(
            &format!(
                "bytes_inflate() produced {} bytes, expected {expected}",
                output.len()
            ),
            "The declared uncompressed size does not match the stream",
        ));
    }
    store_buffer(output)
}

/// `bytes_to_text(handle: integer): text`
fn builtin_bytes_to_text(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_to_text", 1)?;
    let handle = expect_integer(args, 0, "bytes_to_text")?;
    with_buffer(handle, |buffer| {
        Ok(Value::Text(String::from_utf8_lossy(buffer).to_string()))
    })
}

/// `bytes_read_u32_le(handle: integer, offset: integer): integer`
fn builtin_bytes_read_u32_le(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_read_u32_le", 2)?;
    let handle = expect_integer(args, 0, "bytes_read_u32_le")?;
    let offset = nonnegative_index(
        expect_integer(args, 1, "bytes_read_u32_le")?,
        "bytes_read_u32_le",
    )?;
    with_buffer(handle, |buffer| {
        let bytes = buffer
            .get(offset..offset.saturating_add(4))
            .filter(|bytes| bytes.len() == 4)
            .ok_or_else(|| {
                bytes_error(
                    "bytes_read_u32_le() range is out of bounds",
                    "Ensure offset + 4 is within the buffer",
                )
            })?;
        Ok(Value::Integer(
            u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64,
        ))
    })
}

/// `bytes_read_u16_le(handle: integer, offset: integer): integer`
fn builtin_bytes_read_u16_le(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_read_u16_le", 2)?;
    let handle = expect_integer(args, 0, "bytes_read_u16_le")?;
    let offset = nonnegative_index(
        expect_integer(args, 1, "bytes_read_u16_le")?,
        "bytes_read_u16_le",
    )?;
    with_buffer(handle, |buffer| {
        let end = offset.checked_add(2).ok_or_else(|| {
            bytes_error(
                "bytes_read_u16_le() range is out of bounds",
                "Ensure offset + 2 is within the buffer",
            )
        })?;
        let bytes = buffer.get(offset..end).ok_or_else(|| {
            bytes_error(
                "bytes_read_u16_le() range is out of bounds",
                "Ensure offset + 2 is within the buffer",
            )
        })?;
        Ok(Value::Integer(
            u16::from_le_bytes([bytes[0], bytes[1]]) as i64
        ))
    })
}

fn byte_value(value: i64, name: &str) -> Result<u8, LegibleError> {
    u8::try_from(value).map_err(|_| {
        bytes_error(
            &format!("{name}() value must be between 0 and 255"),
            "Pass a byte value in the range 0 through 255",
        )
    })
}

/// `bytes_set(handle: integer, index: integer, value: integer): boolean`
fn builtin_bytes_set(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_set", 3)?;
    let handle = expect_integer(args, 0, "bytes_set")?;
    let index = nonnegative_index(expect_integer(args, 1, "bytes_set")?, "bytes_set")?;
    let value = byte_value(expect_integer(args, 2, "bytes_set")?, "bytes_set")?;
    with_buffer_mut(handle, |buffer| {
        let target = buffer.get_mut(index).ok_or_else(|| {
            bytes_error(
                "bytes_set() index is out of bounds",
                "Use an index smaller than bytes_length(handle)",
            )
        })?;
        *target = value;
        Ok(Value::Boolean(true))
    })
}

/// `bytes_fill(handle: integer, offset: integer, length: integer, value: integer): boolean`
fn builtin_bytes_fill(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_fill", 4)?;
    let handle = expect_integer(args, 0, "bytes_fill")?;
    let offset = nonnegative_index(expect_integer(args, 1, "bytes_fill")?, "bytes_fill")?;
    let length = nonnegative_index(expect_integer(args, 2, "bytes_fill")?, "bytes_fill")?;
    let value = byte_value(expect_integer(args, 3, "bytes_fill")?, "bytes_fill")?;
    with_buffer_mut(handle, |buffer| {
        let end = offset.checked_add(length).ok_or_else(|| {
            bytes_error(
                "bytes_fill() range is out of bounds",
                "Ensure offset + length is within the buffer",
            )
        })?;
        let target = buffer.get_mut(offset..end).ok_or_else(|| {
            bytes_error(
                "bytes_fill() range is out of bounds",
                "Ensure offset + length is within the buffer",
            )
        })?;
        target.fill(value);
        Ok(Value::Boolean(true))
    })
}

/// `bytes_write_bytes(dest: integer, offset: integer, src: integer): integer`
fn builtin_bytes_write_bytes(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_write_bytes", 3)?;
    let destination = expect_integer(args, 0, "bytes_write_bytes")?;
    let offset = nonnegative_index(
        expect_integer(args, 1, "bytes_write_bytes")?,
        "bytes_write_bytes",
    )?;
    let source = expect_integer(args, 2, "bytes_write_bytes")?;
    let source_bytes = with_buffer(source, |buffer| Ok(buffer.to_vec()))?;
    with_buffer_mut(destination, |buffer| {
        let end = offset.checked_add(source_bytes.len()).ok_or_else(|| {
            bytes_error(
                "bytes_write_bytes() range is out of bounds",
                "Ensure the source fits within the destination buffer",
            )
        })?;
        let target = buffer.get_mut(offset..end).ok_or_else(|| {
            bytes_error(
                "bytes_write_bytes() range is out of bounds",
                "Ensure the source fits within the destination buffer",
            )
        })?;
        target.copy_from_slice(&source_bytes);
        Ok(Value::Integer(source_bytes.len() as i64))
    })
}

/// `bytes_write_u32_le(handle: integer, offset: integer, value: integer): boolean`
fn builtin_bytes_write_u32_le(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_write_u32_le", 3)?;
    let handle = expect_integer(args, 0, "bytes_write_u32_le")?;
    let offset = nonnegative_index(
        expect_integer(args, 1, "bytes_write_u32_le")?,
        "bytes_write_u32_le",
    )?;
    let value = u32::try_from(expect_integer(args, 2, "bytes_write_u32_le")?).map_err(|_| {
        bytes_error(
            "bytes_write_u32_le() value must be between 0 and 4294967295",
            "Pass an unsigned 32-bit integer",
        )
    })?;
    with_buffer_mut(handle, |buffer| {
        let end = offset.checked_add(4).ok_or_else(|| {
            bytes_error(
                "bytes_write_u32_le() range is out of bounds",
                "Ensure offset + 4 is within the buffer",
            )
        })?;
        let target = buffer.get_mut(offset..end).ok_or_else(|| {
            bytes_error(
                "bytes_write_u32_le() range is out of bounds",
                "Ensure offset + 4 is within the buffer",
            )
        })?;
        target.copy_from_slice(&value.to_le_bytes());
        Ok(Value::Boolean(true))
    })
}

/// `bytes_index_of(handle: integer, needle: text, from: integer): integer`
fn builtin_bytes_index_of(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_index_of", 3)?;
    let handle = expect_integer(args, 0, "bytes_index_of")?;
    let needle = expect_text(args, 1, "bytes_index_of")?.as_bytes().to_vec();
    let from = nonnegative_index(expect_integer(args, 2, "bytes_index_of")?, "bytes_index_of")?;
    with_buffer(handle, |buffer| {
        if from > buffer.len() {
            return Ok(Value::Integer(-1));
        }
        if needle.is_empty() {
            return Ok(Value::Integer(from as i64));
        }
        let result = buffer[from..]
            .windows(needle.len())
            .position(|window| window == needle.as_slice())
            .map(|position| (from + position) as i64)
            .unwrap_or(-1);
        Ok(Value::Integer(result))
    })
}

/// `bytes_scan_words(handle, start, step, mask, value): a list of integer`
fn builtin_bytes_scan_words(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_scan_words", 5)?;
    let handle = expect_integer(args, 0, "bytes_scan_words")?;
    let start = nonnegative_index(
        expect_integer(args, 1, "bytes_scan_words")?,
        "bytes_scan_words",
    )?;
    let step_value = expect_integer(args, 2, "bytes_scan_words")?;
    if step_value <= 0 {
        return Err(bytes_error(
            "bytes_scan_words() step must be positive",
            "Pass a step greater than zero",
        ));
    }
    let step = step_value as usize;
    let mask = expect_integer(args, 3, "bytes_scan_words")?;
    let value = expect_integer(args, 4, "bytes_scan_words")?;
    with_buffer(handle, |buffer| {
        let mut matches = Vec::new();
        let mut offset = start;
        while offset.checked_add(4).is_some_and(|end| end <= buffer.len()) {
            let bytes = &buffer[offset..offset + 4];
            let word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as i64;
            if (word & mask) == value {
                matches.push(Value::Integer(offset as i64));
            }
            match offset.checked_add(step) {
                Some(next) => offset = next,
                None => break,
            }
        }
        Ok(Value::list(matches))
    })
}

/// `write_file_bytes(path: text, handle: integer): boolean`
fn builtin_write_file_bytes(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "write_file_bytes", 2)?;
    let path = expect_text(args, 0, "write_file_bytes")?.to_string();
    let handle = expect_integer(args, 1, "write_file_bytes")?;
    with_buffer(handle, |buffer| {
        std::fs::write(&path, buffer).map_err(|error| {
            bytes_error(
                &format!("Failed to write file '{path}': {error}"),
                "Check the file path is writable",
            )
        })
    })?;
    Ok(Value::Boolean(true))
}

/// `bytes_free(handle: integer): boolean`
fn builtin_bytes_free(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bytes_free", 1)?;
    let index = handle_index(expect_integer(args, 0, "bytes_free")?)?;
    let mut registry = buffers().lock().map_err(|_| {
        bytes_error(
            "Byte buffer registry is unavailable",
            "Try again in a new process",
        )
    })?;
    match registry.get_mut(index) {
        Some(buffer @ Some(_)) => {
            *buffer = None;
            Ok(Value::Boolean(true))
        }
        _ => Ok(Value::Boolean(false)),
    }
}

fn binary_bitwise(
    args: &[Value],
    name: &str,
    operation: impl FnOnce(i64, i64) -> i64,
) -> Result<Value, LegibleError> {
    require_arity(args, name, 2)?;
    Ok(Value::Integer(operation(
        expect_integer(args, 0, name)?,
        expect_integer(args, 1, name)?,
    )))
}

fn builtin_bit_and(args: &[Value]) -> Result<Value, LegibleError> {
    binary_bitwise(args, "bit_and", |a, b| a & b)
}
fn builtin_bit_or(args: &[Value]) -> Result<Value, LegibleError> {
    binary_bitwise(args, "bit_or", |a, b| a | b)
}
fn builtin_bit_xor(args: &[Value]) -> Result<Value, LegibleError> {
    binary_bitwise(args, "bit_xor", |a, b| a ^ b)
}

fn builtin_bit_not(args: &[Value]) -> Result<Value, LegibleError> {
    require_arity(args, "bit_not", 1)?;
    Ok(Value::Integer(!expect_integer(args, 0, "bit_not")?))
}

fn shift_amount(args: &[Value], name: &str) -> Result<(i64, u32), LegibleError> {
    require_arity(args, name, 2)?;
    let value = expect_integer(args, 0, name)?;
    let amount = expect_integer(args, 1, name)?;
    if !(0..=63).contains(&amount) {
        return Err(bytes_error(
            &format!("{name}() shift amount must be between 0 and 63"),
            "Pass a shift amount in the range 0 through 63",
        ));
    }
    Ok((value, amount as u32))
}

fn builtin_shift_left(args: &[Value]) -> Result<Value, LegibleError> {
    let (value, amount) = shift_amount(args, "shift_left")?;
    Ok(Value::Integer(value << amount))
}

fn builtin_shift_right(args: &[Value]) -> Result<Value, LegibleError> {
    let (value, amount) = shift_amount(args, "shift_right")?;
    Ok(Value::Integer(value >> amount))
}

fn builtin_shift_right_unsigned(args: &[Value]) -> Result<Value, LegibleError> {
    let (value, amount) = shift_amount(args, "shift_right_unsigned")?;
    Ok(Value::Integer(((value as u64) >> amount) as i64))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inflate(input: Vec<u8>, expected: i64) -> Result<Vec<u8>, LegibleError> {
        let source = store_buffer(input)?;
        let Value::Integer(source_handle) = source else {
            unreachable!("byte buffers use integer handles");
        };
        let output =
            builtin_bytes_inflate(&[Value::Integer(source_handle), Value::Integer(expected)])?;
        let Value::Integer(output_handle) = output else {
            unreachable!("byte buffers use integer handles");
        };
        with_buffer(output_handle, |buffer| Ok(buffer.to_vec()))
    }

    #[test]
    fn inflates_raw_deflate_stream() {
        let compressed = vec![0xcb, 0x4d, 0xcc, 0xcb, 0x4c, 0x4b, 0x2d, 0x2e, 0x01, 0x00];
        assert_eq!(inflate(compressed, 8).unwrap(), b"manifest");
    }

    #[test]
    fn inflates_raw_stored_deflate_block() {
        let literal = b"hello";
        let length = literal.len() as u16;
        let mut compressed = vec![0x01];
        compressed.extend_from_slice(&length.to_le_bytes());
        compressed.extend_from_slice(&(!length).to_le_bytes());
        compressed.extend_from_slice(literal);
        assert_eq!(inflate(compressed, literal.len() as i64).unwrap(), literal);
    }

    #[test]
    fn rejects_inflate_size_mismatch() {
        let compressed = vec![0xcb, 0x4d, 0xcc, 0xcb, 0x4c, 0x4b, 0x2d, 0x2e, 0x01, 0x00];
        assert!(inflate(compressed, 7).is_err());
    }

    #[test]
    fn rejects_truncated_or_garbage_inflate_stream() {
        assert!(inflate(vec![0xcb, 0x4d], 8).is_err());
        assert!(inflate(vec![0xff, 0xff, 0xff], 0).is_err());
    }
}
