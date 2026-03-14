/// Parser module for the Legible language.
///
/// Converts a token stream into an arena-allocated AST.
pub mod arena;
pub mod ast;
pub mod parser;

pub use arena::Arena;
pub use ast::{
    AstNode, BinaryOperator, LegibleType, Field, MatchArm, NodeId, NodeKind, Param, Pattern,
    TextPart, UnaryOperator, Variant,
};
pub use parser::Parser;
