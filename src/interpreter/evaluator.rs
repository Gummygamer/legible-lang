/// Tree-walking evaluator for the Clarity language.
///
/// Evaluates an AST by walking it recursively, using the environment
/// for variable bindings and the output writer for `print` calls.
use crate::errors::{ClarityError, ErrorCode, Severity, SourceLocation};
use crate::interpreter::environment::{Env, Environment};
use crate::interpreter::value::{Callable, Value};
use crate::parser::arena::Arena;
use crate::parser::ast::*;

/// Signal used to handle `return` statements as control flow.
enum EvalSignal {
    Value(Value),
    Return(Value),
}

/// Evaluate a parsed program, writing output to the given writer.
///
/// Looks for a `main()` function and invokes it. If no `main()` is found,
/// evaluates all top-level statements in order.
pub fn evaluate_program(
    arena: &Arena,
    root: NodeId,
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    // First pass: register all declarations
    if let NodeKind::Program { ref statements } = arena.get(root).kind.clone() {
        for &stmt_id in statements {
            register_declaration(arena, stmt_id, env)?;
        }

        // Look for main()
        let has_main = env.borrow().get("main").is_some();
        if has_main {
            let main_val = env.borrow().get("main").unwrap().0;
            if let Value::Function(callable) = main_val {
                return call_function(arena, &callable, &[], env, output);
            }
        }

        // No main — evaluate top-level statements
        let mut last = Value::None;
        for &stmt_id in statements {
            match eval_node(arena, stmt_id, env, output)? {
                EvalSignal::Value(v) => last = v,
                EvalSignal::Return(v) => return Ok(v),
            }
        }
        Ok(last)
    } else {
        Ok(Value::None)
    }
}

/// Register top-level declarations (functions, records, unions) in the environment
/// without evaluating their bodies.
fn register_declaration(arena: &Arena, node_id: NodeId, env: &Env) -> Result<(), ClarityError> {
    match &arena.get(node_id).kind.clone() {
        NodeKind::FunctionDecl {
            name,
            params,
            intent,
            requires,
            ensures,
            body,
            is_public: _,
            return_type: _,
        } => {
            let callable = Callable::UserDefined {
                name: name.clone(),
                params: params.clone(),
                intent: intent.clone(),
                requires: requires.clone(),
                ensures: ensures.clone(),
                body: body.clone(),
                closure_env: Env::clone(env),
            };
            env.borrow_mut()
                .define(name.clone(), Value::Function(callable), false);
            Ok(())
        }
        NodeKind::RecordDecl { .. } | NodeKind::UnionDecl { .. } | NodeKind::UseDecl { .. } => {
            // Records and unions are type-level; handled during construction
            Ok(())
        }
        _ => Ok(()),
    }
}

