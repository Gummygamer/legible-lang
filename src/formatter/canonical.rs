/// Canonical formatter for Clarity source code.
///
/// Rewrites valid `.cl` files into the one canonical form.
/// The formatter is idempotent: `fmt(fmt(code)) == fmt(code)`.
use crate::parser::arena::Arena;
use crate::parser::ast::*;

/// Format a Clarity AST back to source code in canonical form.
pub fn format_source(arena: &Arena, root: NodeId) -> String {
    let mut formatter = Formatter::new(arena);
    formatter.format_node(root, 0);
    let mut result = formatter.output;
    // Ensure trailing newline
    if !result.ends_with('\n') {
        result.push('\n');
    }
    result
}

struct Formatter<'a> {
    arena: &'a Arena,
    output: String,
}

impl<'a> Formatter<'a> {
    fn new(arena: &'a Arena) -> Self {
        Self {
            arena,
            output: String::new(),
        }
    }

    fn format_node(&mut self, node_id: NodeId, indent: usize) {
        match self.arena.get(node_id).kind.clone() {
            NodeKind::Program { statements } => {
                let mut first = true;
                for (i, &stmt_id) in statements.iter().enumerate() {
                    let is_decl = matches!(
                        self.arena.get(stmt_id).kind,
                        NodeKind::FunctionDecl { .. }
                            | NodeKind::RecordDecl { .. }
                            | NodeKind::UnionDecl { .. }
                    );
                    if !first && is_decl {
                        self.output.push('\n');
                    }
                    if !first && !is_decl {
                        let prev_is_decl = i > 0 && matches!(
                            self.arena.get(statements[i - 1]).kind,
                            NodeKind::FunctionDecl { .. }
                                | NodeKind::RecordDecl { .. }
                                | NodeKind::UnionDecl { .. }
                        );
                        if prev_is_decl {
                            self.output.push('\n');
                        }
                    }
                    self.format_node(stmt_id, indent);
                    self.output.push('\n');
                    first = false;
                }
            }

            NodeKind::FunctionDecl {
                name,
                params,
                return_type,
                intent,
                requires,
                ensures,
                body,
                is_public,
            } => {
                self.write_indent(indent);
                if is_public {
                    self.output.push_str("public ");
                }
                self.output.push_str("function ");
                self.output.push_str(&name);
                self.output.push('(');
                self.format_params(&params);
                self.output.push_str("): ");
                self.format_type(&return_type);
                self.output.push('\n');

                self.write_indent(indent + 1);
                self.output.push_str("intent: ");
                self.output.push_str(&intent);
                self.output.push('\n');

                if !requires.is_empty() {
                    self.write_indent(indent + 1);
                    self.output.push_str("requires: ");
                    for (i, &req) in requires.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.format_expr(req);
                    }
                    self.output.push('\n');
                }

                if !ensures.is_empty() {
                    self.write_indent(indent + 1);
                    self.output.push_str("ensures: ");
                    for (i, &ens) in ensures.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.format_expr(ens);
                    }
                    self.output.push('\n');
                }

                for &stmt_id in &body {
                    self.format_node(stmt_id, indent + 1);
                    self.output.push('\n');
                }
                self.write_indent(indent);
                self.output.push_str("end");
            }

            NodeKind::RecordDecl { name, fields } => {
                self.write_indent(indent);
                self.output.push_str("record ");
                self.output.push_str(&name);
                self.output.push('\n');
                for field in &fields {
                    self.write_indent(indent + 1);
                    self.output.push_str(&field.name);
                    self.output.push_str(": ");
                    self.format_type(&field.field_type);
                    self.output.push('\n');
                }
                self.write_indent(indent);
                self.output.push_str("end");
            }

