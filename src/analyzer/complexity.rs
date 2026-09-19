//! Comprehension-budget analysis for Legible function bodies.
//!
//! Legible caps how much a single function may ask of its reader. The cap is
//! **not** a line count: counting newlines is trivially defeated by packing
//! statements onto one long line, and an LLM writing Legible has no reason to
//! respect a limit it can dodge by reformatting. Instead each function body is
//! measured on four dimensions that the software-engineering and cognitive
//! psychology literature ties to comprehension effort, and each measurement is
//! normalised against a threshold taken from the paper that established it.
//!
//! # The four dimensions
//!
//! 1. **Halstead volume** — `V = (N1 + N2) * log2(eta1 + eta2)`, the size of the
//!    function in *mental discriminations* rather than in newlines
//!    (Halstead, *Elements of Software Science*, 1977). Volume counts operators
//!    and operands, so it is invariant under reformatting: joining ten
//!    statements onto one physical line does not change it. This is the
//!    dimension that replaces the old line count. The ceiling of 1000 bits is a
//!    widely used industry guideline for a single function (it is the upper
//!    bound in the Halstead guidance of common static-analysis tools such as
//!    Verifysoft's Testwell CMT++); it is not a figure from Halstead's own
//!    book. Volume also feeds the Maintainability Index (Oman & Hagemeister,
//!    1992; Coleman, Ash, Lowther & Oman, IEEE Computer 27(8), 1994),
//!    `MI = 171 - 5.2*ln(V) - 0.23*G - 16.2*ln(LOC)`, where it dominates the
//!    size term.
//!
//! 2. **Cyclomatic complexity** — the number of linearly independent paths
//!    through the body (McCabe, *A Complexity Measure*, IEEE TSE SE-2(4), 1976).
//!    McCabe's paper proposes 10 as the practical upper bound for a single
//!    module, and NIST SP 500-235 (Watson & McCabe, *Structured Testing*, 1996)
//!    reaffirms it. Paths are what a reader — or an agent writing a test — has
//!    to enumerate, and the count is independent of how the source is wrapped.
//!    Only control-flow decisions are counted, as in McCabe's own formulation;
//!    boolean operators are left to cognitive complexity, which is where
//!    Campbell's metric deliberately puts them. Counting them here as well
//!    would inflate the measurement past what the threshold of 10 was set
//!    against.
//!
//! 3. **Cognitive complexity** — nesting-weighted control flow (Campbell,
//!    *Cognitive Complexity: An Overview and Evaluation*, TechDebt '18). Unlike
//!    cyclomatic complexity it charges more for a branch inside a loop inside a
//!    branch than for three sibling branches, which is what actually costs a
//!    reader. Muñoz Barón, Wyrich & Wagner (*An Empirical Validation of
//!    Cognitive Complexity as a Measure of Source Code Understandability*,
//!    ESEM '20) found it correlates with measured comprehension time across
//!    published studies. The threshold of 15 is the one Campbell recommends and
//!    the value the metric was validated at.
//!
//! 4. **Peak live bindings** — the largest number of named values that are
//!    simultaneously live (declared, and still read later) at any point in the
//!    body. This is the *live variables* metric of Conte, Dunsmore & Shen
//!    (*Software Engineering Metrics and Models*, 1986). The ceiling of 9 is
//!    the upper bound of Miller's 7±2 (*The Magical Number Seven, Plus or Minus
//!    Two*, Psychological Review 63(2), 1956). Cowan's revision puts pure
//!    short-term capacity nearer 4 (*The magical number 4 in short-term
//!    memory*, BBS 24(1), 2001), but code can be re-read, and skilled
//!    comprehenders recover encoded chunks from long-term working memory
//!    (Ericsson & Kintsch, Psychological Review 102(2), 1995), so the looser
//!    bound of the pair is the honest one for a written artefact.
//!
//! # The budget
//!
//! A function's *comprehension load* is the largest of the four normalised
//! measurements. A load of `1.0` means the function sits exactly on a published
//! threshold; above `1.0` it has exceeded one, and `E_FUNCTION_TOO_LONG` is
//! reported naming the dimension that was blown and by how much. No constant in
//! this module is chosen to make a particular program pass: each is the figure
//! its own source recommends.
//!
//! # Why this shape suits an LLM author
//!
//! Every dimension is measured on the AST, so none of them can be gamed by
//! reformatting — the failure mode that motivated replacing the line count.
//! Halstead volume is a near-linear proxy for the number of tokens an agent
//! must produce, or later re-read, to understand one function; degradation of
//! model performance on long inputs is documented (Liu et al., *Lost in the
//! Middle: How Language Models Use Long Contexts*, TACL 2024), though that
//! work measures reading rather than writing code, so it motivates keeping
//! functions compact rather than fixing the threshold. Cyclomatic and
//! cognitive complexity bound how much branch state
//! must be carried while writing the tail of a function, and live bindings
//! bound how many names must stay resolved. Diagnostics report each raw
//! measurement so the fix is mechanical rather than guesswork.

