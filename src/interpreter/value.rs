/// Runtime value types for the Legible interpreter.
use std::fmt;
use std::rc::Rc;

use crate::errors::LegibleError;
use crate::interpreter::environment::Env;
use crate::parser::arena::Arena;
use crate::parser::ast::{NodeId, Param};

/// A runtime value in Legible.
#[derive(Debug, Clone)]
pub enum Value {
    Integer(i64),
    Decimal(f64),
    Text(String),
    Boolean(bool),
    None,
    List(Vec<Value>),
    Mapping(Vec<(Value, Value)>),
    Record {
        type_name: String,
        fields: Vec<(String, Value)>,
    },
    UnionVariant {
        type_name: String,
        variant_name: String,
        fields: Vec<(String, Value)>,
    },
    Function(Callable),
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Integer(n) => write!(f, "{n}"),
            Self::Decimal(n) => {
                // Format without trailing zeros
                if *n == n.floor() && n.is_finite() {
                    write!(f, "{}", *n as i64)
                } else {
                    write!(f, "{n}")
                }
            }
            Self::Text(s) => write!(f, "{s}"),
            Self::Boolean(b) => write!(f, "{b}"),
            Self::None => write!(f, "none"),
            Self::List(items) => {
                write!(f, "[")?;
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    match item {
                        Self::Text(s) => write!(f, "\"{s}\"")?,
                        _ => write!(f, "{item}")?,
                    }
                }
                write!(f, "]")
            }
            Self::Mapping(entries) => {
                write!(f, "{{")?;
                for (i, (k, v)) in entries.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{k}: {v}")?;
                }
                write!(f, "}}")
            }
            Self::Record { type_name, fields } => {
                write!(f, "{type_name} {{ ")?;
                for (i, (name, val)) in fields.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{name}: {val}")?;
                }
                write!(f, " }}")
            }
            Self::UnionVariant {
                type_name,
                variant_name,
                fields,
            } => {
                write!(f, "{type_name}.{variant_name}")?;
                if !fields.is_empty() {
                    write!(f, " {{ ")?;
                    for (i, (name, val)) in fields.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{name}: {val}")?;
                    }
                    write!(f, " }}")?;
                }
                Ok(())
            }
            Self::Function(_) => write!(f, "<function>"),
        }
    }
}

impl PartialEq for Value {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Integer(a), Self::Integer(b)) => a == b,
            (Self::Decimal(a), Self::Decimal(b)) => a == b,
            (Self::Text(a), Self::Text(b)) => a == b,
            (Self::Boolean(a), Self::Boolean(b)) => a == b,
            (Self::None, Self::None) => true,
            (Self::List(a), Self::List(b)) => a == b,
            (Self::Mapping(a), Self::Mapping(b)) => a == b,
            (Self::Record { type_name: t1, fields: f1 }, Self::Record { type_name: t2, fields: f2 }) => {
                t1 == t2 && f1 == f2
            }
            (
                Self::UnionVariant { type_name: t1, variant_name: v1, fields: f1 },
                Self::UnionVariant { type_name: t2, variant_name: v2, fields: f2 },
            ) => t1 == t2 && v1 == v2 && f1 == f2,
            _ => false,
        }
    }
}

/// A callable value — user-defined function, lambda, or builtin.
///
/// UserDefined and Lambda variants carry an `Rc<Arena>` so that functions
/// loaded from different modules can reference their own AST nodes.
#[derive(Clone)]
pub enum Callable {
    UserDefined {
        name: String,
        params: Vec<Param>,
        intent: String,
        requires: Vec<NodeId>,
        ensures: Vec<NodeId>,
        body: Vec<NodeId>,
        closure_env: Env,
        source_arena: Rc<Arena>,
    },
    Lambda {
        params: Vec<Param>,
        body: NodeId,
        closure_env: Env,
        source_arena: Rc<Arena>,
    },
    Builtin {
        name: String,
        func: fn(&[Value]) -> Result<Value, LegibleError>,
    },
}

impl Callable {
    /// Get the arena that contains this callable's AST nodes.
    pub fn arena(&self) -> Option<&Arena> {
        match self {
            Self::UserDefined { source_arena, .. } | Self::Lambda { source_arena, .. } => {
                Some(source_arena.as_ref())
            }
            Self::Builtin { .. } => None,
        }
    }
}

impl fmt::Debug for Callable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UserDefined { name, .. } => write!(f, "<function {name}>"),
            Self::Lambda { .. } => write!(f, "<lambda>"),
            Self::Builtin { name, .. } => write!(f, "<builtin {name}>"),
        }
    }
}