fn eval_node(
    arena: &Arena,
    node_id: NodeId,
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<EvalSignal, ClarityError> {
    let node = arena.get(node_id).kind.clone();
    match node {
        NodeKind::Program { statements } => {
            let mut last = Value::None;
            for stmt_id in statements {
                match eval_node(arena, stmt_id, env, output)? {
                    EvalSignal::Value(v) => last = v,
                    EvalSignal::Return(v) => return Ok(EvalSignal::Return(v)),
                }
            }
            Ok(EvalSignal::Value(last))
        }

        NodeKind::LetBinding {
            name,
            value,
            mutable,
            ..
        } => {
            let val = eval_expr(arena, value, env, output)?;
            env.borrow_mut().define(name, val, mutable);
            Ok(EvalSignal::Value(Value::None))
        }

        NodeKind::SetStatement { name, value } => {
            let val = eval_expr(arena, value, env, output)?;
            env.borrow_mut().set(&name, val)?;
            Ok(EvalSignal::Value(Value::None))
        }

        NodeKind::ForLoop {
            binding,
            iterable,
            body,
        } => {
            let iter_val = eval_expr(arena, iterable, env, output)?;
            if let Value::List(items) = iter_val {
                for item in items {
                    let loop_env = Environment::with_parent(env);
                    loop_env.borrow_mut().define(binding.clone(), item, false);
                    for &stmt_id in &body {
                        match eval_node(arena, stmt_id, &loop_env, output)? {
                            EvalSignal::Return(v) => return Ok(EvalSignal::Return(v)),
                            EvalSignal::Value(_) => {}
                        }
                    }
                }
            } else {
                return Err(runtime_error("For loop requires a list as iterable", "Ensure the iterable expression evaluates to a list"));
            }
            Ok(EvalSignal::Value(Value::None))
        }

        NodeKind::WhileLoop { condition, body } => {
            loop {
                let cond = eval_expr(arena, condition, env, output)?;
                if let Value::Boolean(true) = cond {
                    let loop_env = Environment::with_parent(env);
                    for &stmt_id in &body {
                        match eval_node(arena, stmt_id, &loop_env, output)? {
                            EvalSignal::Return(v) => return Ok(EvalSignal::Return(v)),
                            EvalSignal::Value(_) => {}
                        }
                    }
                } else {
                    break;
                }
            }
            Ok(EvalSignal::Value(Value::None))
        }

        NodeKind::ReturnExpr { value } => {
            let val = if let Some(v) = value {
                eval_expr(arena, v, env, output)?
            } else {
                Value::None
            };
            Ok(EvalSignal::Return(val))
        }

        NodeKind::ExprStatement { expr } => {
            let val = eval_expr(arena, expr, env, output)?;
            Ok(EvalSignal::Value(val))
        }

        NodeKind::FunctionDecl { name, .. } => {
            // Already registered in first pass; skip
            // But re-register in case this is inside a block
            register_declaration(arena, node_id, env)?;
            let _ = name;
            Ok(EvalSignal::Value(Value::None))
        }

        NodeKind::RecordDecl { .. } | NodeKind::UnionDecl { .. } | NodeKind::UseDecl { .. } => {
            Ok(EvalSignal::Value(Value::None))
        }

        _ => {
            // Treat as expression
            let val = eval_expr(arena, node_id, env, output)?;
            Ok(EvalSignal::Value(val))
        }
    }
}

fn eval_expr(
    arena: &Arena,
    node_id: NodeId,
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    let node = arena.get(node_id).kind.clone();
    match node {
        NodeKind::IntegerLit(n) => Ok(Value::Integer(n)),
        NodeKind::DecimalLit(n) => Ok(Value::Decimal(n)),
        NodeKind::TextLit(s) => Ok(Value::Text(s)),
        NodeKind::BooleanLit(b) => Ok(Value::Boolean(b)),
        NodeKind::NoneLit => Ok(Value::None),

        NodeKind::ListLit { elements } => {
            let mut items = Vec::new();
            for elem_id in elements {
                items.push(eval_expr(arena, elem_id, env, output)?);
            }
            Ok(Value::List(items))
        }

        NodeKind::MappingLit { entries } => {
            let mut mapping = Vec::new();
            for (key_id, val_id) in entries {
                let key = eval_expr(arena, key_id, env, output)?;
                let val = eval_expr(arena, val_id, env, output)?;
                mapping.push((key, val));
            }
            Ok(Value::Mapping(mapping))
        }

        NodeKind::Identifier(name) => {
            match env.borrow().get(&name) {
                Some((val, _)) => Ok(val),
                None => Err(ClarityError {
                    code: ErrorCode::UndefinedVariable,
                    severity: Severity::Error,
                    location: SourceLocation::unknown(),
                    message: format!("Undefined variable '{name}'"),
                    context: String::new(),
                    suggestion: format!("Define '{name}' with 'let' before using it"),
                }),
            }
        }

        NodeKind::FieldAccess { object, field } => {
            let obj = eval_expr(arena, object, env, output)?;
            match obj {
                Value::Record { fields, .. } => {
                    for (fname, fval) in &fields {
                        if fname == &field {
                            return Ok(fval.clone());
                        }
                    }
                    Err(runtime_error(
                        &format!("Record has no field '{field}'"),
                        "Check the record definition for available fields",
                    ))
                }
                Value::UnionVariant { fields, .. } => {
                    for (fname, fval) in &fields {
                        if fname == &field {
                            return Ok(fval.clone());
                        }
                    }
                    Err(runtime_error(
                        &format!("Union variant has no field '{field}'"),
                        "Check the union definition for available fields",
                    ))
                }
                _ => Err(runtime_error(
                    &format!("Cannot access field '{field}' on {obj}"),
                    "Field access is only supported on records and union variants",
                )),
            }
        }

        NodeKind::FunctionCall { callee, arguments } => {
            // Evaluate arguments
            let mut args = Vec::new();
            for arg_id in &arguments {
                args.push(eval_expr(arena, *arg_id, env, output)?);
            }

            // Check for print special case
            if let NodeKind::Identifier(ref name) = arena.get(callee).kind {
                if name == "print" {
                    if let Some(val) = args.first() {
                        writeln!(output, "{val}").map_err(|e| {
                            runtime_error(&format!("Write error: {e}"), "Check output stream")
                        })?;
                    }
                    return Ok(Value::None);
                }
                // Handle filter, map, reduce, sort_by, take, drop, find as builtins
                // that take lambdas (need arena access)
                match name.as_str() {
                    "filter" => return eval_filter(arena, &args, env, output),
                    "map" => return eval_map(arena, &args, env, output),
                    "reduce" => return eval_reduce(arena, &args, env, output),
                    "sort_by" => return eval_sort_by(arena, &args, env, output),
                    "take" => return eval_take(&args),
                    "drop" => return eval_drop(&args),
                    "find" => return eval_find(arena, &args, env, output),
                    _ => {}
                }
            }

            let callee_val = eval_expr(arena, callee, env, output)?;
            match callee_val {
                Value::Function(callable) => call_function(arena, &callable, &args, env, output),
                _ => Err(runtime_error(
                    "Attempted to call a non-function value",
                    "Ensure the callee is a function",
                )),
            }
        }

        NodeKind::Pipeline { left, right } => {
            let left_val = eval_expr(arena, left, env, output)?;
            // right should be a function call; inject left_val as first arg
            match &arena.get(right).kind.clone() {
                NodeKind::FunctionCall { callee, arguments } => {
                    let callee_clone = *callee;
                    let arguments_clone = arguments.clone();

                    // Check for special builtins that need arena access
                    if let NodeKind::Identifier(ref name) = arena.get(callee_clone).kind {
                        match name.as_str() {
                            "filter" | "map" | "reduce" | "sort_by" | "take" | "drop" | "find" => {
                                let mut args = vec![left_val];
                                for arg_id in &arguments_clone {
                                    args.push(eval_expr(arena, *arg_id, env, output)?);
                                }
                                return match name.as_str() {
                                    "filter" => eval_filter(arena, &args, env, output),
                                    "map" => eval_map(arena, &args, env, output),
                                    "reduce" => eval_reduce(arena, &args, env, output),
                                    "sort_by" => eval_sort_by(arena, &args, env, output),
                                    "take" => eval_take(&args),
                                    "drop" => eval_drop(&args),
                                    "find" => eval_find(arena, &args, env, output),
                                    _ => unreachable!(),
                                };
                            }
                            "print" => {
                                writeln!(output, "{left_val}").map_err(|e| {
                                    runtime_error(&format!("Write error: {e}"), "Check output stream")
                                })?;
                                return Ok(Value::None);
                            }
                            _ => {}
                        }
                    }

                    let callee_val = eval_expr(arena, callee_clone, env, output)?;
                    let mut args = vec![left_val];
                    for arg_id in &arguments_clone {
                        args.push(eval_expr(arena, *arg_id, env, output)?);
                    }
                    match callee_val {
                        Value::Function(callable) => {
                            call_function(arena, &callable, &args, env, output)
                        }
                        _ => Err(runtime_error(
                            "Pipeline target is not a function",
                            "Ensure the right side of |> is a function call",
                        )),
                    }
                }
                NodeKind::Identifier(name) => {
                    // Simple identifier on the right side: call it with left as arg
                    let name = name.clone();
                    // Check special builtins
                    match name.as_str() {
                        "print" => {
                            writeln!(output, "{left_val}").map_err(|e| {
                                runtime_error(&format!("Write error: {e}"), "Check output stream")
                            })?;
                            return Ok(Value::None);
                        }
                        _ => {}
                    }
                    let callee_val = eval_expr(arena, right, env, output)?;
                    match callee_val {
                        Value::Function(callable) => {
                            call_function(arena, &callable, &[left_val], env, output)
                        }
                        _ => Err(runtime_error(
                            "Pipeline target is not a function",
                            "Ensure the right side of |> is a function call",
                        )),
                    }
                }
                _ => Err(runtime_error(
                    "Right side of pipeline must be a function call",
                    "Use |> with a function call, e.g., x |> f(y)",
                )),
            }
        }

        NodeKind::BinaryOp { left, op, right } => {
            let lhs = eval_expr(arena, left, env, output)?;
            let rhs = eval_expr(arena, right, env, output)?;
            eval_binary_op(&lhs, &op, &rhs)
        }

        NodeKind::UnaryOp { op, operand } => {
            let val = eval_expr(arena, operand, env, output)?;
            match op {
                UnaryOperator::Negate => match val {
                    Value::Integer(n) => Ok(Value::Integer(-n)),
                    Value::Decimal(n) => Ok(Value::Decimal(-n)),
                    _ => Err(runtime_error("Cannot negate non-numeric value", "Use negation on numbers only")),
                },
                UnaryOperator::Not => match val {
                    Value::Boolean(b) => Ok(Value::Boolean(!b)),
                    _ => Err(runtime_error("Cannot apply 'not' to non-boolean", "Use 'not' on boolean values only")),
                },
            }
        }

        NodeKind::IfExpr {
            condition,
            then_branch,
            else_branch,
        } => {
            let cond = eval_expr(arena, condition, env, output)?;
            let branch = if is_truthy(&cond) {
                &then_branch
            } else if let Some(ref else_b) = else_branch {
                else_b
            } else {
                return Ok(Value::None);
            };
            let scope = Environment::with_parent(env);
            let mut last = Value::None;
            for &stmt_id in branch {
                match eval_node(arena, stmt_id, &scope, output)? {
                    EvalSignal::Return(v) => return Ok(v),
                    EvalSignal::Value(v) => last = v,
                }
            }
            Ok(last)
        }

        NodeKind::MatchExpr { subject, arms } => {
            let subject_val = eval_expr(arena, subject, env, output)?;
            for arm in &arms {
                if let Some(bindings) = match_pattern(arena, &arm.pattern, &subject_val, env, output)? {
                    let arm_env = Environment::with_parent(env);
                    for (name, val) in bindings {
                        arm_env.borrow_mut().define(name, val, false);
                    }
                    let mut last = Value::None;
                    for &stmt_id in &arm.body {
                        match eval_node(arena, stmt_id, &arm_env, output)? {
                            EvalSignal::Return(v) => return Ok(v),
                            EvalSignal::Value(v) => last = v,
                        }
                    }
                    return Ok(last);
                }
            }
            Err(runtime_error(
                "No matching arm in match expression",
                "Add an 'otherwise' arm to handle all cases",
            ))
        }

        NodeKind::RecordConstruct { type_name, fields } => {
            let mut field_values = Vec::new();
            for (name, val_id) in &fields {
                let val = eval_expr(arena, *val_id, env, output)?;
                field_values.push((name.clone(), val));
            }
            Ok(Value::Record {
                type_name,
                fields: field_values,
            })
        }

        NodeKind::RecordUpdate { base, updates } => {
            let base_val = eval_expr(arena, base, env, output)?;
            match base_val {
                Value::Record { type_name, fields } => {
                    let mut new_fields = fields;
                    for (name, val_id) in &updates {
                        let val = eval_expr(arena, *val_id, env, output)?;
                        let mut found = false;
                        for field in &mut new_fields {
                            if &field.0 == name {
                                field.1 = val.clone();
                                found = true;
                                break;
                            }
                        }
                        if !found {
                            new_fields.push((name.clone(), val));
                        }
                    }
                    Ok(Value::Record {
                        type_name,
                        fields: new_fields,
                    })
                }
                _ => Err(runtime_error(
                    "'with' can only be used on records",
                    "Use 'with' on a record value",
                )),
            }
        }

        NodeKind::UnionConstruct {
            type_name,
            variant_name,
            fields,
        } => {
            let mut field_values = Vec::new();
            for (name, val_id) in &fields {
                let val = eval_expr(arena, *val_id, env, output)?;
                field_values.push((name.clone(), val));
            }
            Ok(Value::UnionVariant {
                type_name,
                variant_name,
                fields: field_values,
            })
        }

        NodeKind::Lambda {
            params,
            body,
            ..
        } => Ok(Value::Function(Callable::Lambda {
            params,
            body,
            closure_env: Env::clone(env),
        })),

        NodeKind::InterpolatedText { parts } => {
            let mut result = String::new();
            for part in &parts {
                match part {
                    TextPart::Literal(s) => result.push_str(s),
                    TextPart::Interpolation(expr_id) => {
                        let val = eval_expr(arena, *expr_id, env, output)?;
                        result.push_str(&val.to_string());
                    }
                }
            }
            Ok(Value::Text(result))
        }

        NodeKind::OldExpr { inner } => {
            // `old(expr)` is evaluated at function entry time.
            // In the evaluator, we evaluate it normally — the contracts
            // system captures old values before the body runs.
            eval_expr(arena, inner, env, output)
        }

        _ => Err(runtime_error(
            &format!("Cannot evaluate node: {:?}", arena.get(node_id).kind),
            "This node type is not supported as an expression",
        )),
    }
}

fn call_function(
    arena: &Arena,
    callable: &Callable,
    args: &[Value],
    _caller_env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    match callable {
        Callable::UserDefined {
            params,
            requires,
            ensures,
            body,
            closure_env,
            ..
        } => {
            let func_env = Environment::with_parent(closure_env);

            // Bind parameters
            for (i, param) in params.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(Value::None);
                func_env.borrow_mut().define(param.name.clone(), val, false);
            }

            // Evaluate requires contracts
            for &req_id in requires {
                let result = eval_expr(arena, req_id, &func_env, output)?;
                if let Value::Boolean(false) = result {
                    return Err(ClarityError {
                        code: ErrorCode::ContractRequires,
                        severity: Severity::Error,
                        location: SourceLocation::unknown(),
                        message: "Precondition (requires) violated".to_string(),
                        context: String::new(),
                        suggestion: "Ensure the function is called with arguments that satisfy its requires clause".to_string(),
                    });
                }
            }

            // Capture old values for ensures (clone env state at entry)
            let old_env = func_env.clone();

            // Evaluate body
            let mut result = Value::None;
            for &stmt_id in body {
                match eval_node(arena, stmt_id, &func_env, output)? {
                    EvalSignal::Return(v) => {
                        result = v;
                        break;
                    }
                    EvalSignal::Value(v) => result = v,
                }
            }

            // Evaluate ensures contracts
            if !ensures.is_empty() {
                let ensures_env = Environment::with_parent(&old_env);
                ensures_env
                    .borrow_mut()
                    .define("result".to_string(), result.clone(), false);
                for &ens_id in ensures {
                    let check = eval_expr(arena, ens_id, &ensures_env, output)?;
                    if let Value::Boolean(false) = check {
                        return Err(ClarityError {
                            code: ErrorCode::ContractEnsures,
                            severity: Severity::Error,
                            location: SourceLocation::unknown(),
                            message: "Postcondition (ensures) violated".to_string(),
                            context: String::new(),
                            suggestion: "The function's return value does not satisfy its ensures clause".to_string(),
                        });
                    }
                }
            }

            Ok(result)
        }

        Callable::Lambda {
            params,
            body,
            closure_env,
        } => {
            let lambda_env = Environment::with_parent(closure_env);
            for (i, param) in params.iter().enumerate() {
                let val = args.get(i).cloned().unwrap_or(Value::None);
                lambda_env
                    .borrow_mut()
                    .define(param.name.clone(), val, false);
            }
            eval_expr(arena, *body, &lambda_env, output)
        }

        Callable::Builtin { func, .. } => func(args),
    }
}