use std::collections::HashMap;

use crate::parser::arena::Arena;
use crate::parser::ast::{BinaryOperator, MatchArm, NodeId, NodeKind, Param, Pattern, TextPart};

/// Maximum Halstead volume, in bits, for one function body.
///
/// Common industry guideline for a single function; see the module docs.
pub const MAX_HALSTEAD_VOLUME: f64 = 1000.0;

/// Maximum cyclomatic complexity for one function body.
///
/// `McCabe` (1976); reaffirmed by NIST SP 500-235 (1996).
pub const MAX_CYCLOMATIC_COMPLEXITY: u32 = 10;

/// Maximum cognitive complexity for one function body.
///
/// Campbell (2018); validated against comprehension time by Muñoz Barón,
/// Wyrich & Wagner (2020).
pub const MAX_COGNITIVE_COMPLEXITY: u32 = 15;

/// Maximum number of simultaneously live named bindings in one function body.
///
/// Live-variables metric of Conte, Dunsmore & Shen (1986); bound from the upper
/// end of Miller's 7±2 (1956).
pub const MAX_LIVE_BINDINGS: u32 = 9;

/// The dimension of the comprehension budget that a function spent the most of.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dimension {
    /// Halstead volume: the function says too much.
    Volume,
    /// Cyclomatic complexity: the function has too many independent paths.
    Paths,
    /// Cognitive complexity: the function nests control flow too deeply.
    Nesting,
    /// Peak live bindings: the function juggles too many names at once.
    Bindings,
}

impl Dimension {
    /// Human-readable name of the dimension, used in diagnostics.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Dimension::Volume => "Halstead volume",
            Dimension::Paths => "cyclomatic complexity",
            Dimension::Nesting => "cognitive complexity",
            Dimension::Bindings => "peak live bindings",
        }
    }

    /// The published source the threshold for this dimension comes from.
    #[must_use]
    pub fn citation(self) -> &'static str {
        match self {
            Dimension::Volume => "Halstead 1977; common industry guideline",
            Dimension::Paths => "McCabe 1976; NIST SP 500-235",
            Dimension::Nesting => "Campbell 2018; Munoz Baron et al. 2020",
            Dimension::Bindings => "Miller 1956; Conte, Dunsmore & Shen 1986",
        }
    }

    /// Concrete advice for bringing this dimension back under budget.
    #[must_use]
    pub fn remedy(self) -> &'static str {
        match self {
            Dimension::Volume => {
                "Extract a cohesive group of statements into its own function with its own intent. \
                 Note that this limit counts operators and operands, not lines, so reformatting \
                 the body onto fewer or longer lines will not change it."
            }
            Dimension::Paths => {
                "Replace nested conditionals with a single match, or move each branch's work into \
                 its own function, so this function enumerates fewer independent paths."
            }
            Dimension::Nesting => {
                "Flatten the deepest nesting: lift an inner loop or conditional into a named \
                 function, or express the traversal as a filter/map pipeline instead."
            }
            Dimension::Bindings => {
                "Too many named values are live at once. Move a self-contained group of let \
                 bindings and the code that consumes them into a helper function, or build a \
                 record so related values travel as one name."
            }
        }
    }
}

/// The measured comprehension cost of a single function body.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FunctionComplexity {
    /// Halstead volume `(N1 + N2) * log2(eta1 + eta2)` in bits.
    pub halstead_volume: f64,
    /// Total operator occurrences (`N1`).
    pub total_operators: usize,
    /// Total operand occurrences (`N2`).
    pub total_operands: usize,
    /// Distinct operators (`eta1`).
    pub distinct_operators: usize,
    /// Distinct operands (`eta2`).
    pub distinct_operands: usize,
    /// Cyclomatic complexity (`McCabe`), over control-flow decisions only.
    pub cyclomatic: u32,
    /// Cognitive complexity (Campbell), nesting-weighted.
    pub cognitive: u32,
    /// Largest number of simultaneously live named bindings.
    pub peak_live_bindings: u32,
}

