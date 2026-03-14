/// Recursive descent parser for the Legible language.
///
/// Converts a token stream into an arena-allocated AST.
/// Uses Pratt parsing for expression precedence.
use crate::errors::{LegibleError, ErrorCode, Severity, SourceLocation};
use crate::lexer::token::{Span, SpannedToken, Token};
use crate::parser::arena::Arena;
use crate::parser::ast::*;

/// The Legible parser.
pub struct Parser {
    tokens: Vec<SpannedToken>,
    current: usize,
    /// The arena where AST nodes are allocated.
    pub arena: Arena,
    file_name: String,
    source: String,
}

impl Parser {
    /// Create a new parser from a list of tokens.
    pub fn new(tokens: Vec<SpannedToken>, file_name: &str, source: &str) -> Self {
        Self {
            tokens,
            current: 0,
            arena: Arena::new(),
            file_name: file_name.to_string(),
            source: source.to_string(),
        }
    }

    /// Parse the entire token stream and return the root `NodeId` for the program.
    pub fn parse_program(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        let mut statements = Vec::new();
        self.skip_newlines();

        while !self.check(&Token::Eof) {
            let stmt = self.parse_top_level()?;
            statements.push(stmt);
            self.skip_newlines();
        }

        let end_span = self.current_span();
        Ok(self.arena.alloc(
            NodeKind::Program { statements },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_top_level(&mut self) -> Result<NodeId, LegibleError> {
        match self.peek_token() {
            Token::Function => self.parse_function(false),
            Token::Public => {
                self.advance();
                self.skip_newlines();
                match self.peek_token() {
                    Token::Function => self.parse_function(true),
                    _ => Err(self.error_at_current(
                        "Expected 'function' after 'public'",
                        "Add 'function' keyword after 'public'",
                    )),
                }
            }
            Token::Record => self.parse_record(),
            Token::Union => self.parse_union(),
            Token::Use => self.parse_use(),
            _ => self.parse_statement(),
        }
    }

    fn parse_function(&mut self, is_public: bool) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Function)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(&Token::RightParen)?;
        self.expect(&Token::Colon)?;
        let return_type = self.parse_type()?;
        self.skip_newlines();

        // Parse intent
        let intent = if self.check(&Token::Intent) {
            self.advance();
            self.expect(&Token::Colon)?;
            self.parse_intent_text()?
        } else {
            return Err(self.error_at_current(
                "Missing intent declaration",
                "Add 'intent: <description>' as the first line of the function body",
            ));
        };
        self.skip_newlines();

        // Parse optional requires
        let mut requires = Vec::new();
        if self.check(&Token::Requires) {
            self.advance();
            self.expect(&Token::Colon)?;
            requires = self.parse_expression_list()?;
            self.skip_newlines();
        }

        // Parse optional ensures
        let mut ensures = Vec::new();
        if self.check(&Token::Ensures) {
            self.advance();
            self.expect(&Token::Colon)?;
            ensures = self.parse_expression_list()?;
            self.skip_newlines();
        }

        // Parse body
        let body = self.parse_body()?;
        self.expect(&Token::End)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::FunctionDecl {
                name,
                params,
                return_type,
                intent,
                requires,
                ensures,
                body,
                is_public,
            },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_intent_text(&mut self) -> Result<String, LegibleError> {
        let mut parts = Vec::new();
        while !self.check(&Token::Newline) && !self.check(&Token::Eof) {
            let tok = &self.tokens[self.current].token;
            let text = match tok {
                Token::Identifier(s) => s.clone(),
                Token::Integer(n) => n.to_string(),
                Token::Decimal(n) => n.to_string(),
                Token::Text(s) => format!("\"{s}\""),
                Token::Boolean(b) => b.to_string(),
                Token::Plus => "+".to_string(),
                Token::Minus => "-".to_string(),
                Token::Star => "*".to_string(),
                Token::Slash => "/".to_string(),
                Token::And => "and".to_string(),
                Token::Or => "or".to_string(),
                Token::Not => "not".to_string(),
                Token::To => "to".to_string(),
                Token::For => "for".to_string(),
                Token::In => "in".to_string(),
                Token::If => "if".to_string(),
                Token::Then => "then".to_string(),
                Token::Else => "else".to_string(),
                Token::With => "with".to_string(),
                Token::Return => "return".to_string(),
                Token::Let => "let".to_string(),
                Token::Set => "set".to_string(),
                Token::Function => "function".to_string(),
                Token::AListOf => "a list of".to_string(),
                Token::AMappingFrom => "a mapping from".to_string(),
                Token::AnOptional => "an optional".to_string(),
                Token::IntegerType => "integer".to_string(),
                Token::DecimalType => "decimal".to_string(),
                Token::TextType => "text".to_string(),
                Token::BooleanType => "boolean".to_string(),
                Token::NothingType => "nothing".to_string(),
                Token::None => "none".to_string(),
                Token::Do => "do".to_string(),
                Token::End => "end".to_string(),
                Token::Match => "match".to_string(),
                Token::When => "when".to_string(),
                Token::Otherwise => "otherwise".to_string(),
                Token::Record => "record".to_string(),
                Token::Union => "union".to_string(),
                Token::Use => "use".to_string(),
                Token::Fn => "fn".to_string(),
                Token::Public => "public".to_string(),
                Token::Dot => ".".to_string(),
                Token::Comma => ",".to_string(),
                Token::Colon => ":".to_string(),
                Token::LeftParen => "(".to_string(),
                Token::RightParen => ")".to_string(),
                _ => {
                    if let Token::Comment(_) = tok {
                        break;
                    }
                    format!("{tok:?}")
                }
            };
            parts.push(text);
            self.advance();
        }
        Ok(parts.join(" "))
    }

    fn parse_param_list(&mut self) -> Result<Vec<Param>, LegibleError> {
        let mut params = Vec::new();
        if self.check(&Token::RightParen) {
            return Ok(params);
        }
        loop {
            let name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;
            let param_type = self.parse_type()?;
            params.push(Param { name, param_type });
            if !self.match_token(&Token::Comma) {
                break;
            }
        }
        Ok(params)
    }

    fn parse_expression_list(&mut self) -> Result<Vec<NodeId>, LegibleError> {
        let mut exprs = Vec::new();
        loop {
            let expr = self.parse_expression()?;
            exprs.push(expr);
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.skip_newlines();
        }
        Ok(exprs)
    }

    fn parse_record(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Record)?;
        let name = self.expect_identifier()?;
        self.skip_newlines();

        let mut fields = Vec::new();
        while !self.check(&Token::End) && !self.check(&Token::Eof) {
            let field_name = self.expect_identifier()?;
            self.expect(&Token::Colon)?;
            let field_type = self.parse_type()?;
            fields.push(Field {
                name: field_name,
                field_type,
            });
            self.skip_newlines();
        }
        self.expect(&Token::End)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::RecordDecl { name, fields },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_union(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Union)?;
        let name = self.expect_identifier()?;
        self.skip_newlines();

        let mut variants = Vec::new();
        while !self.check(&Token::End) && !self.check(&Token::Eof) {
            let variant_name = self.expect_identifier()?;
            let mut fields = Vec::new();
            if self.match_token(&Token::LeftBrace) {
                while !self.check(&Token::RightBrace) && !self.check(&Token::Eof) {
                    let field_name = self.expect_identifier()?;
                    self.expect(&Token::Colon)?;
                    let field_type = self.parse_type()?;
                    fields.push(Field {
                        name: field_name,
                        field_type,
                    });
                    // Allow trailing comma
                    self.match_token(&Token::Comma);
                }
                self.expect(&Token::RightBrace)?;
            }
            variants.push(Variant {
                name: variant_name,
                fields,
            });
            self.skip_newlines();
        }
        self.expect(&Token::End)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::UnionDecl { name, variants },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_use(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Use)?;
        let module_name = self.expect_identifier()?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::UseDecl { module_name },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_statement(&mut self) -> Result<NodeId, LegibleError> {
        match self.peek_token() {
            Token::Let => self.parse_let(false),
            Token::Mutable => self.parse_let(true),
            Token::Set => self.parse_set(),
            Token::For => self.parse_for(),
            Token::While => self.parse_while(),
            Token::Return => self.parse_return(),
            _ => {
                let expr = self.parse_expression()?;
                Ok(self.arena.alloc(
                    NodeKind::ExprStatement { expr },
                    self.arena.get(expr).span,
                ))
            }
        }
    }

    fn parse_let(&mut self, mutable: bool) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        if mutable {
            self.expect(&Token::Mutable)?;
        } else {
            self.expect(&Token::Let)?;
        }
        let name = self.expect_identifier()?;
        self.expect(&Token::Colon)?;
        let declared_type = self.parse_type()?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expression()?;
        let end_span = self.arena.get(value).span;

        Ok(self.arena.alloc(
            NodeKind::LetBinding {
                name,
                declared_type,
                value,
                mutable,
            },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_set(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Set)?;
        let name = self.expect_identifier()?;
        self.expect(&Token::Assign)?;
        let value = self.parse_expression()?;
        let end_span = self.arena.get(value).span;

        Ok(self.arena.alloc(
            NodeKind::SetStatement { name, value },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_for(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::For)?;
        let binding = self.expect_identifier()?;
        self.expect(&Token::In)?;
        let iterable = self.parse_expression()?;
        self.expect(&Token::Do)?;
        self.skip_newlines();
        let body = self.parse_body()?;
        self.expect(&Token::End)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::ForLoop {
                binding,
                iterable,
                body,
            },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_while(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::While)?;
        let condition = self.parse_expression()?;
        self.expect(&Token::Do)?;
        self.skip_newlines();
        let body = self.parse_body()?;
        self.expect(&Token::End)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::WhileLoop { condition, body },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_return(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Return)?;
        let value = if self.check(&Token::Newline) || self.check(&Token::Eof) || self.check(&Token::End) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        let end_span = if let Some(v) = value {
            self.arena.get(v).span
        } else {
            self.previous_span()
        };

        Ok(self.arena.alloc(
            NodeKind::ReturnExpr { value },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_body(&mut self) -> Result<Vec<NodeId>, LegibleError> {
        let mut stmts = Vec::new();
        while !self.check(&Token::End)
            && !self.check(&Token::Else)
            && !self.check(&Token::When)
            && !self.check(&Token::Otherwise)
            && !self.check(&Token::Eof)
        {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_newlines();
        }
        Ok(stmts)
    }

    // ─── Expression Parsing (Pratt) ─────────────────────────

    fn parse_expression(&mut self) -> Result<NodeId, LegibleError> {
        self.parse_pipeline()
    }

    fn parse_pipeline(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_or()?;
        loop {
            // Allow |> after newlines (pipeline continuation)
            let saved = self.current;
            self.skip_newlines();
            if !self.match_token(&Token::Pipe) {
                self.current = saved;
                break;
            }
            self.skip_newlines();
            let right = self.parse_or()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(NodeKind::Pipeline { left, right }, span);
        }
        Ok(left)
    }

    fn parse_or(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_and()?;
        while self.match_token(&Token::Or) {
            self.skip_newlines();
            let right = self.parse_and()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(
                NodeKind::BinaryOp {
                    left,
                    op: BinaryOperator::Or,
                    right,
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_and(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_equality()?;
        while self.match_token(&Token::And) {
            self.skip_newlines();
            let right = self.parse_equality()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(
                NodeKind::BinaryOp {
                    left,
                    op: BinaryOperator::And,
                    right,
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_equality(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_comparison()?;
        loop {
            let op = if self.match_token(&Token::Equals) {
                BinaryOperator::Eq
            } else if self.match_token(&Token::NotEquals) {
                BinaryOperator::NotEq
            } else {
                break;
            };
            self.skip_newlines();
            let right = self.parse_comparison()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(NodeKind::BinaryOp { left, op, right }, span);
        }
        Ok(left)
    }

    fn parse_comparison(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_concat()?;
        loop {
            let op = if self.match_token(&Token::Greater) {
                BinaryOperator::Gt
            } else if self.match_token(&Token::Less) {
                BinaryOperator::Lt
            } else if self.match_token(&Token::GreaterEqual) {
                BinaryOperator::GtEq
            } else if self.match_token(&Token::LessEqual) {
                BinaryOperator::LtEq
            } else {
                break;
            };
            self.skip_newlines();
            let right = self.parse_concat()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(NodeKind::BinaryOp { left, op, right }, span);
        }
        Ok(left)
    }

    fn parse_concat(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_addition()?;
        while self.match_token(&Token::PlusPlus) {
            self.skip_newlines();
            let right = self.parse_addition()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(
                NodeKind::BinaryOp {
                    left,
                    op: BinaryOperator::Concat,
                    right,
                },
                span,
            );
        }
        Ok(left)
    }

    fn parse_addition(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_multiplication()?;
        loop {
            let op = if self.match_token(&Token::Plus) {
                BinaryOperator::Add
            } else if self.match_token(&Token::Minus) {
                BinaryOperator::Sub
            } else {
                break;
            };
            self.skip_newlines();
            let right = self.parse_multiplication()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(NodeKind::BinaryOp { left, op, right }, span);
        }
        Ok(left)
    }

    fn parse_multiplication(&mut self) -> Result<NodeId, LegibleError> {
        let mut left = self.parse_unary()?;
        loop {
            let op = if self.match_token(&Token::Star) {
                BinaryOperator::Mul
            } else if self.match_token(&Token::Slash) {
                BinaryOperator::Div
            } else if self.match_token(&Token::Percent) {
                BinaryOperator::Mod
            } else {
                break;
            };
            self.skip_newlines();
            let right = self.parse_unary()?;
            let span = Span {
                start: self.arena.get(left).span.start,
                end: self.arena.get(right).span.end,
            };
            left = self.arena.alloc(NodeKind::BinaryOp { left, op, right }, span);
        }
        Ok(left)
    }

    fn parse_unary(&mut self) -> Result<NodeId, LegibleError> {
        if self.match_token(&Token::Minus) {
            let start_span = self.previous_span();
            let operand = self.parse_unary()?;
            let end_span = self.arena.get(operand).span;
            return Ok(self.arena.alloc(
                NodeKind::UnaryOp {
                    op: UnaryOperator::Negate,
                    operand,
                },
                Span {
                    start: start_span.start,
                    end: end_span.end,
                },
            ));
        }
        if self.match_token(&Token::Not) {
            let start_span = self.previous_span();
            let operand = self.parse_unary()?;
            let end_span = self.arena.get(operand).span;
            return Ok(self.arena.alloc(
                NodeKind::UnaryOp {
                    op: UnaryOperator::Not,
                    operand,
                },
                Span {
                    start: start_span.start,
                    end: end_span.end,
                },
            ));
        }
        self.parse_postfix()
    }

    fn parse_postfix(&mut self) -> Result<NodeId, LegibleError> {
        let mut expr = self.parse_primary()?;
        loop {
            if self.match_token(&Token::Dot) {
                let field = self.expect_identifier()?;
                let span = Span {
                    start: self.arena.get(expr).span.start,
                    end: self.previous_span().end,
                };
                // Check if this is a union constructor: Type.Variant
                if self.check(&Token::LeftBrace) {
                    if let NodeKind::Identifier(type_name) = &self.arena.get(expr).kind.clone() {
                        let variant_name = field;
                        self.advance(); // consume '{'
                        let mut fields = Vec::new();
                        while !self.check(&Token::RightBrace) && !self.check(&Token::Eof) {
                            let fname = self.expect_identifier()?;
                            self.expect(&Token::Colon)?;
                            let fval = self.parse_expression()?;
                            fields.push((fname, fval));
                            if !self.match_token(&Token::Comma) {
                                break;
                            }
                            self.skip_newlines();
                        }
                        self.expect(&Token::RightBrace)?;
                        let end_span = self.previous_span();
                        expr = self.arena.alloc(
                            NodeKind::UnionConstruct {
                                type_name: type_name.clone(),
                                variant_name,
                                fields,
                            },
                            Span {
                                start: self.arena.get(expr).span.start,
                                end: end_span.end,
                            },
                        );
                        continue;
                    }
                }
                // Check for unit union variant (no braces)
                expr = self.arena.alloc(NodeKind::FieldAccess { object: expr, field }, span);
            } else if self.check(&Token::LeftParen) {
                self.advance();
                let mut arguments = Vec::new();
                if !self.check(&Token::RightParen) {
                    loop {
                        self.skip_newlines();
                        let arg = self.parse_expression()?;
                        arguments.push(arg);
                        self.skip_newlines();
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                }
                self.skip_newlines();
                self.expect(&Token::RightParen)?;
                let span = Span {
                    start: self.arena.get(expr).span.start,
                    end: self.previous_span().end,
                };
                expr = self.arena.alloc(
                    NodeKind::FunctionCall {
                        callee: expr,
                        arguments,
                    },
                    span,
                );
            } else if self.match_token(&Token::Question) {
                // Optional unwrap — desugar to unwrap(expr)
                let span = Span {
                    start: self.arena.get(expr).span.start,
                    end: self.previous_span().end,
                };
                let unwrap_id = self.arena.alloc(
                    NodeKind::Identifier("unwrap".to_string()),
                    self.previous_span(),
                );
                expr = self.arena.alloc(
                    NodeKind::FunctionCall {
                        callee: unwrap_id,
                        arguments: vec![expr],
                    },
                    span,
                );
            } else if self.check(&Token::With) {
                // Record update: expr with { field: value, ... }
                self.advance();
                self.expect(&Token::LeftBrace)?;
                let mut updates = Vec::new();
                while !self.check(&Token::RightBrace) && !self.check(&Token::Eof) {
                    let fname = self.expect_identifier()?;
                    self.expect(&Token::Colon)?;
                    let fval = self.parse_expression()?;
                    updates.push((fname, fval));
                    if !self.match_token(&Token::Comma) {
                        break;
                    }
                    self.skip_newlines();
                }
                self.expect(&Token::RightBrace)?;
                let span = Span {
                    start: self.arena.get(expr).span.start,
                    end: self.previous_span().end,
                };
                expr = self.arena.alloc(NodeKind::RecordUpdate { base: expr, updates }, span);
            } else {
                break;
            }
        }
        Ok(expr)
    }

    fn parse_primary(&mut self) -> Result<NodeId, LegibleError> {
        let span = self.current_span();
        match self.peek_token() {
            Token::Integer(n) => {
                let n = n;
                self.advance();
                Ok(self.arena.alloc(NodeKind::IntegerLit(n), span))
            }
            Token::Decimal(n) => {
                let n = n;
                self.advance();
                Ok(self.arena.alloc(NodeKind::DecimalLit(n), span))
            }
            Token::Text(s) => {
                let s = s;
                self.advance();
                Ok(self.arena.alloc(NodeKind::TextLit(s), span))
            }
            Token::Boolean(b) => {
                let b = b;
                self.advance();
                Ok(self.arena.alloc(NodeKind::BooleanLit(b), span))
            }
            Token::None => {
                self.advance();
                Ok(self.arena.alloc(NodeKind::NoneLit, span))
            }
            Token::Identifier(name) => {
                let name = name;
                self.advance();
                // Handle old(expr) for contract ensures
                if name == "old" && self.check(&Token::LeftParen) {
                    self.advance(); // consume '('
                    let inner = self.parse_expression()?;
                    self.expect(&Token::RightParen)?;
                    let end_span = self.previous_span();
                    return Ok(self.arena.alloc(
                        NodeKind::OldExpr { inner },
                        Span {
                            start: span.start,
                            end: end_span.end,
                        },
                    ));
                }
                // Check if this is a record constructor: Name { ... }
                if self.check(&Token::LeftBrace) && name.chars().next().is_some_and(|c| c.is_uppercase()) {
                    self.advance(); // consume '{'
                    let mut fields = Vec::new();
                    self.skip_newlines();
                    while !self.check(&Token::RightBrace) && !self.check(&Token::Eof) {
                        let fname = self.expect_identifier()?;
                        self.expect(&Token::Colon)?;
                        let fval = self.parse_expression()?;
                        fields.push((fname, fval));
                        self.skip_newlines();
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                        self.skip_newlines();
                    }
                    self.expect(&Token::RightBrace)?;
                    let end_span = self.previous_span();
                    return Ok(self.arena.alloc(
                        NodeKind::RecordConstruct {
                            type_name: name,
                            fields,
                        },
                        Span {
                            start: span.start,
                            end: end_span.end,
                        },
                    ));
                }
                Ok(self.arena.alloc(NodeKind::Identifier(name), span))
            }
            Token::LeftParen => {
                self.advance();
                self.skip_newlines();
                let expr = self.parse_expression()?;
                self.skip_newlines();
                self.expect(&Token::RightParen)?;
                Ok(expr)
            }
            Token::LeftBracket => self.parse_list_literal(),
            Token::LeftBrace => self.parse_mapping_literal(),
            Token::If => self.parse_if_expression(),
            Token::Match => self.parse_match_expression(),
            Token::Fn => self.parse_lambda(),
            Token::InterpolationStart => self.parse_interpolated_string(),
            _ => Err(self.error_at_current(
                &format!("Unexpected token: {:?}", self.peek_token()),
                "Expected an expression (literal, identifier, or opening delimiter)",
            )),
        }
    }

    fn parse_list_literal(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::LeftBracket)?;
        let mut elements = Vec::new();
        self.skip_newlines();
        while !self.check(&Token::RightBracket) && !self.check(&Token::Eof) {
            let elem = self.parse_expression()?;
            elements.push(elem);
            self.skip_newlines();
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.skip_newlines();
        }
        self.expect(&Token::RightBracket)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::ListLit { elements },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_mapping_literal(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::LeftBrace)?;
        let mut entries = Vec::new();
        self.skip_newlines();
        while !self.check(&Token::RightBrace) && !self.check(&Token::Eof) {
            let key = self.parse_expression()?;
            self.expect(&Token::Colon)?;
            let value = self.parse_expression()?;
            entries.push((key, value));
            self.skip_newlines();
            if !self.match_token(&Token::Comma) {
                break;
            }
            self.skip_newlines();
        }
        self.expect(&Token::RightBrace)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::MappingLit { entries },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_if_expression(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::If)?;
        let condition = self.parse_expression()?;
        self.expect(&Token::Then)?;
        self.skip_newlines();

        let then_branch = self.parse_body()?;

        let else_branch = if self.match_token(&Token::Else) {
            self.skip_newlines();
            if self.check(&Token::If) {
                // else if => parse as a single-element else branch containing an if expr
                let nested_if = self.parse_if_expression()?;
                Some(vec![nested_if])
            } else {
                Some(self.parse_body()?)
            }
        } else {
            None
        };

        // Only expect `end` if this is the outermost if (not an else-if chain).
        // The outermost if always ends with `end`. The nested else-if already
        // consumed its own `end` via the recursive parse_if_expression call.
        if else_branch.as_ref().is_none_or(|branch| {
            branch.len() != 1 || !matches!(self.arena.get(branch[0]).kind, NodeKind::IfExpr { .. })
        }) {
            self.expect(&Token::End)?;
        }
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::IfExpr {
                condition,
                then_branch,
                else_branch,
            },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_match_expression(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Match)?;
        let subject = self.parse_expression()?;
        self.skip_newlines();

        let mut arms = Vec::new();
        while self.check(&Token::When) || self.check(&Token::Otherwise) {
            if self.match_token(&Token::When) {
                let pattern = self.parse_pattern()?;
                self.expect(&Token::Then)?;
                self.skip_newlines();
                let body = self.parse_match_arm_body()?;
                arms.push(MatchArm { pattern, body });
            } else {
                self.advance(); // consume 'otherwise'
                self.expect(&Token::Then)?;
                self.skip_newlines();
                let body = self.parse_match_arm_body()?;
                arms.push(MatchArm {
                    pattern: Pattern::Otherwise,
                    body,
                });
            }
            self.skip_newlines();
        }

        self.expect(&Token::End)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::MatchExpr { subject, arms },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_match_arm_body(&mut self) -> Result<Vec<NodeId>, LegibleError> {
        let mut stmts = Vec::new();
        while !self.check(&Token::When)
            && !self.check(&Token::Otherwise)
            && !self.check(&Token::End)
            && !self.check(&Token::Eof)
        {
            let stmt = self.parse_statement()?;
            stmts.push(stmt);
            self.skip_newlines();
        }
        Ok(stmts)
    }

    fn parse_pattern(&mut self) -> Result<Pattern, LegibleError> {
        match self.peek_token() {
            Token::Integer(_)
            | Token::Decimal(_)
            | Token::Text(_)
            | Token::Boolean(_)
            | Token::None => {
                let expr = self.parse_primary()?;
                Ok(Pattern::Literal(expr))
            }
            Token::Identifier(name) => {
                let name = name;
                self.advance();
                // Check for variant pattern: VariantName { bindings }
                if self.match_token(&Token::LeftBrace) {
                    let mut bindings = Vec::new();
                    while !self.check(&Token::RightBrace) && !self.check(&Token::Eof) {
                        let binding = self.expect_identifier()?;
                        bindings.push(binding);
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                    self.expect(&Token::RightBrace)?;
                    Ok(Pattern::Variant { name, bindings })
                } else {
                    // Could be a unit variant or a literal identifier used as a pattern
                    // Treat as variant with no bindings
                    Ok(Pattern::Variant {
                        name,
                        bindings: Vec::new(),
                    })
                }
            }
            Token::InterpolationStart => {
                let expr = self.parse_primary()?;
                Ok(Pattern::Literal(expr))
            }
            _ => Err(self.error_at_current(
                "Expected a pattern",
                "Use a literal, variant name, or 'otherwise'",
            )),
        }
    }

    fn parse_lambda(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::Fn)?;
        self.expect(&Token::LeftParen)?;
        let params = self.parse_param_list()?;
        self.expect(&Token::RightParen)?;
        self.expect(&Token::Colon)?;
        let return_type = self.parse_type()?;
        self.expect(&Token::Arrow)?;
        let body = self.parse_expression()?;
        let end_span = self.arena.get(body).span;

        Ok(self.arena.alloc(
            NodeKind::Lambda {
                params,
                return_type,
                body,
            },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    fn parse_interpolated_string(&mut self) -> Result<NodeId, LegibleError> {
        let start_span = self.current_span();
        self.expect(&Token::InterpolationStart)?;
        let mut parts = Vec::new();

        while !self.check_interpolation_end() && !self.check(&Token::Eof) {
            match self.peek_token() {
                Token::InterpolationLiteral(s) => {
                    let s = s;
                    self.advance();
                    parts.push(TextPart::Literal(s));
                }
                Token::InterpolationExprStart => {
                    self.advance();
                    let expr = self.parse_expression()?;
                    self.expect_interpolation_expr_end()?;
                    parts.push(TextPart::Interpolation(expr));
                }
                _ => {
                    return Err(self.error_at_current(
                        "Unexpected token in interpolated string",
                        "Expected a literal segment or interpolation expression",
                    ));
                }
            }
        }
        self.expect(&Token::InterpolationEnd)?;
        let end_span = self.previous_span();

        Ok(self.arena.alloc(
            NodeKind::InterpolatedText { parts },
            Span {
                start: start_span.start,
                end: end_span.end,
            },
        ))
    }

    // ─── Type Parsing ───────────────────────────────────────

    fn parse_type(&mut self) -> Result<LegibleType, LegibleError> {
        match self.peek_token() {
            Token::IntegerType => {
                self.advance();
                Ok(LegibleType::Integer)
            }
            Token::DecimalType => {
                self.advance();
                Ok(LegibleType::Decimal)
            }
            Token::TextType => {
                self.advance();
                Ok(LegibleType::Text)
            }
            Token::BooleanType => {
                self.advance();
                Ok(LegibleType::Boolean)
            }
            Token::NothingType => {
                self.advance();
                Ok(LegibleType::Nothing)
            }
            Token::AListOf => {
                self.advance();
                let inner = self.parse_type()?;
                Ok(LegibleType::ListOf(Box::new(inner)))
            }
            Token::AMappingFrom => {
                self.advance();
                let key_type = self.parse_type()?;
                self.expect(&Token::To)?;
                let value_type = self.parse_type()?;
                Ok(LegibleType::MappingFrom(
                    Box::new(key_type),
                    Box::new(value_type),
                ))
            }
            Token::AnOptional => {
                self.advance();
                let inner = self.parse_type()?;
                Ok(LegibleType::Optional(Box::new(inner)))
            }
            Token::Fn => {
                self.advance();
                self.expect(&Token::LeftParen)?;
                let mut params = Vec::new();
                if !self.check(&Token::RightParen) {
                    loop {
                        let t = self.parse_type()?;
                        params.push(t);
                        if !self.match_token(&Token::Comma) {
                            break;
                        }
                    }
                }
                self.expect(&Token::RightParen)?;
                self.expect(&Token::Colon)?;
                let return_type = self.parse_type()?;
                Ok(LegibleType::Function {
                    params,
                    return_type: Box::new(return_type),
                })
            }
            Token::Identifier(name) => {
                let name = name;
                self.advance();
                Ok(LegibleType::Named(name))
            }
            _ => Err(self.error_at_current(
                &format!("Expected a type, got {:?}", self.peek_token()),
                "Use a type name like 'integer', 'text', 'a list of T', etc.",
            )),
        }
    }

    // ─── Token Helpers ──────────────────────────────────────

    fn peek_token(&self) -> Token {
        if self.current < self.tokens.len() {
            self.tokens[self.current].token.clone()
        } else {
            Token::Eof
        }
    }

    fn check(&self, token: &Token) -> bool {
        std::mem::discriminant(&self.peek_token()) == std::mem::discriminant(token)
    }

    fn check_interpolation_end(&self) -> bool {
        matches!(self.peek_token(), Token::InterpolationEnd)
    }

    fn expect_interpolation_expr_end(&mut self) -> Result<(), LegibleError> {
        if matches!(self.peek_token(), Token::InterpolationExprEnd) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_at_current(
                "Expected end of interpolation expression '}'",
                "Add a closing '}' to end the interpolation",
            ))
        }
    }

    fn match_token(&mut self, token: &Token) -> bool {
        if self.check(token) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn advance(&mut self) -> &SpannedToken {
        if self.current < self.tokens.len() {
            self.current += 1;
        }
        &self.tokens[self.current - 1]
    }

    fn expect(&mut self, token: &Token) -> Result<(), LegibleError> {
        if self.check(token) {
            self.advance();
            Ok(())
        } else {
            Err(self.error_at_current(
                &format!("Expected {:?}, got {:?}", token, self.peek_token()),
                &format!("Add {:?} here", token),
            ))
        }
    }

    fn expect_identifier(&mut self) -> Result<String, LegibleError> {
        match self.peek_token() {
            Token::Identifier(name) => {
                self.advance();
                Ok(name)
            }
            _ => Err(self.error_at_current(
                &format!("Expected identifier, got {:?}", self.peek_token()),
                "Provide an identifier name",
            )),
        }
    }

    fn skip_newlines(&mut self) {
        while self.check(&Token::Newline) || self.check(&Token::Comment(String::new())) {
            self.advance();
        }
    }

    fn current_span(&self) -> Span {
        if self.current < self.tokens.len() {
            self.tokens[self.current].span
        } else {
            Span { start: 0, end: 0 }
        }
    }

    fn previous_span(&self) -> Span {
        if self.current > 0 {
            self.tokens[self.current - 1].span
        } else {
            Span { start: 0, end: 0 }
        }
    }

    fn error_at_current(&self, message: &str, suggestion: &str) -> LegibleError {
        let span = self.current_span();
        let (line, column) =
            crate::errors::reporter::offset_to_line_col(&self.source, span.start);
        LegibleError {
            code: ErrorCode::UnexpectedToken,
            severity: Severity::Error,
            location: SourceLocation {
                file: self.file_name.clone(),
                line,
                column,
                end_line: line,
                end_column: column + (span.end - span.start),
            },
            message: message.to_string(),
            context: String::new(),
            suggestion: suggestion.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lexer::scan;

    fn parse(source: &str) -> (Arena, NodeId) {
        let tokens = scan(source).expect("lexer error");
        let mut parser = Parser::new(tokens, "<test>", source);
        let root = parser.parse_program().expect("parse error");
        (parser.arena, root)
    }

    #[test]
    fn parse_let_binding() {
        let (arena, root) = parse("let x: integer = 42");
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            assert_eq!(statements.len(), 1);
            if let NodeKind::LetBinding {
                name,
                declared_type,
                mutable,
                ..
            } = &arena.get(statements[0]).kind
            {
                assert_eq!(name, "x");
                assert_eq!(*declared_type, LegibleType::Integer);
                assert!(!mutable);
            } else {
                panic!("Expected LetBinding");
            }
        } else {
            panic!("Expected Program");
        }
    }

    #[test]
    fn parse_function_decl() {
        let source = "function greet(name: text): text\n  intent: produce a greeting\n  return name\nend";
        let (arena, root) = parse(source);
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            assert_eq!(statements.len(), 1);
            if let NodeKind::FunctionDecl { name, params, .. } = &arena.get(statements[0]).kind {
                assert_eq!(name, "greet");
                assert_eq!(params.len(), 1);
                assert_eq!(params[0].name, "name");
            } else {
                panic!("Expected FunctionDecl");
            }
        } else {
            panic!("Expected Program");
        }
    }

    #[test]
    fn parse_if_expression() {
        let source = "if true then\n  1\nelse\n  2\nend";
        let (arena, root) = parse(source);
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            assert_eq!(statements.len(), 1);
        } else {
            panic!("Expected Program");
        }
    }

    #[test]
    fn parse_record_decl() {
        let source = "record User\n  name: text\n  age: integer\nend";
        let (arena, root) = parse(source);
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            if let NodeKind::RecordDecl { name, fields } = &arena.get(statements[0]).kind {
                assert_eq!(name, "User");
                assert_eq!(fields.len(), 2);
            } else {
                panic!("Expected RecordDecl");
            }
        } else {
            panic!("Expected Program");
        }
    }

    #[test]
    fn parse_pipeline() {
        let source = "let x: integer = 1\nlet y: integer = x |> f(2)";
        let (arena, root) = parse(source);
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            assert_eq!(statements.len(), 2);
        } else {
            panic!("Expected Program");
        }
    }

    #[test]
    fn parse_lambda() {
        let source = "let f: fn(integer): integer = fn(x: integer): integer => x + 1";
        let (arena, root) = parse(source);
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            assert_eq!(statements.len(), 1);
        } else {
            panic!("Expected Program");
        }
    }

    #[test]
    fn parse_list_literal() {
        let source = "let xs: a list of integer = [1, 2, 3]";
        let (arena, root) = parse(source);
        let prog = arena.get(root);
        if let NodeKind::Program { statements } = &prog.kind {
            assert_eq!(statements.len(), 1);
            if let NodeKind::LetBinding { declared_type, .. } = &arena.get(statements[0]).kind {
                assert_eq!(*declared_type, LegibleType::ListOf(Box::new(LegibleType::Integer)));
            } else {
                panic!("Expected LetBinding");
            }
        } else {
            panic!("Expected Program");
        }
    }
}
