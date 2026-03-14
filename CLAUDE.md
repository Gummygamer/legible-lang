# CLAUDE.md — Legible Language Interpreter

## Project Overview

You are building an **interpreter** for **Legible**, a programming language designed to be optimal for LLMs to write, read, and reason about. The interpreter is written in **Rust** (edition 2021, stable toolchain).

The project name is `legible-lang`. It is a single Cargo binary crate invoked as:

```bash
legible run <file.lbl>
legible check <file.lbl>     # typecheck + intent verification only
legible fmt <file.lbl>        # canonical formatter (stdout or --write)
legible repl                 # interactive REPL
```

---

## Architecture

```
legible-lang/
├── Cargo.toml
├── src/
│   ├── main.rs                 # CLI entry point (clap)
│   ├── lib.rs                  # Re-exports all modules for testing
│   ├── lexer/
│   │   ├── mod.rs
│   │   ├── token.rs            # Token enum + Span
│   │   └── scanner.rs          # Character-by-character tokenizer
│   ├── parser/
│   │   ├── mod.rs
│   │   ├── ast.rs              # AST node enums (arena-indexed)
│   │   ├── arena.rs            # Typed arena for AST allocation
│   │   └── parser.rs           # Recursive descent parser
│   ├── analyzer/
│   │   ├── mod.rs
│   │   ├── typechecker.rs      # Type checking pass
│   │   ├── intent.rs           # Intent-vs-code verification
│   │   └── contracts.rs        # Pre/post condition instrumentation
│   ├── interpreter/
│   │   ├── mod.rs
│   │   ├── evaluator.rs        # Tree-walking evaluator
│   │   ├── environment.rs      # Scope chain with Rc<RefCell<>> interiors
│   │   ├── value.rs            # Runtime value enum
│   │   └── builtins.rs         # Standard library functions
│   ├── formatter/
│   │   ├── mod.rs
│   │   └── canonical.rs        # Canonical form formatter
│   └── errors/
│       ├── mod.rs
│       └── reporter.rs         # Structured JSON error output
├── tests/
│   ├── integration.rs          # End-to-end .lbl file tests
│   └── fixtures/
│       ├── valid/
│       │   ├── hello.lbl
│       │   ├── hello.expected
│       │   ├── fizzbuzz.lbl
│       │   ├── fizzbuzz.expected
│       │   ├── pipelines.lbl
│       │   ├── pipelines.expected
│       │   ├── contracts.lbl
│       │   ├── contracts.expected
│       │   └── ...
│       └── errors/
│           ├── type_mismatch.lbl
│           ├── type_mismatch.error.json
│           └── ...
└── benches/
    └── interpreter_bench.rs    # Criterion benchmarks
```

### Dependency Policy

Keep dependencies minimal. Approved crates:

| Crate        | Purpose                          |
|--------------|----------------------------------|
| `clap`       | CLI argument parsing (derive)    |
| `serde`      | Serialization (error output)     |
| `serde_json` | JSON error reporting             |
| `miette`     | Dev-mode human-readable errors   |
| `logos`      | Lexer generator (optional)       |
| `criterion`  | Benchmarking (dev-dependency)    |

Do **not** add runtime dependencies beyond these without justification. No async runtime. No allocator crates. Keep the binary lean.

---

## Cargo.toml

```toml
[package]
name = "legible-lang"
version = "0.1.0"
edition = "2021"
rust-version = "1.75"

[dependencies]
clap = { version = "4", features = ["derive"] }
serde = { version = "1", features = ["derive"] }
serde_json = "1"
miette = { version = "7", features = ["fancy"] }

[dev-dependencies]
criterion = { version = "0.5", features = ["html_reports"] }
pretty_assertions = "1"

[[bench]]
name = "interpreter_bench"
harness = false
```

---

## Core Data Structures

### Spans and Source Tracking

Every token and AST node carries a `Span`:

```rust
/// Byte offset range into the source string.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}
```

Store the original source as a `&str` or `Arc<str>` alongside the AST so error reporting can slice back into it.