impl FunctionComplexity {
    /// Fraction of the published threshold spent on each dimension, paired with
    /// the dimension it belongs to.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn normalized(&self) -> [(Dimension, f64); 4] {
        [
            (
                Dimension::Volume,
                self.halstead_volume / MAX_HALSTEAD_VOLUME,
            ),
            (
                Dimension::Paths,
                f64::from(self.cyclomatic) / f64::from(MAX_CYCLOMATIC_COMPLEXITY),
            ),
            (
                Dimension::Nesting,
                f64::from(self.cognitive) / f64::from(MAX_COGNITIVE_COMPLEXITY),
            ),
            (
                Dimension::Bindings,
                f64::from(self.peak_live_bindings) / f64::from(MAX_LIVE_BINDINGS),
            ),
        ]
    }

    /// The comprehension load of the function: the largest normalised
    /// measurement. `1.0` is exactly on budget; anything above is over.
    #[must_use]
    pub fn load(&self) -> f64 {
        self.normalized()
            .iter()
            .fold(0.0_f64, |worst, &(_, value)| worst.max(value))
    }

    /// The dimension responsible for the load, and the load itself.
    #[must_use]
    pub fn dominant(&self) -> (Dimension, f64) {
        self.normalized().iter().fold(
            (Dimension::Volume, f64::NEG_INFINITY),
            |worst, &(dimension, value)| {
                if value > worst.1 {
                    (dimension, value)
                } else {
                    worst
                }
            },
        )
    }

    /// True when the function is within the comprehension budget.
    #[must_use]
    pub fn within_budget(&self) -> bool {
        self.load() <= 1.0
    }

    /// The raw measurement behind a dimension, rendered against its threshold.
    #[must_use]
    pub fn measurement(&self, dimension: Dimension) -> String {
        match dimension {
            Dimension::Volume => format!(
                "{:.0} of {MAX_HALSTEAD_VOLUME:.0} bits ({} operators and {} operands over {} distinct symbols)",
                self.halstead_volume,
                self.total_operators,
                self.total_operands,
                self.distinct_operators + self.distinct_operands,
            ),
            Dimension::Paths => format!("{} of {MAX_CYCLOMATIC_COMPLEXITY}", self.cyclomatic),
            Dimension::Nesting => format!("{} of {MAX_COGNITIVE_COMPLEXITY}", self.cognitive),
            Dimension::Bindings => {
                format!("{} of {MAX_LIVE_BINDINGS}", self.peak_live_bindings)
            }
        }
    }
}

/// Measure the comprehension cost of a function body.
///
/// `params` are counted as bindings that are live from function entry, and as
/// operands, because a reader carries them for the whole body.
#[must_use]
pub fn measure_function(arena: &Arena, params: &[Param], body: &[NodeId]) -> FunctionComplexity {
    let mut collector = Collector::default();
    for param in params {
        collector.declare(&param.name);
    }
    for &statement in body {
        collector.visit(arena, statement, 0, None);
    }
    collector.finish()
}

/// Accumulates the four measurements during a single AST walk.
#[derive(Default)]
struct Collector {
    operators: HashMap<String, usize>,
    operands: HashMap<String, usize>,
    total_operators: usize,
    total_operands: usize,
    cyclomatic_decisions: u32,
    cognitive: u32,
    /// Monotonic position in the walk, standing in for "time" during the read.
    clock: u32,
    /// First declaration position of each named binding.
    declared_at: HashMap<String, u32>,
    /// Last position at which each named binding is read.
    last_read_at: HashMap<String, u32>,
}

impl Collector {
    fn operator(&mut self, symbol: &str) {
        self.total_operators += 1;
        *self.operators.entry(symbol.to_string()).or_insert(0) += 1;
    }

    fn operand(&mut self, symbol: &str) {
        self.total_operands += 1;
        *self.operands.entry(symbol.to_string()).or_insert(0) += 1;
    }

    fn tick(&mut self) -> u32 {
        self.clock += 1;
        self.clock
    }