            NodeKind::UnionDecl { name, variants } => {
                self.write_indent(indent);
                self.output.push_str("union ");
                self.output.push_str(&name);
                self.output.push('\n');
                for variant in &variants {
                    self.write_indent(indent + 1);
                    self.output.push_str(&variant.name);
                    if !variant.fields.is_empty() {
                        self.output.push_str(" { ");
                        for (i, field) in variant.fields.iter().enumerate() {
                            if i > 0 {
                                self.output.push_str(", ");
                            }
                            self.output.push_str(&field.name);
                            self.output.push_str(": ");
                            self.format_type(&field.field_type);
                        }
                        self.output.push_str(" }");
                    }
                    self.output.push('\n');
                }
                self.write_indent(indent);
                self.output.push_str("end");
            }

            NodeKind::UseDecl { module_name } => {
                self.write_indent(indent);
                self.output.push_str("use ");
                self.output.push_str(&module_name);
            }

            NodeKind::LetBinding {
                name,
                declared_type,
                value,
                mutable,
            } => {
                self.write_indent(indent);
                if mutable {
                    self.output.push_str("mutable ");
                } else {
                    self.output.push_str("let ");
                }
                self.output.push_str(&name);
                self.output.push_str(": ");
                self.format_type(&declared_type);
                self.output.push_str(" = ");
                self.format_expr(value);
            }

            NodeKind::SetStatement { name, value } => {
                self.write_indent(indent);
                self.output.push_str("set ");
                self.output.push_str(&name);
                self.output.push_str(" = ");
                self.format_expr(value);
            }

            NodeKind::ForLoop {
                binding,
                iterable,
                body,
            } => {
                self.write_indent(indent);
                self.output.push_str("for ");
                self.output.push_str(&binding);
                self.output.push_str(" in ");
                self.format_expr(iterable);
                self.output.push_str(" do\n");
                for &stmt_id in &body {
                    self.format_node(stmt_id, indent + 1);
                    self.output.push('\n');
                }
                self.write_indent(indent);
                self.output.push_str("end");
            }

            NodeKind::WhileLoop { condition, body } => {
                self.write_indent(indent);
                self.output.push_str("while ");
                self.format_expr(condition);
                self.output.push_str(" do\n");
                for &stmt_id in &body {
                    self.format_node(stmt_id, indent + 1);
                    self.output.push('\n');
                }
                self.write_indent(indent);
                self.output.push_str("end");
            }

            NodeKind::ReturnExpr { value } => {
                self.write_indent(indent);
                self.output.push_str("return");
                if let Some(val) = value {
                    self.output.push(' ');
                    self.format_expr(val);
                }
            }

            NodeKind::ExprStatement { expr } => {
                self.write_indent(indent);
                self.format_expr(expr);
            }

