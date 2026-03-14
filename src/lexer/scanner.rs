/// Character-by-character scanner for the Clarity language.
///
/// Produces a flat sequence of `SpannedToken` values from source text.
/// Handles multi-word type keywords (`a list of`, `a mapping from`,
/// `an optional`) via lookahead, and string interpolation via structured
/// token sequences.
use crate::errors::{ClarityError, ErrorCode, Severity, SourceLocation};
use crate::lexer::token::{Span, SpannedToken, Token};

/// Tokenize a source string into a list of spanned tokens.
///
/// Comments are included in the output so the formatter can preserve them.
/// Newlines are emitted as `Token::Newline` for statement termination.
#[must_use]
pub fn scan(source: &str) -> Result<Vec<SpannedToken>, ClarityError> {
    let mut scanner = Scanner::new(source);
    scanner.scan_all()?;
    Ok(scanner.tokens)
}

struct Scanner {
    source: Vec<char>,
    tokens: Vec<SpannedToken>,
    start: usize,
    current: usize,
    /// Byte offset corresponding to `start` (char index).
    start_byte: usize,
    /// Byte offset corresponding to `current` (char index).
    current_byte: usize,
    /// The original source as a string slice for error reporting.
    source_str: String,
}

impl Scanner {
    fn new(source: &str) -> Self {
        Self {
            source: source.chars().collect(),
            tokens: Vec::new(),
            start: 0,
            current: 0,
            start_byte: 0,
            current_byte: 0,
            source_str: source.to_string(),
        }
    }

    fn scan_all(&mut self) -> Result<(), ClarityError> {
        while !self.is_at_end() {
            self.start = self.current;
            self.start_byte = self.current_byte;
            self.scan_token()?;
        }
        self.tokens.push(SpannedToken {
            token: Token::Eof,
            span: Span {
                start: self.current_byte,
                end: self.current_byte,
            },
        });
        Ok(())
    }

    fn scan_token(&mut self) -> Result<(), ClarityError> {
        let ch = self.advance();
        match ch {
            ' ' | '\t' | '\r' => {} // skip whitespace (not newlines)
            '\n' => self.add_token(Token::Newline),
            '(' => self.add_token(Token::LeftParen),
            ')' => self.add_token(Token::RightParen),
            '[' => self.add_token(Token::LeftBracket),
            ']' => self.add_token(Token::RightBracket),
            '{' => self.add_token(Token::LeftBrace),
            '}' => self.add_token(Token::RightBrace),
            ',' => self.add_token(Token::Comma),
            '.' => self.add_token(Token::Dot),
            ':' => self.add_token(Token::Colon),
            '?' => self.add_token(Token::Question),
            '*' => self.add_token(Token::Star),
            '%' => self.add_token(Token::Percent),
            '+' => {
                if self.match_char('+') {
                    self.add_token(Token::PlusPlus);
                } else {
                    self.add_token(Token::Plus);
                }
            }
            '-' => {
                if self.match_char('-') {
                    self.scan_comment();
                } else {
                    self.add_token(Token::Minus);
                }
            }
            '/' => self.add_token(Token::Slash),
            '=' => {
                if self.match_char('=') {
                    self.add_token(Token::Equals);
                } else if self.match_char('>') {
                    self.add_token(Token::Arrow);
                } else {
                    self.add_token(Token::Assign);
                }
            }
            '!' => {
                if self.match_char('=') {
                    self.add_token(Token::NotEquals);
                } else {
                    return Err(self.error("Unexpected character '!'", "Use 'not' for logical negation or '!=' for not-equals"));
                }
            }
            '>' => {
                if self.match_char('=') {
                    self.add_token(Token::GreaterEqual);
                } else {
                    self.add_token(Token::Greater);
                }
            }
            '<' => {
                if self.match_char('=') {
                    self.add_token(Token::LessEqual);
                } else {
                    self.add_token(Token::Less);
                }
            }
            '|' => {
                if self.match_char('>') {
                    self.add_token(Token::Pipe);
                } else {
                    return Err(self.error("Unexpected character '|'", "Use '|>' for the pipeline operator"));
                }
            }
            '"' => self.scan_string()?,
            _ => {
                if ch.is_ascii_digit() {
                    self.scan_number()?;
                } else if ch.is_alphabetic() || ch == '_' {
                    self.scan_identifier()?;
                } else {
                    return Err(self.error(
                        &format!("Unexpected character '{ch}'"),
                        "Remove this character or replace it with a valid token",
                    ));
                }
            }
        }
        Ok(())
    }

