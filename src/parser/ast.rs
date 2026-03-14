/// AST node types for the Legible language.
///
/// All nodes are arena-allocated and referenced by `NodeId`.
use crate::lexer::Span;

/// Index into the AST arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

/// An AST node with its kind and source span.
#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: NodeKind,
    pub span: Span,
}

/// The different kinds of AST nodes.
#[derive(Debug, Clone)]
pub enum NodeKind {
    // Top-level
    Program {
        statements: Vec<NodeId>,
    },
    UseDecl {
        module_name: String,
    },
    FunctionDecl {
        name: String,
        params: Vec<Param>,
        return_type: LegibleType,
        intent: String,
        requires: Vec<NodeId>,
        ensures: Vec<NodeId>,
        body: Vec<NodeId>,
        is_public: bool,
    },
    RecordDecl {
        name: String,
        fields: Vec<Field>,
    },
    UnionDecl {
        name: String,
        variants: Vec<Variant>,
    },

    // Statements
    LetBinding {
        name: String,
        declared_type: LegibleType,
        value: NodeId,
        mutable: bool,
    },
    SetStatement {
        name: String,
        value: NodeId,
    },
    ForLoop {
        binding: String,
        iterable: NodeId,
        body: Vec<NodeId>,
    },
    WhileLoop {
        condition: NodeId,
        body: Vec<NodeId>,
    },
    ReturnExpr {
        value: Option<NodeId>,
    },
    ExprStatement {
        expr: NodeId,
    },

    // Expressions
    IntegerLit(i64),
    DecimalLit(f64),
    TextLit(String),
    InterpolatedText {
        parts: Vec<TextPart>,
    },
    BooleanLit(bool),
    NoneLit,
    ListLit {
        elements: Vec<NodeId>,
    },
    MappingLit {
        entries: Vec<(NodeId, NodeId)>,
    },
    Identifier(String),
    FieldAccess {
        object: NodeId,
        field: String,
    },
    FunctionCall {
        callee: NodeId,
        arguments: Vec<NodeId>,
    },
    Lambda {
        params: Vec<Param>,
        return_type: LegibleType,
        body: NodeId,
    },
    Pipeline {
        left: NodeId,
        right: NodeId,
    },
    BinaryOp {
        left: NodeId,
        op: BinaryOperator,
        right: NodeId,
    },
    UnaryOp {
        op: UnaryOperator,
        operand: NodeId,
    },
    IfExpr {
        condition: NodeId,
        then_branch: Vec<NodeId>,
        else_branch: Option<Vec<NodeId>>,
    },
    MatchExpr {
        subject: NodeId,
        arms: Vec<MatchArm>,
    },
    RecordConstruct {
        type_name: String,
        fields: Vec<(String, NodeId)>,
    },
    RecordUpdate {
        base: NodeId,
        updates: Vec<(String, NodeId)>,
    },
    UnionConstruct {
        type_name: String,
        variant_name: String,
        fields: Vec<(String, NodeId)>,
    },
    OldExpr {
        inner: NodeId,
    },
}

/// A function or lambda parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub param_type: LegibleType,
}

/// A record field definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: LegibleType,
}

/// A union variant definition.
#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,
}

/// A match arm with a pattern and body.
#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<NodeId>,
}

/// Pattern for match expressions.
#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Literal(NodeId),
    Variant {
        name: String,
        bindings: Vec<String>,
    },
    Otherwise,
}

/// A part of an interpolated string.
#[derive(Debug, Clone)]
pub enum TextPart {
    Literal(String),
    Interpolation(NodeId),
}

/// Legible type system representation.
#[derive(Debug, Clone, PartialEq)]
pub enum LegibleType {
    Integer,
    Decimal,
    Text,
    Boolean,
    Nothing,
    ListOf(Box<LegibleType>),
    MappingFrom(Box<LegibleType>, Box<LegibleType>),
    Optional(Box<LegibleType>),
    Named(String),
    Function {
        params: Vec<LegibleType>,
        return_type: Box<LegibleType>,
    },
    Generic(String),
}

/// Binary operators.
#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Concat,
    Eq,
    NotEq,
    Gt,
    Lt,
    GtEq,
    LtEq,
    And,
    Or,
}

/// Unary operators.
#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Negate,
    Not,
}