            _ => {
                self.write_indent(indent);
                self.format_expr(node_id);
            }
        }
    }

    fn format_expr(&mut self, node_id: NodeId) {
        match self.arena.get(node_id).kind.clone() {
            NodeKind::IntegerLit(n) => {
                self.output.push_str(&n.to_string());
            }
            NodeKind::DecimalLit(n) => {
                self.output.push_str(&n.to_string());
            }
            NodeKind::TextLit(s) => {
                self.output.push('"');
                self.output.push_str(&s.replace('\\', "\\\\").replace('"', "\\\""));
                self.output.push('"');
            }
            NodeKind::BooleanLit(b) => {
                self.output.push_str(if b { "true" } else { "false" });
            }
            NodeKind::NoneLit => {
                self.output.push_str("none");
            }
            NodeKind::Identifier(name) => {
                self.output.push_str(&name);
            }
            NodeKind::ListLit { elements } => {
                self.output.push('[');
                for (i, &elem) in elements.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(elem);
                }
                self.output.push(']');
            }
            NodeKind::MappingLit { entries } => {
                self.output.push('{');
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(*k);
                    self.output.push_str(": ");
                    self.format_expr(*v);
                }
                self.output.push('}');
            }
            NodeKind::BinaryOp { left, op, right } => {
                self.format_expr(left);
                let op_str = match op {
                    BinaryOperator::Add => " + ",
                    BinaryOperator::Sub => " - ",
                    BinaryOperator::Mul => " * ",
                    BinaryOperator::Div => " / ",
                    BinaryOperator::Mod => " % ",
                    BinaryOperator::Concat => " ++ ",
                    BinaryOperator::Eq => " == ",
                    BinaryOperator::NotEq => " != ",
                    BinaryOperator::Gt => " > ",
                    BinaryOperator::Lt => " < ",
                    BinaryOperator::GtEq => " >= ",
                    BinaryOperator::LtEq => " <= ",
                    BinaryOperator::And => " and ",
                    BinaryOperator::Or => " or ",
                };
                self.output.push_str(op_str);
                self.format_expr(right);
            }
            NodeKind::UnaryOp { op, operand } => {
                match op {
                    UnaryOperator::Negate => self.output.push('-'),
                    UnaryOperator::Not => self.output.push_str("not "),
                }
                self.format_expr(operand);
            }
            NodeKind::FunctionCall { callee, arguments } => {
                self.format_expr(callee);
                self.output.push('(');
                for (i, &arg) in arguments.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_expr(arg);
                }
                self.output.push(')');
            }
            NodeKind::FieldAccess { object, field } => {
                self.format_expr(object);
                self.output.push('.');
                self.output.push_str(&field);
            }
            NodeKind::Pipeline { left, right } => {
                self.format_expr(left);
                self.output.push_str("\n");
                // Find current indent level (count leading spaces of last line)
                let indent = self.current_indent() + 1;
                self.write_indent(indent);
                self.output.push_str("|> ");
                self.format_expr(right);
            }
            NodeKind::Lambda { params, return_type, body } => {
                self.output.push_str("fn(");
                self.format_params(&params);
                self.output.push_str("): ");
                self.format_type(&return_type);
                self.output.push_str(" => ");
                self.format_expr(body);
            }
            NodeKind::IfExpr {
                condition,
                then_branch,
                else_branch,
            } => {
                self.output.push_str("if ");
                self.format_expr(condition);
                self.output.push_str(" then\n");
                let indent = self.current_indent() + 1;
                for &stmt_id in &then_branch {
                    self.format_node(stmt_id, indent);
                    self.output.push('\n');
                }
                if let Some(else_b) = else_branch {
                    if else_b.len() == 1 {
                        if let NodeKind::IfExpr { .. } = self.arena.get(else_b[0]).kind {
                            self.write_indent(indent - 1);
                            self.output.push_str("else ");
                            self.format_expr(else_b[0]);
                            return;
                        }
                    }
                    self.write_indent(indent - 1);
                    self.output.push_str("else\n");
                    for &stmt_id in &else_b {
                        self.format_node(stmt_id, indent);
                        self.output.push('\n');
                    }
                }
                self.write_indent(indent - 1);
                self.output.push_str("end");
            }
            NodeKind::MatchExpr { subject, arms } => {
                self.output.push_str("match ");
                self.format_expr(subject);
                self.output.push('\n');
                let indent = self.current_indent() + 1;
                for arm in &arms {
                    self.write_indent(indent);
                    match &arm.pattern {
                        Pattern::Otherwise => self.output.push_str("otherwise"),
                        Pattern::Literal(lit) => {
                            self.output.push_str("when ");
                            self.format_expr(*lit);
                        }
                        Pattern::Variant { name, bindings } => {
                            self.output.push_str("when ");
                            self.output.push_str(name);
                            if !bindings.is_empty() {
                                self.output.push_str(" { ");
                                for (i, b) in bindings.iter().enumerate() {
                                    if i > 0 {
                                        self.output.push_str(", ");
                                    }
                                    self.output.push_str(b);
                                }
                                self.output.push_str(" }");
                            }
                        }
                    }
                    self.output.push_str(" then ");
                    if arm.body.len() == 1 {
                        self.format_expr(arm.body[0]);
                        self.output.push('\n');
                    } else {
                        self.output.push('\n');
                        for &stmt_id in &arm.body {
                            self.format_node(stmt_id, indent + 1);
                            self.output.push('\n');
                        }
                    }
                }
                self.write_indent(indent - 1);
                self.output.push_str("end");
            }
            NodeKind::RecordConstruct { type_name, fields } => {
                self.output.push_str(&type_name);
                self.output.push_str(" { ");
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(name);
                    self.output.push_str(": ");
                    self.format_expr(*val);
                }
                self.output.push_str(" }");
            }
            NodeKind::RecordUpdate { base, updates } => {
                self.format_expr(base);
                self.output.push_str(" with { ");
                for (i, (name, val)) in updates.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.output.push_str(name);
                    self.output.push_str(": ");
                    self.format_expr(*val);
                }
                self.output.push_str(" }");
            }
            NodeKind::UnionConstruct {
                type_name,
                variant_name,
                fields,
            } => {
                self.output.push_str(&type_name);
                self.output.push('.');
                self.output.push_str(&variant_name);
                if !fields.is_empty() {
                    self.output.push_str(" { ");
                    for (i, (name, val)) in fields.iter().enumerate() {
                        if i > 0 {
                            self.output.push_str(", ");
                        }
                        self.output.push_str(name);
                        self.output.push_str(": ");
                        self.format_expr(*val);
                    }
                    self.output.push_str(" }");
                }
            }
            NodeKind::InterpolatedText { parts } => {
                self.output.push('"');
                for part in &parts {
                    match part {
                        TextPart::Literal(s) => self.output.push_str(s),
                        TextPart::Interpolation(expr) => {
                            self.output.push('{');
                            self.format_expr(*expr);
                            self.output.push('}');
                        }
                    }
                }
                self.output.push('"');
            }
            NodeKind::OldExpr { inner } => {
                self.output.push_str("old(");
                self.format_expr(inner);
                self.output.push(')');
            }
            NodeKind::ReturnExpr { value } => {
                self.output.push_str("return");
                if let Some(val) = value {
                    self.output.push(' ');
                    self.format_expr(val);
                }
            }
            _ => {
                self.output.push_str(&format!("/* unformatted: {:?} */", self.arena.get(node_id).kind));
            }
        }
    }

    fn format_params(&mut self, params: &[Param]) {
        for (i, param) in params.iter().enumerate() {
            if i > 0 {
                self.output.push_str(", ");
            }
            self.output.push_str(&param.name);
            self.output.push_str(": ");
            self.format_type(&param.param_type);
        }
    }

    fn format_type(&mut self, clarity_type: &ClarityType) {
        match clarity_type {
            ClarityType::Integer => self.output.push_str("integer"),
            ClarityType::Decimal => self.output.push_str("decimal"),
            ClarityType::Text => self.output.push_str("text"),
            ClarityType::Boolean => self.output.push_str("boolean"),
            ClarityType::Nothing => self.output.push_str("nothing"),
            ClarityType::ListOf(inner) => {
                self.output.push_str("a list of ");
                self.format_type(inner);
            }
            ClarityType::MappingFrom(key, val) => {
                self.output.push_str("a mapping from ");
                self.format_type(key);
                self.output.push_str(" to ");
                self.format_type(val);
            }
            ClarityType::Optional(inner) => {
                self.output.push_str("an optional ");
                self.format_type(inner);
            }
            ClarityType::Named(name) => self.output.push_str(name),
            ClarityType::Function { params, return_type } => {
                self.output.push_str("fn(");
                for (i, p) in params.iter().enumerate() {
                    if i > 0 {
                        self.output.push_str(", ");
                    }
                    self.format_type(p);
                }
                self.output.push_str("): ");
                self.format_type(return_type);
            }
            ClarityType::Generic(name) => self.output.push_str(name),
        }
    }

    fn write_indent(&mut self, level: usize) {
        for _ in 0..level {
            self.output.push_str("  ");
        }
    }

    fn current_indent(&self) -> usize {
        // Count the indent of the last line
        let spaces = if let Some(last_newline) = self.output.rfind('\n') {
            let after = &self.output[last_newline + 1..];
            after.len() - after.trim_start().len()
        } else {
            let trimmed = self.output.trim_start();
            self.output.len() - trimmed.len()
        };
        spaces / 2
    }
}