fn eval_binary_op(lhs: &Value, op: &BinaryOperator, rhs: &Value) -> Result<Value, ClarityError> {
    match op {
        BinaryOperator::Add => match (lhs, rhs) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a + b)),
            (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Decimal(a + b)),
            (Value::Integer(a), Value::Decimal(b)) => Ok(Value::Decimal(*a as f64 + b)),
            (Value::Decimal(a), Value::Integer(b)) => Ok(Value::Decimal(a + *b as f64)),
            _ => Err(runtime_error(
                &format!("Cannot add {lhs} and {rhs}"),
                "Use + with numbers only. For string concatenation, use ++",
            )),
        },
        BinaryOperator::Sub => match (lhs, rhs) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a - b)),
            (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Decimal(a - b)),
            (Value::Integer(a), Value::Decimal(b)) => Ok(Value::Decimal(*a as f64 - b)),
            (Value::Decimal(a), Value::Integer(b)) => Ok(Value::Decimal(a - *b as f64)),
            _ => Err(runtime_error("Cannot subtract non-numeric values", "Use - with numbers only")),
        },
        BinaryOperator::Mul => match (lhs, rhs) {
            (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a * b)),
            (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Decimal(a * b)),
            (Value::Integer(a), Value::Decimal(b)) => Ok(Value::Decimal(*a as f64 * b)),
            (Value::Decimal(a), Value::Integer(b)) => Ok(Value::Decimal(a * *b as f64)),
            _ => Err(runtime_error("Cannot multiply non-numeric values", "Use * with numbers only")),
        },
        BinaryOperator::Div => {
            // Check for zero divisor first
            let is_zero = matches!(rhs, Value::Integer(0)) || matches!(rhs, Value::Decimal(n) if *n == 0.0);
            if is_zero {
                return Err(ClarityError {
                    code: ErrorCode::DivisionByZero,
                    severity: Severity::Error,
                    location: SourceLocation::unknown(),
                    message: "Division by zero".to_string(),
                    context: String::new(),
                    suggestion: "Check that the divisor is not zero before dividing".to_string(),
                });
            }
            match (lhs, rhs) {
                (Value::Integer(a), Value::Integer(b)) => Ok(Value::Integer(a / b)),
                (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Decimal(a / b)),
                (Value::Integer(a), Value::Decimal(b)) => Ok(Value::Decimal(*a as f64 / b)),
                (Value::Decimal(a), Value::Integer(b)) => Ok(Value::Decimal(a / *b as f64)),
                _ => Err(runtime_error("Cannot divide non-numeric values", "Use / with numbers only")),
            }
        }
        BinaryOperator::Mod => match (lhs, rhs) {
            (Value::Integer(a), Value::Integer(b)) => {
                if *b == 0 {
                    Err(ClarityError {
                        code: ErrorCode::DivisionByZero,
                        severity: Severity::Error,
                        location: SourceLocation::unknown(),
                        message: "Modulo by zero".to_string(),
                        context: String::new(),
                        suggestion: "Check that the divisor is not zero".to_string(),
                    })
                } else {
                    Ok(Value::Integer(a % b))
                }
            }
            _ => Err(runtime_error("Modulo requires integer operands", "Use % with integers only")),
        },
        BinaryOperator::Concat => match (lhs, rhs) {
            (Value::Text(a), Value::Text(b)) => Ok(Value::Text(format!("{a}{b}"))),
            _ => Err(runtime_error("++ requires text operands", "Use ++ with text values only")),
        },
        BinaryOperator::Eq => Ok(Value::Boolean(lhs == rhs)),
        BinaryOperator::NotEq => Ok(Value::Boolean(lhs != rhs)),
        BinaryOperator::Gt => eval_comparison(lhs, rhs, |a, b| a > b, |a, b| a > b),
        BinaryOperator::Lt => eval_comparison(lhs, rhs, |a, b| a < b, |a, b| a < b),
        BinaryOperator::GtEq => eval_comparison(lhs, rhs, |a, b| a >= b, |a, b| a >= b),
        BinaryOperator::LtEq => eval_comparison(lhs, rhs, |a, b| a <= b, |a, b| a <= b),
        BinaryOperator::And => match (lhs, rhs) {
            (Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(*a && *b)),
            _ => Err(runtime_error("'and' requires boolean operands", "Use 'and' with boolean values")),
        },
        BinaryOperator::Or => match (lhs, rhs) {
            (Value::Boolean(a), Value::Boolean(b)) => Ok(Value::Boolean(*a || *b)),
            _ => Err(runtime_error("'or' requires boolean operands", "Use 'or' with boolean values")),
        },
    }
}

