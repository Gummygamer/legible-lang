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

fn operand_value(
    operand_type: &str,
    reg: String,
    imm: i64,
    mem_base: String,
    mem_index: String,
    mem_disp: i64,
) -> Value {
    Value::mapping(vec![
        (Value::Text("type".to_string()), Value::Text(operand_type.to_string())),
        (Value::Text("reg".to_string()), Value::Text(reg)),
        (Value::Text("imm".to_string()), Value::Integer(imm)),
        (Value::Text("mem_base".to_string()), Value::Text(mem_base)),
        (Value::Text("mem_index".to_string()), Value::Text(mem_index)),
        (Value::Text("mem_disp".to_string()), Value::Integer(mem_disp)),
    ])
}

fn register_name(capstone: &Capstone, register: RegId) -> String {
    capstone.reg_name(register).unwrap_or_default()
}

fn decoded_arm64_value(
    capstone: &Capstone,
    instructions: capstone::Instructions<'_>,
) -> Result<Value, LegibleError> {
    let mut decoded = Vec::new();
    for instruction in instructions.iter() {
        let detail = capstone.insn_detail(instruction).map_err(|error| {
            disasm_error(
                &format!("Failed to read ARM64 instruction detail: {error}"),
                "Use a valid byte buffer",
            )
        })?;
        let operands = detail
            .arch_detail()
            .arm64()
            .expect("ARM64 disassembler returned non-ARM64 detail")
            .operands()
            .map(|operand| match operand.op_type {
                arch::arm64::Arm64OperandType::Reg(register) => operand_value(
                    "reg",
                    register_name(capstone, register),
                    0,
                    String::new(),
                    String::new(),
                    0,
                ),
                arch::arm64::Arm64OperandType::Imm(value) => operand_value(
                    "imm",
                    String::new(),
                    value,
                    String::new(),
                    String::new(),
                    0,
                ),
                arch::arm64::Arm64OperandType::Mem(memory) => operand_value(
                    "mem",
                    String::new(),
                    0,
                    register_name(capstone, memory.base()),
                    register_name(capstone, memory.index()),
                    memory.disp() as i64,
                ),
                _ => operand_value("other", String::new(), 0, String::new(), String::new(), 0),
            })
            .collect();
        decoded.push(Value::mapping(vec![
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
            (Value::Text("operands".to_string()), Value::list(operands)),
        ]));
    }
    Ok(Value::list(decoded))
}

fn decoded_arm32_value(
    capstone: &Capstone,
    instructions: capstone::Instructions<'_>,
) -> Result<Value, LegibleError> {
    let mut decoded = Vec::new();
    for instruction in instructions.iter() {
        let detail = capstone.insn_detail(instruction).map_err(|error| {
            disasm_error(
                &format!("Failed to read ARM32 instruction detail: {error}"),
                "Use a valid byte buffer",
            )
        })?;
        let operands = detail
            .arch_detail()
            .arm()
            .expect("ARM32 disassembler returned non-ARM detail")
            .operands()
            .map(|operand| match operand.op_type {
                arch::arm::ArmOperandType::Reg(register) => operand_value(
                    "reg",
                    register_name(capstone, register),
                    0,
                    String::new(),
                    String::new(),
                    0,
                ),
                arch::arm::ArmOperandType::Imm(value) => operand_value(
                    "imm",
                    String::new(),
                    value as i64,
                    String::new(),
                    String::new(),
                    0,
                ),
                arch::arm::ArmOperandType::Mem(memory) => operand_value(
                    "mem",
                    String::new(),
                    0,
                    register_name(capstone, memory.base()),
                    register_name(capstone, memory.index()),
                    memory.disp() as i64,
                ),
                _ => operand_value("other", String::new(), 0, String::new(), String::new(), 0),
            })
            .collect();
        decoded.push(Value::mapping(vec![
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
            (Value::Text("operands".to_string()), Value::list(operands)),
        ]));
    }
    Ok(Value::list(decoded))
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
            .detail(true)
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
        decoded_arm64_value(&capstone, instructions)
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
            .detail(true)
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
        decoded_arm32_value(&capstone, instructions)
    })
}
