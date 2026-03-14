/// Scope-chained environment for variable bindings.
///
/// Uses `Rc<RefCell<>>` interiors for shared mutable access across closures.
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

use crate::errors::{ClarityError, ErrorCode, Severity, SourceLocation};
use crate::interpreter::value::Value;

/// A reference-counted, mutable environment.
pub type Env = Rc<RefCell<Environment>>;

/// A scope in the environment chain, mapping names to (value, is_mutable) pairs.
#[derive(Debug, Clone)]
pub struct Environment {
    bindings: HashMap<String, (Value, bool)>,
    parent: Option<Env>,
}

impl Environment {
    /// Create a new root environment with no parent.
    pub fn new() -> Env {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent: None,
        }))
    }

    /// Create a new child environment with the given parent.
    pub fn with_parent(parent: &Env) -> Env {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent: Some(Rc::clone(parent)),
        }))
    }

    /// Define a new binding in this scope.
    pub fn define(&mut self, name: String, value: Value, mutable: bool) {
        self.bindings.insert(name, (value, mutable));
    }

    /// Look up a binding by name, walking up the scope chain.
    pub fn get(&self, name: &str) -> Option<(Value, bool)> {
        if let Some(binding) = self.bindings.get(name) {
            Some(binding.clone())
        } else if let Some(parent) = &self.parent {
            parent.borrow().get(name)
        } else {
            None
        }
    }

    /// Reassign a mutable binding. Returns an error if the name is not found
    /// or the binding is immutable.
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), ClarityError> {
        if let Some(binding) = self.bindings.get_mut(name) {
            if !binding.1 {
                return Err(ClarityError {
                    code: ErrorCode::ImmutableReassign,
                    severity: Severity::Error,
                    location: SourceLocation::unknown(),
                    message: format!("Cannot reassign immutable variable '{name}'"),
                    context: String::new(),
                    suggestion: format!("Declare '{name}' with 'mutable' to allow reassignment"),
                });
            }
            binding.0 = value;
            Ok(())
        } else if let Some(parent) = &self.parent {
            parent.borrow_mut().set(name, value)
        } else {
            Err(ClarityError {
                code: ErrorCode::UndefinedVariable,
                severity: Severity::Error,
                location: SourceLocation::unknown(),
                message: format!("Undefined variable '{name}'"),
                context: String::new(),
                suggestion: format!("Define '{name}' with 'let' or 'mutable' before using it"),
            })
        }
    }
}