    fn declare(&mut self, name: &str) {
        let now = self.tick();
        self.declared_at.entry(name.to_string()).or_insert(now);
        self.last_read_at
            .entry(name.to_string())
            .and_modify(|at| *at = now)
            .or_insert(now);
    }

    fn read(&mut self, name: &str) {
        let now = self.tick();
        if self.declared_at.contains_key(name) {
            self.last_read_at.insert(name.to_string(), now);
        }
    }

    /// Peak number of bindings whose live range covers the same position.
    fn peak_live_bindings(&self) -> u32 {
        let mut events: Vec<(u32, i32)> = Vec::with_capacity(self.declared_at.len() * 2);
        for (name, &start) in &self.declared_at {
            let end = self.last_read_at.get(name).copied().unwrap_or(start);
            events.push((start, 1));
            events.push((end + 1, -1));
        }
        // Closing a range before opening another at the same instant keeps a
        // binding that dies exactly where the next is born from double-counting.
        events.sort_unstable_by_key(|&(position, delta)| (position, delta));
        let mut live = 0_i32;
        let mut peak = 0_i32;
        for (_, delta) in events {
            live += delta;
            peak = peak.max(live);
        }
        u32::try_from(peak).unwrap_or(u32::MAX)
    }

    #[allow(clippy::cast_precision_loss)]
    fn finish(self) -> FunctionComplexity {
        let peak_live_bindings = self.peak_live_bindings();
        let vocabulary = self.operators.len() + self.operands.len();
        let length = self.total_operators + self.total_operands;
        let halstead_volume = if vocabulary <= 1 {
            0.0
        } else {
            length as f64 * (vocabulary as f64).log2()
        };
        FunctionComplexity {
            halstead_volume,
            total_operators: self.total_operators,
            total_operands: self.total_operands,
            distinct_operators: self.operators.len(),
            distinct_operands: self.operands.len(),
            cyclomatic: self.cyclomatic_decisions + 1,
            cognitive: self.cognitive,
            peak_live_bindings,
        }
    }

