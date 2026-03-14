/// Type checking pass for the Legible language.
///
/// Walks the AST and verifies type consistency before evaluation.
/// This is a placeholder for Phase 4 — currently a no-op pass.
use crate::errors::LegibleError;
use crate::parser::arena::Arena;
use crate::parser::ast::NodeId;

/// Run the type checker on the given AST. Returns a list of errors/warnings.
pub fn typecheck(_arena: &Arena, _root: NodeId) -> Vec<LegibleError> {
    // Phase 4: implement full type checking
    Vec::new()
}