    fn scan_comment(&mut self) {
        let start = self.current;
        while !self.is_at_end() && self.peek() != '\n' {
            self.advance();
        }
        let text: String = self.source[start..self.current].iter().collect();
        self.add_token(Token::Comment(text.trim().to_string()));
    }

    fn scan_string(&mut self) -> Result<(), ClarityError> {
        // Check for triple-quoted string
        if self.peek() == '"' && self.peek_next() == '"' {
            self.advance(); // second "
            self.advance(); // third "
            return self.scan_triple_string();
        }
        self.scan_regular_string()
    }

    fn scan_regular_string(&mut self) -> Result<(), ClarityError> {
        let mut has_interpolation = false;
        let mut parts: Vec<(Token, Span)> = Vec::new();
        let mut literal_buf = String::new();
        let literal_start_byte = self.current_byte;
        let string_start_byte = self.start_byte;

        while !self.is_at_end() && self.peek() != '"' {
            if self.peek() == '{' {
                has_interpolation = true;
                // Save literal segment
                if !literal_buf.is_empty() {
                    parts.push((
                        Token::InterpolationLiteral(literal_buf.clone()),
                        Span {
                            start: literal_start_byte,
                            end: self.current_byte,
                        },
                    ));
                    literal_buf.clear();
                }
                self.advance(); // consume '{'
                let expr_start_byte = self.current_byte;
                parts.push((
                    Token::InterpolationExprStart,
                    Span {
                        start: self.current_byte - 1,
                        end: self.current_byte,
                    },
                ));
                // Scan tokens inside the interpolation until '}'
                let mut depth = 1;
                while !self.is_at_end() && depth > 0 {
                    if self.peek() == '}' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                    } else if self.peek() == '{' {
                        depth += 1;
                    }
                    self.start = self.current;
                    self.start_byte = self.current_byte;
                    self.scan_token()?;
                    // Move last token from self.tokens to parts
                    if let Some(tok) = self.tokens.pop() {
                        parts.push((tok.token, tok.span));
                    }
                }
                if self.is_at_end() {
                    return Err(self.error(
                        "Unterminated string interpolation",
                        "Add a closing '}' to end the interpolation expression",
                    ));
                }
                let _ = expr_start_byte;
                parts.push((
                    Token::InterpolationExprEnd,
                    Span {
                        start: self.current_byte,
                        end: self.current_byte + 1,
                    },
                ));
                self.advance(); // consume '}'
            } else if self.peek() == '\\' {
                self.advance(); // backslash
                if self.is_at_end() {
                    return Err(self.error(
                        "Unterminated escape sequence",
                        "Add a character after the backslash",
                    ));
                }
                let escaped = self.advance();
                match escaped {
                    'n' => literal_buf.push('\n'),
                    't' => literal_buf.push('\t'),
                    'r' => literal_buf.push('\r'),
                    '\\' => literal_buf.push('\\'),
                    '"' => literal_buf.push('"'),
                    '{' => literal_buf.push('{'),
                    '}' => literal_buf.push('}'),
                    _ => {
                        literal_buf.push('\\');
                        literal_buf.push(escaped);
                    }
                }
            } else if self.peek() == '\n' {
                return Err(self.error(
                    "Unterminated string",
                    "Close the string with '\"' before the end of the line, or use triple-quoted strings for multiline text",
                ));
            } else {
                literal_buf.push(self.advance());
            }
        }

        if self.is_at_end() {
            return Err(self.error(
                "Unterminated string",
                "Add a closing '\"' to end the string",
            ));
        }
        self.advance(); // closing "