    /// Walk one node.
    ///
    /// One arm per `NodeKind`; splitting it would only scatter the dispatch.
    ///
    /// `nesting` is the cognitive-complexity nesting level. `enclosing_boolean`
    /// carries the logical operator of the parent node so that a run of like
    /// operators (`a and b and c`) is charged once, as Campbell specifies.
    #[allow(clippy::too_many_lines)]
    fn visit(
        &mut self,
        arena: &Arena,
        node: NodeId,
        nesting: u32,
        enclosing_boolean: Option<&BinaryOperator>,
    ) {
        match &arena.get(node).kind {
            NodeKind::Program { statements } => {
                for &statement in statements {
                    self.visit(arena, statement, nesting, None);
                }
            }
            NodeKind::UseDecl { module_name } => {
                self.operator("use");
                self.operand(module_name);
            }
            NodeKind::FunctionDecl { name, body, .. } => {
                // Nested declarations are measured on their own; here they only
                // contribute their name.
                self.operator("function");
                self.operand(name);
                for &statement in body {
                    self.visit(arena, statement, nesting + 1, None);
                }
            }
            NodeKind::RecordDecl { name, .. } | NodeKind::UnionDecl { name, .. } => {
                self.operator("type");
                self.operand(name);
            }

            NodeKind::LetBinding {
                name,
                value,
                mutable,
                ..
            } => {
                self.operator(if *mutable { "mutable" } else { "let" });
                self.visit(arena, *value, nesting, None);
                self.declare(name);
                self.operand(name);
            }
            NodeKind::SetStatement { name, value } => {
                self.operator("set");
                self.visit(arena, *value, nesting, None);
                self.read(name);
                self.operand(name);
            }
            NodeKind::ForLoop {
                binding,
                iterable,
                body,
            } => {
                self.operator("for");
                self.cyclomatic_decisions += 1;
                self.cognitive += 1 + nesting;
                self.visit(arena, *iterable, nesting, None);
                self.declare(binding);
                self.operand(binding);
                for &statement in body {
                    self.visit(arena, statement, nesting + 1, None);
                }
            }
            NodeKind::WhileLoop { condition, body } => {
                self.operator("while");
                self.cyclomatic_decisions += 1;
                self.cognitive += 1 + nesting;
                self.visit(arena, *condition, nesting, None);
                for &statement in body {
                    self.visit(arena, statement, nesting + 1, None);
                }
            }
            NodeKind::ReturnExpr { value } => {
                self.operator("return");
                if let Some(inner) = value {
                    self.visit(arena, *inner, nesting, None);
                }
            }
            NodeKind::ExprStatement { expr } => self.visit(arena, *expr, nesting, None),

            NodeKind::IntegerLit(literal) => self.operand(&literal.to_string()),
            NodeKind::DecimalLit(literal) => self.operand(&literal.to_string()),
            NodeKind::TextLit(literal) => self.operand(&format!("\"{literal}\"")),
            NodeKind::BooleanLit(literal) => self.operand(&literal.to_string()),
            NodeKind::NoneLit => self.operand("none"),
            NodeKind::InterpolatedText { parts } => {
                self.operator("interpolate");
                for part in parts {
                    match part {
                        TextPart::Literal(literal) => self.operand(&format!("\"{literal}\"")),
                        TextPart::Interpolation(inner) => self.visit(arena, *inner, nesting, None),
                    }
                }
            }
            NodeKind::ListLit { elements } => {
                self.operator("[]");
                for &element in elements {
                    self.visit(arena, element, nesting, None);
                }
            }
            NodeKind::MappingLit { entries } => {
                self.operator("{}");
                for &(key, value) in entries {
                    self.visit(arena, key, nesting, None);
                    self.visit(arena, value, nesting, None);
                }
            }
            NodeKind::Identifier(name) => {
                self.read(name);
                self.operand(name);
            }
            NodeKind::FieldAccess { object, field } => {
                self.operator(".");
                self.visit(arena, *object, nesting, None);
                self.operand(field);
            }
            NodeKind::FunctionCall { callee, arguments } => {
                self.operator("()");
                self.visit(arena, *callee, nesting, None);
                for &argument in arguments {
                    self.visit(arena, argument, nesting, None);
                }
            }
            NodeKind::Lambda { params, body, .. } => {
                self.operator("fn");
                for param in params {
                    self.declare(&param.name);
                    self.operand(&param.name);
                }
                // A lambda is not itself a branch, but its body reads as nested.
                self.visit(arena, *body, nesting + 1, None);
            }
            NodeKind::Pipeline { left, right } => {
                self.operator("|>");
                self.visit(arena, *left, nesting, None);
                self.visit(arena, *right, nesting, None);
            }
            NodeKind::BinaryOp { left, op, right } => {
                self.operator(binary_symbol(op));
                if matches!(op, BinaryOperator::And | BinaryOperator::Or) {
                    // Campbell charges one point per run of like operators, so
                    // only the outermost member of a run is counted.
                    if enclosing_boolean != Some(op) {
                        self.cognitive += 1;
                    }
                    self.visit(arena, *left, nesting, Some(op));
                    self.visit(arena, *right, nesting, Some(op));
                } else {
                    self.visit(arena, *left, nesting, None);
                    self.visit(arena, *right, nesting, None);
                }
            }
            NodeKind::UnaryOp { op, operand } => {
                self.operator(unary_symbol(op));
                self.visit(arena, *operand, nesting, None);
            }
            NodeKind::IfExpr {
                condition,
                then_branch,
                else_branch,
            } => {
                self.operator("if");
                self.cyclomatic_decisions += 1;
                self.cognitive += 1 + nesting;
                self.visit(arena, *condition, nesting, None);
                for &statement in then_branch {
                    self.visit(arena, statement, nesting + 1, None);
                }
                if let Some(alternative) = else_branch {
                    self.visit_else(arena, alternative, nesting);
                }
            }
            NodeKind::MatchExpr { subject, arms } => {
                self.operator("match");
                // A match with n arms offers n independent paths, hence n - 1
                // decisions on top of the single path every body already has.
                self.cyclomatic_decisions +=
                    u32::try_from(arms.len()).unwrap_or(u32::MAX).max(1) - 1;
                self.cognitive += 1 + nesting;
                self.visit(arena, *subject, nesting, None);
                for arm in arms {
                    self.visit_arm(arena, arm, nesting);
                }
            }
            NodeKind::RecordConstruct { type_name, fields } => {
                self.operator("construct");
                self.operand(type_name);
                for (field, value) in fields {
                    self.operand(field);
                    self.visit(arena, *value, nesting, None);
                }
            }
            NodeKind::RecordUpdate { base, updates } => {
                self.operator("with");
                self.visit(arena, *base, nesting, None);
                for (field, value) in updates {
                    self.operand(field);
                    self.visit(arena, *value, nesting, None);
                }
            }
            NodeKind::UnionConstruct {
                type_name,
                variant_name,
                fields,
            } => {
                self.operator("construct");
                self.operand(&format!("{type_name}.{variant_name}"));
                for (field, value) in fields {
                    self.operand(field);
                    self.visit(arena, *value, nesting, None);
                }
            }
            NodeKind::OldExpr { inner } => {
                self.operator("old");
                self.visit(arena, *inner, nesting, None);
            }
        }
    }

