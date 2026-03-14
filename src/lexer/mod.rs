/// Lexer module for the Clarity language.
///
/// Tokenizes source text into a stream of `SpannedToken` values.
pub mod scanner;
pub mod token;

pub use scanner::scan;
pub use token::{Span, SpannedToken, Token};