fn eval_comparison(
    lhs: &Value,
    rhs: &Value,
    int_cmp: fn(i64, i64) -> bool,
    float_cmp: fn(f64, f64) -> bool,
) -> Result<Value, ClarityError> {
    match (lhs, rhs) {
        (Value::Integer(a), Value::Integer(b)) => Ok(Value::Boolean(int_cmp(*a, *b))),
        (Value::Decimal(a), Value::Decimal(b)) => Ok(Value::Boolean(float_cmp(*a, *b))),
        (Value::Integer(a), Value::Decimal(b)) => Ok(Value::Boolean(float_cmp(*a as f64, *b))),
        (Value::Decimal(a), Value::Integer(b)) => Ok(Value::Boolean(float_cmp(*a, *b as f64))),
        (Value::Text(a), Value::Text(b)) => Ok(Value::Boolean(int_cmp(a.cmp(b) as i64, 0))),
        _ => Err(runtime_error(
            "Cannot compare these values",
            "Comparison requires numeric or text operands",
        )),
    }
}

fn match_pattern(
    arena: &Arena,
    pattern: &Pattern,
    value: &Value,
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Option<Vec<(String, Value)>>, ClarityError> {
    match pattern {
        Pattern::Otherwise => Ok(Some(Vec::new())),
        Pattern::Literal(node_id) => {
            let pattern_val = eval_expr(arena, *node_id, env, output)?;
            if &pattern_val == value {
                Ok(Some(Vec::new()))
            } else {
                Ok(None)
            }
        }
        Pattern::Variant { name, bindings } => {
            match value {
                Value::UnionVariant {
                    variant_name,
                    fields,
                    ..
                } => {
                    if variant_name == name {
                        let mut bound = Vec::new();
                        for (i, binding) in bindings.iter().enumerate() {
                            if let Some((_, val)) = fields.get(i) {
                                bound.push((binding.clone(), val.clone()));
                            }
                        }
                        Ok(Some(bound))
                    } else {
                        Ok(None)
                    }
                }
                // Also handle matching against literal identifiers (e.g. enum-like strings)
                _ => Ok(None),
            }
        }
    }
}

fn is_truthy(val: &Value) -> bool {
    match val {
        Value::Boolean(b) => *b,
        Value::None => false,
        Value::Integer(0) => false,
        Value::Text(s) if s.is_empty() => false,
        Value::List(items) if items.is_empty() => false,
        _ => true,
    }
}

// ─── Higher-order builtin helpers ───────────────────────

fn eval_filter(
    arena: &Arena,
    args: &[Value],
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    if args.len() != 2 {
        return Err(runtime_error("filter() expects 2 arguments", "Usage: filter(list, predicate)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Function(pred)) => {
            let mut result = Vec::new();
            for item in items {
                let val = call_function(arena, pred, &[item.clone()], env, output)?;
                if let Value::Boolean(true) = val {
                    result.push(item.clone());
                }
            }
            Ok(Value::List(result))
        }
        _ => Err(runtime_error("filter() expects a list and a function", "Pass a list and predicate function")),
    }
}

fn eval_map(
    arena: &Arena,
    args: &[Value],
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    if args.len() != 2 {
        return Err(runtime_error("map() expects 2 arguments", "Usage: map(list, transform)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Function(transform)) => {
            let mut result = Vec::new();
            for item in items {
                let val = call_function(arena, transform, &[item.clone()], env, output)?;
                result.push(val);
            }
            Ok(Value::List(result))
        }
        _ => Err(runtime_error("map() expects a list and a function", "Pass a list and transform function")),
    }
}

fn eval_reduce(
    arena: &Arena,
    args: &[Value],
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    if args.len() != 3 {
        return Err(runtime_error("reduce() expects 3 arguments", "Usage: reduce(list, initial, combine)"));
    }
    match (&args[0], &args[2]) {
        (Value::List(items), Value::Function(combine)) => {
            let mut acc = args[1].clone();
            for item in items {
                acc = call_function(arena, combine, &[acc, item.clone()], env, output)?;
            }
            Ok(acc)
        }
        _ => Err(runtime_error("reduce() expects a list and a combine function", "Pass a list, initial value, and combine function")),
    }
}

fn eval_sort_by(
    arena: &Arena,
    args: &[Value],
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    if args.len() != 2 {
        return Err(runtime_error("sort_by() expects 2 arguments", "Usage: sort_by(list, key_fn)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Function(key_fn)) => {
            // Compute keys for each item
            let mut keyed: Vec<(Value, Value)> = Vec::new();
            for item in items {
                let key = call_function(arena, key_fn, &[item.clone()], env, output)?;
                keyed.push((key, item.clone()));
            }
            // Sort by key
            keyed.sort_by(|(ka, _), (kb, _)| {
                match (ka, kb) {
                    (Value::Integer(a), Value::Integer(b)) => a.cmp(b),
                    (Value::Decimal(a), Value::Decimal(b)) => a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal),
                    (Value::Text(a), Value::Text(b)) => a.cmp(b),
                    _ => std::cmp::Ordering::Equal,
                }
            });
            Ok(Value::List(keyed.into_iter().map(|(_, v)| v).collect()))
        }
        _ => Err(runtime_error("sort_by() expects a list and a key function", "Pass a list and key function")),
    }
}

fn eval_take(args: &[Value]) -> Result<Value, ClarityError> {
    if args.len() != 2 {
        return Err(runtime_error("take() expects 2 arguments", "Usage: take(list, count)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Integer(n)) => {
            let n = *n as usize;
            Ok(Value::List(items.iter().take(n).cloned().collect()))
        }
        _ => Err(runtime_error("take() expects a list and integer", "Pass a list and count")),
    }
}

fn eval_drop(args: &[Value]) -> Result<Value, ClarityError> {
    if args.len() != 2 {
        return Err(runtime_error("drop() expects 2 arguments", "Usage: drop(list, count)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Integer(n)) => {
            let n = *n as usize;
            Ok(Value::List(items.iter().skip(n).cloned().collect()))
        }
        _ => Err(runtime_error("drop() expects a list and integer", "Pass a list and count")),
    }
}

fn eval_find(
    arena: &Arena,
    args: &[Value],
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, ClarityError> {
    if args.len() != 2 {
        return Err(runtime_error("find() expects 2 arguments", "Usage: find(list, predicate)"));
    }
    match (&args[0], &args[1]) {
        (Value::List(items), Value::Function(pred)) => {
            for item in items {
                let val = call_function(arena, pred, &[item.clone()], env, output)?;
                if let Value::Boolean(true) = val {
                    return Ok(item.clone());
                }
            }
            Ok(Value::None)
        }
        _ => Err(runtime_error("find() expects a list and a function", "Pass a list and predicate")),
    }
}

fn runtime_error(message: &str, suggestion: &str) -> ClarityError {
    ClarityError {
        code: ErrorCode::Syntax,
        severity: Severity::Error,
        location: SourceLocation::unknown(),
        message: message.to_string(),
        context: String::new(),
        suggestion: suggestion.to_string(),
    }
}