### Token Enum

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct SpannedToken {
    pub token: Token,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    // Literals
    Integer(i64),
    Decimal(f64),
    Text(String),
    Boolean(bool),
    None,

    // Identifiers & keywords
    Identifier(String),
    Let,
    Mutable,
    Set,
    Function,
    Public,
    Return,
    If,
    Then,
    Else,
    End,
    Match,
    When,
    Otherwise,
    For,
    In,
    Do,
    While,
    Record,
    Union,
    Use,
    With,
    Intent,
    Requires,
    Ensures,
    And,
    Or,
    Not,
    Fn,

    // Types (keywords)
    IntegerType,
    DecimalType,
    TextType,
    BooleanType,
    NothingType,
    AListOf,       // "a list of" → single token
    AMappingFrom,  // "a mapping from" → single token
    To,            // used in "a mapping from K to V"
    AnOptional,    // "an optional" → single token

    // Operators
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,       // ++
    Pipe,           // |>
    Equals,         // ==
    NotEquals,      // !=
    Greater,
    Less,
    GreaterEqual,
    LessEqual,
    Assign,         // =
    Arrow,          // =>
    Question,       // ?
    Dot,
    Comma,
    Colon,

    // Delimiters
    LeftParen,
    RightParen,
    LeftBracket,
    RightBracket,
    LeftBrace,
    RightBrace,

    // Special
    Comment(String),
    Newline,
    Eof,
}
```

**Critical lexer detail**: multi-word type keywords (`a list of`, `a mapping from`, `an optional`) must be recognized as **single tokens** by lookahead. When the lexer sees `a`, peek ahead for `list of` or `mapping from`. When it sees `an`, peek ahead for `optional`. This is essential — the parser must not have to reassemble these from individual words.

### AST Nodes

Use an arena-allocated design. Each node is an enum variant, and child references are `NodeId` indices into the arena:

```rust
/// Index into the AST arena.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct NodeId(pub usize);

#[derive(Debug, Clone)]
pub struct AstNode {
    pub kind: NodeKind,
    pub span: Span,
}

#[derive(Debug, Clone)]
pub enum NodeKind {
    // Top-level
    Program { statements: Vec<NodeId> },
    UseDecl { module_name: String },
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
    ReturnExpr { value: Option<NodeId> },
    ExprStatement { expr: NodeId },

    // Expressions
    IntegerLit(i64),
    DecimalLit(f64),
    TextLit(String),
    InterpolatedText { parts: Vec<TextPart> },
    BooleanLit(bool),
    NoneLit,
    ListLit { elements: Vec<NodeId> },
    MappingLit { entries: Vec<(NodeId, NodeId)> },
    Identifier(String),
    FieldAccess { object: NodeId, field: String },
    FunctionCall { callee: NodeId, arguments: Vec<NodeId> },
    Lambda {
        params: Vec<Param>,
        return_type: LegibleType,
        body: NodeId,
    },
    Pipeline { left: NodeId, right: NodeId },
    BinaryOp { left: NodeId, op: BinaryOperator, right: NodeId },
    UnaryOp { op: UnaryOperator, operand: NodeId },
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
    OldExpr { inner: NodeId },
}
```

**Supporting types** (not AST nodes, just data):

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Param {
    pub name: String,
    pub param_type: LegibleType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Field {
    pub name: String,
    pub field_type: LegibleType,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Variant {
    pub name: String,
    pub fields: Vec<Field>,  // empty for unit variants
}

#[derive(Debug, Clone, PartialEq)]
pub struct MatchArm {
    pub pattern: Pattern,
    pub body: Vec<NodeId>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Pattern {
    Literal(NodeId),
    Variant { name: String, bindings: Vec<String> },
    Otherwise,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TextPart {
    Literal(String),
    Interpolation(NodeId),
}

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
    Generic(String),  // for builtins: T, U, K, V
}

#[derive(Debug, Clone, PartialEq)]
pub enum BinaryOperator {
    Add, Sub, Mul, Div, Mod,
    Concat,
    Eq, NotEq, Gt, Lt, GtEq, LtEq,
    And, Or,
}

#[derive(Debug, Clone, PartialEq)]
pub enum UnaryOperator {
    Negate, Not,
}
```

### Runtime Values

```rust
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

#[derive(Debug, Clone)]
pub enum Callable {
    UserDefined {
        name: String,
        params: Vec<Param>,
        return_type: LegibleType,
        intent: String,
        requires: Vec<NodeId>,
        ensures: Vec<NodeId>,
        body: Vec<NodeId>,
        closure_env: Env,
    },
    Lambda {
        params: Vec<Param>,
        return_type: LegibleType,
        body: NodeId,
        closure_env: Env,
    },
    Builtin {
        name: String,
        func: fn(&[Value]) -> Result<Value, LegibleError>,
    },
}
```