        if has_interpolation {
            // Push remaining literal
            if !literal_buf.is_empty() {
                parts.push((
                    Token::InterpolationLiteral(literal_buf),
                    Span {
                        start: literal_start_byte,
                        end: self.current_byte - 1,
                    },
                ));
            }
            self.tokens.push(SpannedToken {
                token: Token::InterpolationStart,
                span: Span {
                    start: string_start_byte,
                    end: string_start_byte + 1,
                },
            });
            for (token, span) in parts {
                self.tokens.push(SpannedToken { token, span });
            }
            self.tokens.push(SpannedToken {
                token: Token::InterpolationEnd,
                span: Span {
                    start: self.current_byte - 1,
                    end: self.current_byte,
                },
            });
        } else {
            self.add_token(Token::Text(literal_buf));
        }
        Ok(())
    }

    fn scan_triple_string(&mut self) -> Result<(), ClarityError> {
        let mut content = String::new();
        while !self.is_at_end() {
            if self.peek() == '"' && self.peek_next() == '"' && self.peek_at(2) == '"' {
                self.advance();
                self.advance();
                self.advance();
                // Trim leading newline if present
                if content.starts_with('\n') {
                    content.remove(0);
                }
                // Trim trailing newline if present
                if content.ends_with('\n') {
                    content.pop();
                }
                self.add_token(Token::Text(content));
                return Ok(());
            }
            content.push(self.advance());
        }
        Err(self.error(
            "Unterminated triple-quoted string",
            "Add closing '\"\"\"' to end the string",
        ))
    }

    fn scan_number(&mut self) -> Result<(), ClarityError> {
        while !self.is_at_end() && self.peek().is_ascii_digit() {
            self.advance();
        }
        if !self.is_at_end() && self.peek() == '.' && self.peek_next().is_ascii_digit() {
            self.advance(); // consume '.'
            while !self.is_at_end() && self.peek().is_ascii_digit() {
                self.advance();
            }
            let text: String = self.source[self.start..self.current].iter().collect();
            let value: f64 = text.parse().map_err(|_| {
                self.error(
                    &format!("Invalid decimal literal: {text}"),
                    "Ensure the number is a valid decimal value",
                )
            })?;
            self.add_token(Token::Decimal(value));
        } else {
            let text: String = self.source[self.start..self.current].iter().collect();
            let value: i64 = text.parse().map_err(|_| {
                self.error(
                    &format!("Invalid integer literal: {text}"),
                    "Ensure the number is a valid 64-bit integer",
                )
            })?;
            self.add_token(Token::Integer(value));
        }
        Ok(())
    }

    fn scan_identifier(&mut self) -> Result<(), ClarityError> {
        while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
            self.advance();
        }
        let text: String = self.source[self.start..self.current].iter().collect();

        let token = match text.as_str() {
            "let" => Token::Let,
            "mutable" => Token::Mutable,
            "set" => Token::Set,
            "function" => Token::Function,
            "public" => Token::Public,
            "return" => Token::Return,
            "if" => Token::If,
            "then" => Token::Then,
            "else" => Token::Else,
            "end" => Token::End,
            "match" => Token::Match,
            "when" => Token::When,
            "otherwise" => Token::Otherwise,
            "for" => Token::For,
            "in" => Token::In,
            "do" => Token::Do,
            "while" => Token::While,
            "record" => Token::Record,
            "union" => Token::Union,
            "use" => Token::Use,
            "with" => Token::With,
            "intent" => Token::Intent,
            "requires" => Token::Requires,
            "ensures" => Token::Ensures,
            "and" => Token::And,
            "or" => Token::Or,
            "not" => Token::Not,
            "fn" => Token::Fn,
            "true" => Token::Boolean(true),
            "false" => Token::Boolean(false),
            "none" => Token::None,
            "integer" => Token::IntegerType,
            "decimal" => Token::DecimalType,
            "text" => Token::TextType,
            "boolean" => Token::BooleanType,
            "nothing" => Token::NothingType,
            "to" => Token::To,
            "a" => {
                // Try multi-word: "a list of" or "a mapping from"
                if self.try_match_words(&["list", "of"]) {
                    Token::AListOf
                } else if self.try_match_words(&["mapping", "from"]) {
                    Token::AMappingFrom
                } else {
                    Token::Identifier(text)
                }
            }
            "an" => {
                // Try multi-word: "an optional"
                if self.try_match_words(&["optional"]) {
                    Token::AnOptional
                } else {
                    Token::Identifier(text)
                }
            }
            _ => Token::Identifier(text),
        };
        self.add_token(token);
        Ok(())
    }

    /// Try to match a sequence of whitespace-separated words following the
    /// current position. If successful, advances the scanner past them and
    /// returns `true`. Otherwise, leaves the scanner position unchanged.
    fn try_match_words(&mut self, words: &[&str]) -> bool {
        let saved_current = self.current;
        let saved_byte = self.current_byte;

        for &word in words {
            // Skip whitespace
            let mut found_space = false;
            while !self.is_at_end() && (self.peek() == ' ' || self.peek() == '\t') {
                self.advance();
                found_space = true;
            }
            if !found_space {
                self.current = saved_current;
                self.current_byte = saved_byte;
                return false;
            }
            // Try to match the word
            let word_start = self.current;
            while !self.is_at_end() && (self.peek().is_alphanumeric() || self.peek() == '_') {
                self.advance();
            }
            let scanned: String = self.source[word_start..self.current].iter().collect();
            if scanned != word {
                self.current = saved_current;
                self.current_byte = saved_byte;
                return false;
            }
        }
        true
    }

    // ─── Helpers ──────────────────────────────────────────────

    fn is_at_end(&self) -> bool {
        self.current >= self.source.len()
    }

    fn advance(&mut self) -> char {
        let ch = self.source[self.current];
        self.current += 1;
        self.current_byte += ch.len_utf8();
        ch
    }

    fn peek(&self) -> char {
        if self.is_at_end() {
            '\0'
        } else {
            self.source[self.current]
        }
    }

    fn peek_next(&self) -> char {
        if self.current + 1 >= self.source.len() {
            '\0'
        } else {
            self.source[self.current + 1]
        }
    }

    fn peek_at(&self, offset: usize) -> char {
        let idx = self.current + offset;
        if idx >= self.source.len() {
            '\0'
        } else {
            self.source[idx]
        }
    }

    fn match_char(&mut self, expected: char) -> bool {
        if self.is_at_end() || self.source[self.current] != expected {
            return false;
        }
        self.current += 1;
        self.current_byte += expected.len_utf8();
        true
    }

    fn add_token(&mut self, token: Token) {
        self.tokens.push(SpannedToken {
            token,
            span: Span {
                start: self.start_byte,
                end: self.current_byte,
            },
        });
    }

    fn error(&self, message: &str, suggestion: &str) -> ClarityError {
        let (line, column) =
            crate::errors::reporter::offset_to_line_col(&self.source_str, self.start_byte);
        ClarityError {
            code: ErrorCode::Syntax,
            severity: Severity::Error,
            location: SourceLocation {
                file: "<input>".to_string(),
                line,
                column,
                end_line: line,
                end_column: column + (self.current_byte - self.start_byte),
            },
            message: message.to_string(),
            context: self.get_context_line(),
            suggestion: suggestion.to_string(),
        }
    }

    fn get_context_line(&self) -> String {
        let bytes = self.source_str.as_bytes();
        let mut line_start = self.start_byte;
        while line_start > 0 && bytes.get(line_start - 1).copied() != Some(b'\n') {
            line_start -= 1;
        }
        let mut line_end = self.start_byte;
        while line_end < bytes.len() && bytes[line_end] != b'\n' {
            line_end += 1;
        }
        self.source_str[line_start..line_end].to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tokens(source: &str) -> Vec<Token> {
        scan(source)
            .unwrap()
            .into_iter()
            .map(|st| st.token)
            .filter(|t| !matches!(t, Token::Newline | Token::Eof))
            .collect()
    }

    #[test]
    fn tokenize_simple_let() {
        let result = tokens("let x: integer = 42");
        assert_eq!(
            result,
            vec![
                Token::Let,
                Token::Identifier("x".into()),
                Token::Colon,
                Token::IntegerType,
                Token::Assign,
                Token::Integer(42),
            ]
        );
    }

    #[test]
    fn tokenize_decimal() {
        let result = tokens("3.14");
        assert_eq!(result, vec![Token::Decimal(3.14)]);
    }

    #[test]
    fn tokenize_string() {
        let result = tokens(r#""hello world""#);
        assert_eq!(result, vec![Token::Text("hello world".into())]);
    }

    #[test]
    fn tokenize_empty_string() {
        let result = tokens(r#""""#);
        assert_eq!(result, vec![Token::Text(String::new())]);
    }

    #[test]
    fn tokenize_operators() {
        let result = tokens("+ - * / % ++ |> == != > < >= <= = =>");
        assert_eq!(
            result,
            vec![
                Token::Plus,
                Token::Minus,
                Token::Star,
                Token::Slash,
                Token::Percent,
                Token::PlusPlus,
                Token::Pipe,
                Token::Equals,
                Token::NotEquals,
                Token::Greater,
                Token::Less,
                Token::GreaterEqual,
                Token::LessEqual,
                Token::Assign,
                Token::Arrow,
            ]
        );
    }

    #[test]
    fn tokenize_keywords() {
        let result = tokens("let mutable set function public return if then else end");
        assert_eq!(
            result,
            vec![
                Token::Let,
                Token::Mutable,
                Token::Set,
                Token::Function,
                Token::Public,
                Token::Return,
                Token::If,
                Token::Then,
                Token::Else,
                Token::End,
            ]
        );
    }

    #[test]
    fn tokenize_boolean_and_none() {
        let result = tokens("true false none");
        assert_eq!(
            result,
            vec![Token::Boolean(true), Token::Boolean(false), Token::None,]
        );
    }

    #[test]
    fn tokenize_comment() {
        let result = tokens("-- this is a comment");
        assert_eq!(
            result,
            vec![Token::Comment("this is a comment".into())]
        );
    }

    #[test]
    fn tokenize_multiword_type_a_list_of() {
        let result = tokens("a list of integer");
        assert_eq!(result, vec![Token::AListOf, Token::IntegerType]);
    }

    #[test]
    fn tokenize_multiword_type_a_mapping_from() {
        let result = tokens("a mapping from text to integer");
        assert_eq!(
            result,
            vec![
                Token::AMappingFrom,
                Token::TextType,
                Token::To,
                Token::IntegerType,
            ]
        );
    }

    #[test]
    fn tokenize_multiword_type_an_optional() {
        let result = tokens("an optional integer");
        assert_eq!(result, vec![Token::AnOptional, Token::IntegerType]);
    }

    #[test]
    fn tokenize_delimiters() {
        let result = tokens("( ) [ ] { }");
        assert_eq!(
            result,
            vec![
                Token::LeftParen,
                Token::RightParen,
                Token::LeftBracket,
                Token::RightBracket,
                Token::LeftBrace,
                Token::RightBrace,
            ]
        );
    }

    #[test]
    fn tokenize_function_decl() {
        let result = tokens("function greet(name: text): text");
        assert_eq!(
            result,
            vec![
                Token::Function,
                Token::Identifier("greet".into()),
                Token::LeftParen,
                Token::Identifier("name".into()),
                Token::Colon,
                Token::TextType,
                Token::RightParen,
                Token::Colon,
                Token::TextType,
            ]
        );
    }

    #[test]
    fn tokenize_pipe_operator() {
        let result = tokens("x |> f(y)");
        assert_eq!(
            result,
            vec![
                Token::Identifier("x".into()),
                Token::Pipe,
                Token::Identifier("f".into()),
                Token::LeftParen,
                Token::Identifier("y".into()),
                Token::RightParen,
            ]
        );
    }

    #[test]
    fn tokenize_lambda() {
        let result = tokens("fn(x: integer): integer => x + 1");
        assert_eq!(
            result,
            vec![
                Token::Fn,
                Token::LeftParen,
                Token::Identifier("x".into()),
                Token::Colon,
                Token::IntegerType,
                Token::RightParen,
                Token::Colon,
                Token::IntegerType,
                Token::Arrow,
                Token::Identifier("x".into()),
                Token::Plus,
                Token::Integer(1),
            ]
        );
    }

    #[test]
    fn tokenize_string_interpolation() {
        let result = tokens(r#""Hello, {name}!""#);
        assert_eq!(
            result,
            vec![
                Token::InterpolationStart,
                Token::InterpolationLiteral("Hello, ".into()),
                Token::InterpolationExprStart,
                Token::Identifier("name".into()),
                Token::InterpolationExprEnd,
                Token::InterpolationLiteral("!".into()),
                Token::InterpolationEnd,
            ]
        );
    }

    #[test]
    fn tokenize_newlines() {
        let all = scan("a\nb").unwrap();
        let toks: Vec<_> = all.iter().map(|t| &t.token).collect();
        assert_eq!(
            toks,
            vec![
                &Token::Identifier("a".into()),
                &Token::Newline,
                &Token::Identifier("b".into()),
                &Token::Eof,
            ]
        );
    }

    #[test]
    fn tokenize_triple_quoted_string() {
        let result = tokens("\"\"\"hello\nworld\"\"\"");
        assert_eq!(result, vec![Token::Text("hello\nworld".into())]);
    }

    #[test]
    fn tokenize_a_as_identifier_when_not_type() {
        let result = tokens("a + b");
        assert_eq!(
            result,
            vec![
                Token::Identifier("a".into()),
                Token::Plus,
                Token::Identifier("b".into()),
            ]
        );
    }

    #[test]
    fn tokenize_escape_sequences() {
        let result = tokens(r#""hello\nworld""#);
        assert_eq!(result, vec![Token::Text("hello\nworld".into())]);
    }
}
