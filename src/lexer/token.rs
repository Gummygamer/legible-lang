/// Token types and span information for the Clarity lexer.

/// Byte offset range into the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

/// A token together with its source span.
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

/// All token types in the Clarity language.
#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Integer(i64),
    Decimal(f64),
    Text(String),
    Boolean(bool),
    None,

    // Identifiers & keywords
    Identifier(String),
    Let,
    Mutable,
    Set,
    Function,
    Public,
    Return,
    If,
    Then,
    Else,
    End,
    Match,
    When,
    Otherwise,
    For,
    In,
    Do,
    While,
    Record,
    Union,
    Use,
    With,
    Intent,
    Requires,
    Ensures,
    And,
    Or,
    Not,
    Fn,

    // Types (keywords)
    IntegerType,
    DecimalType,
    TextType,
    BooleanType,
    NothingType,
    /// `a list of` as a single token.
    AListOf,
    /// `a mapping from` as a single token.
    AMappingFrom,
    /// Used in `a mapping from K to V`.
    To,
    /// `an optional` as a single token.
    AnOptional,

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    /// `++` string concatenation.
    PlusPlus,
    /// `|>` pipeline operator.
    Pipe,
    /// `==`
    Equals,
    /// `!=`
    NotEquals,
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    /// `=` assignment/initialization.
    Assign,
    /// `=>`
    Arrow,
    /// `?` optional unwrap.
    Question,
    Dot,
    Comma,
    Colon,

    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,

    // Special
    Comment(String),
    Newline,
    Eof,

    // String interpolation markers (internal use)
    /// Marks the start of an interpolated string.
    InterpolationStart,
    /// A literal segment of an interpolated string.
    InterpolationLiteral(String),
    /// Marks an interpolation expression boundary.
    InterpolationExprStart,
    /// Marks the end of an interpolation expression.
    InterpolationExprEnd,
    /// Marks the end of an interpolated string.
    InterpolationEnd,
}