### Environment

Use `Rc<RefCell<...>>` for the scope chain:

```rust
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub type Env = Rc<RefCell<Environment>>;

#[derive(Debug, Clone)]
pub struct Environment {
    bindings: HashMap<String, (Value, bool)>,  // (value, is_mutable)
    parent: Option<Env>,
}

impl Environment {
    pub fn new() -> Env {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent: None,
        }))
    }

    pub fn with_parent(parent: &Env) -> Env {
        Rc::new(RefCell::new(Environment {
            bindings: HashMap::new(),
            parent: Some(Rc::clone(parent)),
        }))
    }

    pub fn define(&mut self, name: String, value: Value, mutable: bool) { ... }
    pub fn get(&self, name: &str) -> Option<(Value, bool)> { ... }  // walk chain
    pub fn set(&mut self, name: &str, value: Value) -> Result<(), LegibleError> { ... }
}
```

---

## Language Specification

### File Extension

`.lbl`

### Comments

```
-- This is a line comment
```

No block comments. One comment style only (canonical form principle).

### Primitive Types

Legible uses verbose, natural-language-style type names:

| Legible Type              | Rust Representation              |
|---------------------------|----------------------------------|
| `integer`                 | `i64`                            |
| `decimal`                 | `f64`                            |
| `text`                    | `String`                         |
| `boolean`                 | `bool`                           |
| `nothing`                 | Unit `()`                        |
| `a list of T`             | `Vec<Value>`                     |
| `a mapping from K to V`   | `Vec<(Value, Value)>`            |
| `an optional T`           | `Option<Value>` (Value::None)    |

### Variable Declaration

```
let name: text = "Alice"
let age: integer = 30
let scores: a list of integer = [90, 85, 77]
let lookup: a mapping from text to integer = {"alice": 1, "bob": 2}
let maybe_val: an optional integer = none
```

Variables are **immutable by default**. Use `mutable` for reassignable bindings:

```
mutable count: integer = 0
set count = count + 1
```

`set` is the **only** reassignment keyword. `=` at declaration is initialization, not assignment.

### Functions

```
function greet(name: text): text
  intent: produce a greeting string that includes the given name
  return "Hello, " ++ name
end
```

Rules:
- Every function **must** have an `intent:` line as its first body statement.
- Max **40 lines** per function body (enforced by the analyzer). If exceeded, emit error `E_FUNCTION_TOO_LONG`.
- Return type is always explicit.
- No overloading. No default parameters. One signature per function name.

### Contracts (requires / ensures)

```
function withdraw(account: Account, amount: decimal): Account
  intent: subtract amount from account balance safely
  requires: account.balance >= amount, amount > 0
  ensures: result.balance == account.balance - amount
  let new_balance: decimal = account.balance - amount
  return Account { balance: new_balance }
end
```

- `requires:` is checked **before** the function body executes. Violation → runtime error `E_CONTRACT_REQUIRES`.
- `ensures:` is checked **after** return. The keyword `result` refers to the return value. `old(expr)` captures the value of `expr` at function entry. Violation → runtime error `E_CONTRACT_ENSURES`.
- Contracts are **optional** but encouraged.

### Control Flow

Legible uses **flat, keyword-delimited** control flow. No curly braces. No parentheses around conditions.

**If/else:**

```
if age > 18 then
  "adult"
else if age > 12 then
  "teen"
else
  "child"
end
```

`if` is an **expression** — it returns the value of the taken branch.

**Match (pattern matching):**

```
match status
  when "active" then handle_active()
  when "inactive" then handle_inactive()
  otherwise then handle_unknown()
end
```

Match is exhaustive — `otherwise` is required unless all variants are covered (for tagged unions).

**Loops:**

```
for item in collection do
  process(item)
end
```

```
while condition do
  step()
end
```

There is **no** `break` or `continue`. Use `filter` / `take_while` pipelines instead.

### Pipelines

The primary composition mechanism. The pipe operator `|>` threads a value through a chain:

```
users
  |> filter(fn(u: User): boolean => u.age > 30)
  |> sort_by(fn(u: User): text => u.name)
  |> take(10)
  |> map(fn(u: User): text => u.name)
```

Lambda syntax: `fn(params): return_type => body`

Lambdas are **single-expression only**. For multi-step logic, extract a named function.

### Records (Structs)

```
record User
  name: text
  age: integer
  email: an optional text
end
```

Construct with: `User { name: "Alice", age: 30, email: none }`

Access with dot notation: `user.name`

Records are **immutable**. To "update", use spread-copy:

```
let updated: User = user with { age: 31 }
```

### Tagged Unions

```
union Shape
  Circle { radius: decimal }
  Rectangle { width: decimal, height: decimal }
  Point
end
```

Construct: `Shape.Circle { radius: 5.0 }` or `Shape.Point`

Pattern match to destructure:

```
match shape
  when Circle { radius } then 3.14159 * radius * radius
  when Rectangle { width, height } then width * height
  when Point then 0.0
end
```

### String Operations

- Concatenation: `++` operator (NOT `+`)
- Interpolation: `"Hello, {name}!"` (single curly braces)
- Multiline strings: triple quotes `""" ... """`

### Operators

| Category    | Operators                              |
|-------------|----------------------------------------|
| Arithmetic  | `+`, `-`, `*`, `/`, `%`               |
| Comparison  | `==`, `!=`, `>`, `<`, `>=`, `<=`      |
| Logical     | `and`, `or`, `not`                     |
| String      | `++` (concat)                          |
| Pipeline    | `\|>`                                  |
| Optional    | `?` (unwrap-or-propagate)              |

No bitwise operators. No ternary operator (use `if/then/else`).

### Modules

Each `.lbl` file is a module. The filename is the module name.

```
-- in math_utils.lbl
public function add(a: integer, b: integer): integer
  intent: return the sum of two integers
  return a + b
end
```

```
-- in main.lbl
use math_utils

let result: integer = math_utils.add(1, 2)
```

- `public` marks exports. Everything is private by default.
- `use` imports a module. Access via `module_name.symbol`.
- No wildcard imports. No renaming. One canonical way.

---

## Built-in Functions

Implement in `builtins.rs`. Each builtin is registered as a `Callable::Builtin` in the global environment before `main()` is invoked.

### Registration Pattern

```rust
fn register_builtins(env: &Env) {
    let builtins: Vec<(&str, fn(&[Value]) -> Result<Value, LegibleError>)> = vec![
        ("print", builtin_print),
        ("read_line", builtin_read_line),
        ("length", builtin_length),
        // ... etc
    ];
    for (name, func) in builtins {
        env.borrow_mut().define(
            name.to_string(),
            Value::Function(Callable::Builtin {
                name: name.to_string(),
                func,
            }),
            false,
        );
    }
}
```

### Full Builtin Catalog

**I/O:**
- `print(value: text): nothing` — print to stdout with newline
- `read_line(): text` — read a line from stdin

**List operations:**
- `length(list: a list of T): integer`
- `filter(list: a list of T, pred: fn(T): boolean): a list of T`
- `map(list: a list of T, transform: fn(T): U): a list of U`
- `reduce(list: a list of T, initial: U, combine: fn(U, T): U): U`
- `sort_by(list: a list of T, key: fn(T): U): a list of T`
- `take(list: a list of T, count: integer): a list of T`
- `drop(list: a list of T, count: integer): a list of T`
- `append(list: a list of T, item: T): a list of T`
- `concat(a: a list of T, b: a list of T): a list of T`
- `contains(list: a list of T, item: T): boolean`
- `find(list: a list of T, pred: fn(T): boolean): an optional T`
- `range(start: integer, end_exclusive: integer): a list of integer`

**Text operations:**
- `split(str: text, delimiter: text): a list of text`
- `join(parts: a list of text, separator: text): text`
- `trim(str: text): text`
- `uppercase(str: text): text`
- `lowercase(str: text): text`
- `starts_with(str: text, prefix: text): boolean`
- `ends_with(str: text, suffix: text): boolean`
- `text_length(str: text): integer`
- `to_text(value: T): text` — universal stringification