    /// Walk an `else` branch.
    ///
    /// A lone `if` in the `else` position is an `else if`: Campbell charges it a
    /// flat point with no nesting penalty, because a chain reads as one
    /// decision table rather than as growing indentation.
    fn visit_else(&mut self, arena: &Arena, alternative: &[NodeId], nesting: u32) {
        if let [only] = alternative {
            if let NodeKind::IfExpr {
                condition,
                then_branch,
                else_branch,
            } = &arena.get(*only).kind
            {
                self.operator("if");
                self.cyclomatic_decisions += 1;
                self.cognitive += 1;
                self.visit(arena, *condition, nesting, None);
                for &statement in then_branch {
                    self.visit(arena, statement, nesting + 1, None);
                }
                if let Some(tail) = else_branch {
                    self.visit_else(arena, tail, nesting);
                }
                return;
            }
        }
        self.operator("else");
        self.cognitive += 1;
        for &statement in alternative {
            self.visit(arena, statement, nesting + 1, None);
        }
    }

    /// Walk one match arm, binding any destructured names.
    fn visit_arm(&mut self, arena: &Arena, arm: &MatchArm, nesting: u32) {
        self.operator("when");
        match &arm.pattern {
            Pattern::Literal(literal) => self.visit(arena, *literal, nesting, None),
            Pattern::Variant { name, bindings } => {
                self.operand(name);
                for binding in bindings {
                    self.declare(binding);
                    self.operand(binding);
                }
            }
            Pattern::Otherwise => self.operand("otherwise"),
        }
        for &statement in &arm.body {
            self.visit(arena, statement, nesting + 1, None);
        }
    }
}

/// Halstead operator symbol for a binary operator.
fn binary_symbol(op: &BinaryOperator) -> &'static str {
    match op {
        BinaryOperator::Add => "+",
        BinaryOperator::Sub => "-",
        BinaryOperator::Mul => "*",
        BinaryOperator::Div => "/",
        BinaryOperator::Mod => "%",
        BinaryOperator::Concat => "++",
        BinaryOperator::Eq => "==",
        BinaryOperator::NotEq => "!=",
        BinaryOperator::Gt => ">",
        BinaryOperator::Lt => "<",
        BinaryOperator::GtEq => ">=",
        BinaryOperator::LtEq => "<=",
        BinaryOperator::And => "and",
        BinaryOperator::Or => "or",
    }
}

