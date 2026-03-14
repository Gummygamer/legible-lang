/// Interpreter module for the Clarity language.
///
/// Tree-walking evaluator with scope-chained environments.
pub mod builtins;
pub mod environment;
pub mod evaluator;
pub mod value;

pub use environment::{Env, Environment};
pub use evaluator::evaluate_program;
pub use value::{Callable, Value};