**Mapping operations:**
- `keys(map: a mapping from K to V): a list of K`
- `values(map: a mapping from K to V): a list of V`
- `has_key(map: a mapping from K to V, key: K): boolean`
- `get(map: a mapping from K to V, key: K): an optional V`
- `put(map: a mapping from K to V, key: K, value: V): a mapping from K to V`

**Optional operations:**
- `unwrap(opt: an optional T): T` — panics if `none`
- `unwrap_or(opt: an optional T, default: T): T`
- `is_some(opt: an optional T): boolean`
- `is_none(opt: an optional T): boolean`

**Math:**
- `abs(n: decimal): decimal`
- `max(a: decimal, b: decimal): decimal`
- `min(a: decimal, b: decimal): decimal`
- `floor(n: decimal): integer`
- `ceil(n: decimal): integer`
- `round(n: decimal): integer`

**Type conversion:**
- `to_integer(value: text): an optional integer`
- `to_decimal(value: text): an optional decimal`

---

## Error Reporting Format

All errors **must** be emitted as structured JSON to stderr, one object per error:

```json
{
  "code": "E_TYPE_MISMATCH",
  "severity": "error",
  "location": {
    "file": "main.lbl",
    "line": 12,
    "column": 5,
    "end_line": 12,
    "end_column": 22
  },
  "message": "Expected type 'integer' but got 'text'",
  "context": "let x: integer = \"hello\"",
  "suggestion": "Convert the text to an integer using to_integer(), or change the variable type to 'text'"
}
```

Every error **must** include a `suggestion` field. This is critical — it enables LLM agents to self-correct in one pass.

### Rust Implementation

```rust
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct LegibleError {
    pub code: ErrorCode,
    pub severity: Severity,
    pub location: SourceLocation,
    pub message: String,
    pub context: String,
    pub suggestion: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SourceLocation {
    pub file: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

#[derive(Debug, Clone, Serialize)]
pub enum ErrorCode {
    #[serde(rename = "E_SYNTAX")]
    Syntax,
    #[serde(rename = "E_UNEXPECTED_TOKEN")]
    UnexpectedToken,
    #[serde(rename = "E_TYPE_MISMATCH")]
    TypeMismatch,
    #[serde(rename = "E_UNDEFINED_VARIABLE")]
    UndefinedVariable,
    #[serde(rename = "E_UNDEFINED_FUNCTION")]
    UndefinedFunction,
    #[serde(rename = "E_IMMUTABLE_REASSIGN")]
    ImmutableReassign,
    #[serde(rename = "E_FUNCTION_TOO_LONG")]
    FunctionTooLong,
    #[serde(rename = "E_MISSING_INTENT")]
    MissingIntent,
    #[serde(rename = "E_MISSING_RETURN_TYPE")]
    MissingReturnType,
    #[serde(rename = "E_EXHAUSTIVENESS")]
    Exhaustiveness,
    #[serde(rename = "E_DUPLICATE_DEFINITION")]
    DuplicateDefinition,
    #[serde(rename = "E_IMPORT_NOT_FOUND")]
    ImportNotFound,
    #[serde(rename = "E_DIVISION_BY_ZERO")]
    DivisionByZero,
    #[serde(rename = "E_UNWRAP_NONE")]
    UnwrapNone,
    #[serde(rename = "E_CONTRACT_REQUIRES")]
    ContractRequires,
    #[serde(rename = "E_CONTRACT_ENSURES")]
    ContractEnsures,
    #[serde(rename = "E_INDEX_OUT_OF_BOUNDS")]
    IndexOutOfBounds,
    #[serde(rename = "E_INTENT_MISMATCH")]
    IntentMismatch,
}
```

Emit with: `eprintln!("{}", serde_json::to_string(&error).unwrap());`

When the `--human` flag is passed (or in REPL mode), also render the error with `miette` for color-coded, human-friendly output. The JSON output to stderr always happens regardless.

---

## Intent Verification

The `intent:` line is verified using **heuristic keyword matching** (not full NLP — keep it deterministic).

Strategy for `analyzer/intent.rs`:

1. **Tokenize** the intent string into lowercase keywords, stripping stop words (`a`, `the`, `an`, `is`, `to`, `of`, `that`, `for`, `and`, `or`, `it`, `in`, `by`, `with`, etc.).
2. **Extract** semantic signals from the function body by walking the AST:
   - Function/method names called (e.g., `filter`, `sort_by`, `map`)
   - Operators used (`>`, `<`, `+`, `++`, etc.)
   - Field names accessed
   - Literals and constant patterns