/// Halstead operator symbol for a unary operator.
fn unary_symbol(op: &crate::parser::ast::UnaryOperator) -> &'static str {
    match op {
        crate::parser::ast::UnaryOperator::Negate => "neg",
        crate::parser::ast::UnaryOperator::Not => "not",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::ast::NodeKind;

    /// Measure the first function declared in `source`.
    fn measure(source: &str) -> FunctionComplexity {
        let tokens = crate::lexer::scan(source).expect("source should tokenize");
        let mut parser = crate::parser::Parser::new(tokens, "<test>", source);
        let root = parser.parse_program().expect("source should parse");
        let arena = &parser.arena;
        let NodeKind::Program { ref statements } = arena.get(root).kind else {
            panic!("root should be a program");
        };
        for &statement in statements {
            if let NodeKind::FunctionDecl {
                ref params,
                ref body,
                ..
            } = arena.get(statement).kind
            {
                return measure_function(arena, params, body);
            }
        }
        panic!("source should declare a function");
    }

    /// The whole point of measuring the AST rather than newlines: an agent
    /// cannot buy itself more budget by packing the body onto fewer lines.
    #[test]
    fn line_packing_does_not_change_the_measurement() {
        let spread = "\
function pick(numbers: a list of integer): a list of text
  intent: filter numbers and format the survivors as text
  numbers
    |> filter(fn(n: integer): boolean => n > 10)
    |> take(5)
    |> map(fn(n: integer): text => to_text(n))
end
";
        let packed = "\
function pick(numbers: a list of integer): a list of text
  intent: filter numbers and format the survivors as text
  numbers |> filter(fn(n: integer): boolean => n > 10) |> take(5) |> map(fn(n: integer): text => to_text(n))
end
";
        assert_eq!(measure(spread), measure(packed));
    }

    #[test]
    fn a_small_function_is_well_within_budget() {
        let complexity = measure(
            "\
function greet(name: text): text
  intent: produce a greeting string that includes the given name
  return \"Hello, \" ++ name
end
",
        );
        assert!(
            complexity.load() < 0.25,
            "a three-line function should barely touch the budget, got {:.2}",
            complexity.load()
        );
        assert_eq!(complexity.cyclomatic, 1);
        assert_eq!(complexity.cognitive, 0);
    }

    /// An `else if` chain reads as one decision table, so Campbell charges each
    /// arm a flat point rather than a growing nesting penalty.
    #[test]
    fn an_else_if_chain_does_not_accumulate_nesting() {
        let complexity = measure(
            "\
function classify(n: integer): text
  intent: return fizz buzz or the number as text
  if n % 15 == 0 then
    \"FizzBuzz\"
  else if n % 3 == 0 then
    \"Fizz\"
  else if n % 5 == 0 then
    \"Buzz\"
  else
    to_text(n)
  end
end
",
        );
        // if (1) + three else-branch points, with no nesting surcharge.
        assert_eq!(complexity.cognitive, 4);
        assert_eq!(complexity.cyclomatic, 4);
        assert!(complexity.within_budget());
    }

    /// Nested control flow costs more than the same number of sibling branches,
    /// which is the difference between cognitive and cyclomatic complexity.
    #[test]
    fn nesting_costs_more_than_sequence() {
        let nested = measure(
            "\
function scan(rows: a list of integer, columns: a list of integer): integer
  intent: count matching pairs across two ranges
  mutable total: integer = 0
  for row in rows do
    for column in columns do
      if row > column then
        if row % column == 0 then
          set total = total + 1
        end
      end
    end
  end
  return total
end
",
        );
        assert!(
            nested.cognitive > nested.cyclomatic,
            "nesting should outweigh the raw path count, got {} vs {}",
            nested.cognitive,
            nested.cyclomatic
        );
        // for(+1) + for(+2) + if(+3) + if(+4), exactly Campbell's arithmetic.
        assert_eq!(nested.cognitive, 10);
        assert_eq!(nested.dominant().0, Dimension::Nesting);
    }

    /// Carrying more names than working memory holds is over budget even when
    /// the function is short and branchless.
    #[test]
    fn too_many_live_bindings_is_over_budget() {
        let complexity = measure(
            "\
function tally(a: integer, b: integer, c: integer, d: integer, e: integer, f: integer, g: integer, h: integer, i: integer, j: integer, k: integer): integer
  intent: add every argument together
  return a + b + c + d + e + f + g + h + i + j + k
end
",
        );
        assert_eq!(complexity.peak_live_bindings, 11);
        assert_eq!(complexity.dominant().0, Dimension::Bindings);
        assert!(!complexity.within_budget());
    }

    /// A binding that is consumed immediately does not stay live, so a long
    /// chain of short-lived steps is not penalised.
    #[test]
    fn short_lived_bindings_do_not_accumulate() {
        let complexity = measure(
            "\
function normalize(raw: text): text
  intent: trim and lowercase the text then replace spaces with dashes
  let trimmed: text = trim(raw)
  let lowered: text = lowercase(trimmed)
  let dashed: text = replace(lowered, \" \", \"-\")
  return dashed
end
",
        );
        assert!(
            complexity.peak_live_bindings <= 3,
            "sequential single-use bindings should not pile up, got {}",
            complexity.peak_live_bindings
        );
        assert!(complexity.within_budget());
    }

    #[test]
    fn every_dimension_carries_a_citation_and_a_remedy() {
        for dimension in [
            Dimension::Volume,
            Dimension::Paths,
            Dimension::Nesting,
            Dimension::Bindings,
        ] {
            assert!(!dimension.label().is_empty());
            assert!(!dimension.citation().is_empty());
            assert!(!dimension.remedy().is_empty());
        }
    }
}
