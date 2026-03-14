/// Typed arena for AST node allocation.
///
/// All AST nodes live in a `Vec<AstNode>` and are referenced by `NodeId`.
use crate::lexer::Span;
use crate::parser::ast::{AstNode, NodeId, NodeKind};

/// Arena allocator for AST nodes.
#[derive(Debug, Clone)]
pub struct Arena {
    nodes: Vec<AstNode>,
}

impl Arena {
    /// Create a new empty arena.
    pub fn new() -> Self {
        Self { nodes: Vec::new() }
    }

    /// Allocate a new node in the arena and return its `NodeId`.
    pub fn alloc(&mut self, kind: NodeKind, span: Span) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(AstNode { kind, span });
        id
    }

    /// Get a reference to the node at the given `NodeId`.
    ///
    /// # Panics
    ///
    /// Panics if the `NodeId` is out of bounds. This should never happen
    /// with well-formed ASTs produced by the parser.
    pub fn get(&self, id: NodeId) -> &AstNode {
        &self.nodes[id.0]
    }

    /// Get a mutable reference to the node at the given `NodeId`.
    pub fn get_mut(&mut self, id: NodeId) -> &mut AstNode {
        &mut self.nodes[id.0]
    }

    /// Return the number of nodes in the arena.
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// Return true if the arena is empty.
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Default for Arena {
    fn default() -> Self {
        Self::new()
    }
}