3. **Build a signal map** that associates intent keywords with AST patterns:
   - `"filter"` → expect `filter` call or conditional in body
   - `"sort"` → expect `sort_by` call
   - `"sum"` / `"total"` / `"add"` → expect `+` or `reduce`
   - `"greeting"` / `"hello"` → expect string construction
   - `"subtract"` / `"minus"` / `"remove"` → expect `-` operator
   - `"print"` / `"display"` / `"output"` → expect `print` call
   - `"return"` / `"produce"` / `"compute"` / `"calculate"` → generic, always matches
4. If **fewer than 30%** of non-generic intent keywords have a plausible match in the body, emit `E_INTENT_MISMATCH` as a **warning** (not error).

This is intentionally a soft check. The goal is to catch obviously wrong intents (copy-paste mistakes), not to be a proof system.

---

## Canonical Formatter

`legible fmt` rewrites any valid `.lbl` file into **the** canonical form. Rules:

1. **Indentation**: 2 spaces per level, no tabs. Ever.
2. **One blank line** between top-level declarations (functions, records, unions).
3. **No blank lines** inside function bodies.
4. **Pipeline chains**: each `|>` stage on its own line, indented 2 spaces from the source expression.
5. **Trailing newline** at end of file.
6. **Keyword casing**: all keywords lowercase.
7. **Spaces around operators**: `a + b`, not `a+b`.
8. **No trailing whitespace** on any line.
9. **Record/union fields**: one per line, indented 2 spaces.
10. **Function arguments**: on one line if total length ≤ 80 chars. Otherwise, one per line indented 2 spaces.

The formatter is **idempotent**: `fmt(fmt(code)) == fmt(code)`. Test this property explicitly.

---

## Implementation Order

Build the interpreter in this exact order. Each phase should be **fully tested** before moving on.

### Phase 1: Lexer + Token Types
- Define the `Token` enum and `Span` in `token.rs`.
- Implement `scanner.rs` as a single-pass character scanner with lookahead.
- Handle multi-word type tokens (`a list of`, `a mapping from`, `an optional`).
- Handle string interpolation by emitting structured token sequences.
- Handle triple-quoted strings, `--` comments, and all operators including `|>` and `++`.
- **Test**: tokenize every token type. Test edge cases: empty strings, nested interpolation, adjacent operators, multi-word type keywords.

### Phase 2: Parser + AST
- Define AST node types in `ast.rs`. Implement the arena in `arena.rs`.
- Implement a **recursive descent** parser in `parser.rs`.
- Implement **Pratt parsing** for expression precedence.
- Parse: let bindings, function declarations (with intent/requires/ensures), if/else, match, for, while, pipelines, records, unions, lambdas, modules (`use` / `public`).
- **Test**: parse representative programs, verify AST structure. Use `pretty_assertions` for snapshot testing AST output.

### Phase 3: Tree-Walking Evaluator
- Implement `environment.rs` with the `Rc<RefCell<>>` scope chain.
- Implement `value.rs` with `Display` trait for runtime value printing.
- Implement `evaluator.rs` — a `fn evaluate(node: NodeId, arena: &Arena, env: &Env) -> Result<Value, LegibleError>` function.
- Implement `builtins.rs` with all standard library functions.
- **Test**: evaluate arithmetic, let bindings, function calls, closures, if/else, match, loops, pipelines, records, record update, union construction, pattern matching. Run fixture files and compare stdout to `.expected` files.

### Phase 4: Type Checker
- Implement `typechecker.rs` as a separate AST pass before evaluation.
- Walk the AST, infer and check types. Maintain a type environment parallel to runtime env.
- Handle generic builtins via structural unification (e.g., `filter` on `a list of User` infers `T = User`).
- **Test**: type errors are caught (E_TYPE_MISMATCH, E_UNDEFINED_VARIABLE, etc.), correct programs pass.

### Phase 5: Contracts + Intent
- Implement `contracts.rs` — before a function call, evaluate `requires` expressions in the caller's env. After return, evaluate `ensures` expressions with `result` and `old()` bindings.
- Implement `intent.rs` — heuristic keyword matching as described above.
- **Test**: contract violations produce correct errors, intent mismatches produce warnings.

