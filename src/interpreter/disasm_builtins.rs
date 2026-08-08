/// Capstone-backed ARM disassembly built-in functions.
use capstone::arch;
use capstone::prelude::*;

use crate::errors::{ErrorCode, LegibleError, Severity, SourceLocation};
use crate::interpreter::bytes_builtins::with_buffer;
use crate::interpreter::environment::Env;
use crate::interpreter::value::{Callable, Value};

fn disasm_error(message: &str, suggestion: &str) -> LegibleError {
    LegibleError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}

/// Register Capstone-backed ARM disassembly built-ins in the given environment.
pub fn register_disasm_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("disasm_arm64", builtin_disasm_arm64),
        ("disasm_arm32", builtin_disasm_arm32),
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
        Err(disasm_error(
            &format!(
                "{name}() expects {count} argument{}",
                if count == 1 { "" } else { "s" }
            ),
            &format!("Usage: {name}(...)"),
        ))
    }
}

fn expect_integer(args: &[Value], index: usize, name: &str) -> Result<i64, LegibleError> {
    match args.get(index) {
        Some(Value::Integer(value)) => Ok(*value),
        _ => Err(disasm_error(
            &format!("{name}() expects an integer argument"),
            "Pass integer arguments",
        )),
    }
}

fn nonnegative_index(value: i64, name: &str) -> Result<usize, LegibleError> {
    if value < 0 {
        return Err(disasm_error(
            &format!("{name}() index values must be non-negative"),
            "Pass a non-negative byte offset or length",
        ));
    }
    Ok(value as usize)
}

fn disasm_arguments(args: &[Value], name: &str) -> Result<(i64, usize, usize, u64), LegibleError> {
    require_arity(args, name, 4)?;
    let handle = expect_integer(args, 0, name)?;
    let offset = nonnegative_index(expect_integer(args, 1, name)?, name)?;
    let length = nonnegative_index(expect_integer(args, 2, name)?, name)?;
    let address = expect_integer(args, 3, name)?;
    if address < 0 {
        return Err(disasm_error(
            &format!("{name}() index values must be non-negative"),
            "Pass a non-negative instruction address",
        ));
    }
    Ok((handle, offset, length, address as u64))
}

fn decoded_value(instructions: capstone::Instructions<'_>) -> Value {
    Value::List(
        instructions
            .iter()
            .map(|instruction| {
                Value::Mapping(vec![
                    (
                        Value::Text("address".to_string()),
                        Value::Integer(instruction.address() as i64),
                    ),
                    (
                        Value::Text("mnemonic".to_string()),
                        Value::Text(instruction.mnemonic().unwrap_or("").to_string()),
                    ),
                    (
                        Value::Text("op_str".to_string()),
                        Value::Text(instruction.op_str().unwrap_or("").to_string()),
                    ),
                ])
            })
            .collect(),
    )
}

/// `disasm_arm64(handle, offset, length, address): a list of mappings`
fn builtin_disasm_arm64(args: &[Value]) -> Result<Value, LegibleError> {
    let (handle, offset, length, address) = disasm_arguments(args, "disasm_arm64")?;
    with_buffer(handle, |buffer| {
        let slice = buffer
            .get(offset..buffer.len().min(offset.saturating_add(length)))
            .unwrap_or(&[]);
        let capstone = Capstone::new()
            .arm64()
            .mode(arch::arm64::ArchMode::Arm)
            .endian(capstone::Endian::Little)
            .build()
            .map_err(|error| {
                disasm_error(
                    &format!("Failed to initialize ARM64 disassembler: {error}"),
                    "Try again in a new process",
                )
            })?;
        let instructions = capstone.disasm_all(slice, address).map_err(|error| {
            disasm_error(
                &format!("Failed to disassemble ARM64 bytes: {error}"),
                "Use a valid byte buffer",
            )
        })?;
        Ok(decoded_value(instructions))
    })
}

/// `disasm_arm32(handle, offset, length, address): a list of mappings`
fn builtin_disasm_arm32(args: &[Value]) -> Result<Value, LegibleError> {
    let (handle, offset, length, address) = disasm_arguments(args, "disasm_arm32")?;
    with_buffer(handle, |buffer| {
        let slice = buffer
            .get(offset..buffer.len().min(offset.saturating_add(length)))
            .unwrap_or(&[]);
        let capstone = Capstone::new()
            .arm()
            .mode(arch::arm::ArchMode::Arm)
            .build()
            .map_err(|error| {
                disasm_error(
                    &format!("Failed to initialize ARM32 disassembler: {error}"),
                    "Try again in a new process",
                )
            })?;
        let instructions = capstone.disasm_all(slice, address).map_err(|error| {
            disasm_error(
                &format!("Failed to disassemble ARM32 bytes: {error}"),
                "Use a valid byte buffer",
            )
        })?;
        Ok(decoded_value(instructions))
    })
}
