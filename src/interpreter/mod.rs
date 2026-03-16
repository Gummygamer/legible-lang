/// Interpreter module for the Legible language.
///
/// Tree-walking evaluator with scope-chained environments.
pub mod builtins;
pub mod environment;
pub mod evaluator;
pub mod http_builtins;
pub mod io_builtins;
pub mod json_builtins;
pub mod sdl_builtins;
pub mod value;

pub use environment::{Env, Environment};
pub use evaluator::{evaluate_program, evaluate_program_rc};
pub use value::{Callable, Value};