### Phase 6: Error Reporter + CLI + Formatter
- Wire up `reporter.rs` to emit structured JSON to stderr.
- Implement `main.rs` with `clap` derive macros for `run`, `check`, `fmt`, `repl` subcommands.
- Implement the canonical formatter in `canonical.rs`.
- **Test**: end-to-end CLI usage. Formatter idempotency. REPL basic interaction.

---

## Testing Strategy

Use **`cargo test`** with the built-in test framework for unit and integration tests. Use `criterion` for benchmarks.

```bash
cargo test                      # all tests
cargo test -- --nocapture       # show println output
cargo test lexer                # filter by module
cargo bench                     # criterion benchmarks
```

### Unit Tests

Place in the same file as the implementation using `#[cfg(test)] mod tests { ... }`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokenize_simple_let() {
        let tokens = scan("let x: integer = 42");
        assert_eq!(tokens[0].token, Token::Let);
        assert_eq!(tokens[1].token, Token::Identifier("x".into()));
        // ...
    }
}
```

### Integration Tests

In `tests/integration.rs`, run `.lbl` fixture files through the full pipeline:

```rust
use std::fs;

fn run_fixture(name: &str) {
    let source = fs::read_to_string(
        format!("tests/fixtures/valid/{name}.lbl")
    ).unwrap();
    let expected = fs::read_to_string(
        format!("tests/fixtures/valid/{name}.expected")
    ).unwrap();
    let output = legible_lang::run_source(&source).unwrap();
    assert_eq!(output.trim(), expected.trim());
}

#[test] fn test_hello() { run_fixture("hello"); }
#[test] fn test_fizzbuzz() { run_fixture("fizzbuzz"); }
#[test] fn test_pipelines() { run_fixture("pipelines"); }
#[test] fn test_contracts() { run_fixture("contracts"); }
```

For error fixtures, assert that the emitted JSON matches the `.error.json` file.

### Property Tests (Optional but Encouraged)

- Formatter idempotency: `assert_eq!(fmt(source), fmt(fmt(source)))`
- Parse-then-format round-trip: `assert_eq!(fmt(source), fmt(parse_then_print(source)))`

---

## Example Programs

### Hello World — `tests/fixtures/valid/hello.lbl`

```
function main(): nothing
  intent: print a greeting to the console
  print("Hello, Legible!")
end
```

Expected output: `Hello, Legible!`

### FizzBuzz — `tests/fixtures/valid/fizzbuzz.lbl`

```
function fizzbuzz(n: integer): text
  intent: return fizz buzz or the number as text
  if n % 15 == 0 then
    "FizzBuzz"
  else if n % 3 == 0 then
    "Fizz"
  else if n % 5 == 0 then
    "Buzz"
  else
    to_text(n)
  end
end

function main(): nothing
  intent: print fizzbuzz for numbers 1 through 20
  for i in range(1, 21) do
    print(fizzbuzz(i))
  end
end
```

### Pipelines — `tests/fixtures/valid/pipelines.lbl`

```
record Person
  name: text
  age: integer
end

function get_senior_names(people: a list of Person): a list of text
  intent: filter people older than 65 and return their names sorted
  people
    |> filter(fn(p: Person): boolean => p.age > 65)
    |> sort_by(fn(p: Person): text => p.name)
    |> map(fn(p: Person): text => p.name)
end

function main(): nothing
  intent: demonstrate pipeline processing on a list of people
  let people: a list of Person = [
    Person { name: "Alice", age: 70 },
    Person { name: "Bob", age: 45 },
    Person { name: "Carol", age: 68 }
  ]
  let names: a list of text = get_senior_names(people)
  for name in names do
    print(name)
  end
end
```

Expected output:
```
Alice
Carol
```

### Contracts — `tests/fixtures/valid/contracts.lbl`

```
record Account
  owner: text
  balance: decimal
end

function deposit(account: Account, amount: decimal): Account
  intent: add amount to account balance
  requires: amount > 0.0
  ensures: result.balance == account.balance + amount
  account with { balance: account.balance + amount }
end

function withdraw(account: Account, amount: decimal): Account
  intent: subtract amount from account balance safely
  requires: amount > 0.0, account.balance >= amount
  ensures: result.balance == account.balance - amount
  account with { balance: account.balance - amount }
end

function main(): nothing
  intent: demonstrate deposit and withdrawal with contracts
  let acc: Account = Account { owner: "Alice", balance: 100.0 }
  let acc2: Account = deposit(acc, 50.0)
  print("After deposit: " ++ to_text(acc2.balance))
  let acc3: Account = withdraw(acc2, 30.0)
  print("After withdraw: " ++ to_text(acc3.balance))
end
```

Expected output:
```
After deposit: 150
After withdraw: 120
```

---

## Rust Coding Conventions

- **No `unwrap()` in production code.** Use `?` propagation or explicit error handling. `unwrap()` is allowed only in tests and builtins where the type system guarantees safety.
- **No `unsafe`.** There is no reason to need it in a tree-walking interpreter.
- **Use `#[must_use]`** on functions that return `Result` or values that should not be silently discarded.
- **Derive liberally**: `Debug`, `Clone`, `PartialEq`, `Serialize` on all public types.
- **Document every public function and type** with `///` doc comments.
- **No abbreviations** in identifiers. `token_index` not `tok_idx`. `current_character` not `cc`.
- **Error types**: all fallible operations return `Result<T, LegibleError>`. Do not use `Box<dyn Error>`.
- **No `println!` in library code.** All output goes through the evaluator's I/O abstraction (a `Write` trait object) so tests can capture output.
- **Clippy clean**: run `cargo clippy -- -W clippy::pedantic` and fix all warnings.

### I/O Abstraction

The evaluator must accept a writer for output so tests can capture it:

```rust
pub fn evaluate_program(
    arena: &Arena,
    root: NodeId,
    env: &Env,
    output: &mut dyn std::io::Write,
) -> Result<Value, LegibleError> { ... }
```

In `main.rs`, pass `&mut std::io::stdout()`. In tests, pass `&mut Vec<u8>`.

---

## Key Design Decisions

1. **`main()` is the entry point.** When running a `.lbl` file, the interpreter looks for `function main(): nothing` and invokes it. If absent, emit `E_UNDEFINED_FUNCTION` with suggestion "Define a main() function as the program entry point."
2. **Everything is an expression** where possible. `if/else` returns a value. `match` returns a value. Blocks return their last expression.
3. **No null, only `none`.** The optional type is explicit. Assigning `none` to a non-optional type is `E_TYPE_MISMATCH`.
4. **All data structures are immutable.** Strings, lists, records — operations return new values.
5. **`range(start, end)` is a builtin** returning `a list of integer` from start (inclusive) to end (exclusive).
6. **The pipeline operator `|>` passes the left side as the first argument** to the right side. `x |> f(y)` desugars to `f(x, y)`.
7. **Semicolons do not exist.** Newlines are statement terminators. Continuation across lines is implicit inside `()`, `[]`, `{}`, and after `|>`.
8. **No implicit type coercion.** `1 + "2"` is `E_TYPE_MISMATCH`. Use `to_text()`, `to_integer()`.
9. **The `with` keyword** for record update creates a shallow copy with specified fields replaced.
10. **Arena allocation for the AST.** All AST nodes live in a `Vec<AstNode>` and are referenced by `NodeId`. This avoids recursive `Box` types and makes the AST trivially traversable.

---

## Benchmarking

After Phase 3 is complete, add Criterion benchmarks in `benches/interpreter_bench.rs`:

```rust
use criterion::{criterion_group, criterion_main, Criterion};

fn bench_fizzbuzz(c: &mut Criterion) {
    let source = include_str!("../tests/fixtures/valid/fizzbuzz.lbl");
    c.bench_function("fizzbuzz_1_to_1000", |b| {
        b.iter(|| legible_lang::run_source(source))
    });
}

criterion_group!(benches, bench_fizzbuzz);
criterion_main!(benches);
```

Track performance across phases to catch regressions.

---

## Final Checklist Before Marking a Phase Complete

- [ ] `cargo test` passes with zero failures
- [ ] `cargo clippy -- -W clippy::pedantic` has zero warnings
- [ ] No uses of `unwrap()` outside of tests
- [ ] No uses of `unsafe`
- [ ] All error paths produce a `LegibleError` with a `suggestion` field
- [ ] All public types and functions have `///` doc comments
- [ ] Fixture files added for new features
- [ ] `cargo bench` runs without regression (Phase 3+)
