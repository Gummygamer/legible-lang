# Legible

A programming language designed to be optimal for LLMs to write, read, and reason about. Legible uses natural-language-style syntax, explicit intent annotations, and a pipeline-first composition model.

**Live site:** https://legible-lang-site.fly.dev

## Features

- **Natural-language types**: `a list of text`, `a mapping from text to integer`, `an optional User`
- **Intent annotations**: every function declares its purpose in plain English (`intent:` line)
- **Design-by-contract**: `requires:` and `ensures:` clauses with runtime verification
- **Pipeline operator**: `|>` threads values through transformation chains
- **Immutable by default**: records and data structures are immutable; use `with` for updates
- **Readable control flow**: keyword-delimited `if/then/else`, `match/when/otherwise`, `for/in/do`, `while/do`
- **Modules**: each `.lbl` file is a module; `public` marks exports
- **Built-in HTTP, SQLite, JSON, and SDL2** support via interpreter builtins

## Installation

```bash
git clone https://github.com/darabat/legible
cd legible
cargo build --release
# Binary at target/release/legible
```

## Quick Start

```bash
# Run a program
legible run hello.lbl

# Type-check without running
legible check hello.lbl

# Format source canonically
legible fmt hello.lbl --write

# Interactive REPL
legible repl
```

## Language Tour

### Hello World

```
function main(): nothing
  intent: print a greeting to the console
  print("Hello, Legible!")
end
```

### Variables and Types

```
let name: text = "Alice"
let age: integer = 30
let scores: a list of integer = [90, 85, 77]
let lookup: a mapping from text to integer = {"alice": 1, "bob": 2}
let maybe: an optional integer = none

-- mutable bindings use 'mutable' and 'set'
mutable count: integer = 0
set count = count + 1
```

### Functions with Intent and Contracts

```
function withdraw(balance: decimal, amount: decimal): decimal
  intent: subtract amount from balance, rejecting invalid amounts
  requires: amount > 0.0, balance >= amount
  ensures: result == balance - amount
  balance - amount
end
```

### Records

```
record User
  name: text
  age: integer
  email: an optional text
end

let alice: User = User { name: "Alice", age: 30, email: none }
let older: User = alice with { age: 31 }
```

### Pipelines

```
users
  |> filter(fn(u: User): boolean => u.age > 30)
  |> sort_by(fn(u: User): text => u.name)
  |> map(fn(u: User): text => u.name)
```

### Tagged Unions

```
union Shape
  Circle { radius: decimal }
  Rectangle { width: decimal, height: decimal }
  Point
end

match shape
  when Circle { radius } then 3.14159 * radius * radius
  when Rectangle { width, height } then width * height
  when Point then 0.0
end
```

### Modules

```
-- math_utils.lbl
public function add(a: integer, b: integer): integer
  intent: return the sum of two integers
  a + b
end
```

```
-- main.lbl
use math_utils
let result: integer = math_utils.add(1, 2)
```

## Built-in Functions

### Standard Library

| Category | Functions |
|----------|-----------|
| I/O | `print`, `read_line` |
| Lists | `length`, `filter`, `map`, `reduce`, `sort_by`, `take`, `drop`, `append`, `concat`, `contains`, `find`, `range` |
| Text | `split`, `join`, `trim`, `uppercase`, `lowercase`, `starts_with`, `ends_with`, `text_length`, `to_text`, `replace`, `substring`, `contains_text`, `index_of` |
| Mappings | `keys`, `values`, `has_key`, `get`, `put` |
| Optionals | `unwrap`, `unwrap_or`, `is_some`, `is_none` |
| Math | `abs`, `max`, `min`, `floor`, `ceil`, `round` |
| Conversion | `to_integer`, `to_decimal`, `to_text` |
| Utility | `current_time_ms`, `log` |

### Extension Builtins

| Module | Functions | Crate |
|--------|-----------|-------|
| HTTP | `http_start`, `http_next_request`, `http_respond`, `http_respond_with_headers`, `http_stop` | `tiny_http` |
| JSON | `json_parse`, `json_encode` | `serde_json` |
| File I/O | `read_file`, `write_file`, `file_exists` | std |
| SQLite | `db_open`, `db_close`, `db_exec`, `db_exec_params`, `db_query`, `db_query_params` | `rusqlite` |

## Error Messages

All errors are emitted as structured JSON to stderr:

```json
{
  "code": "E_TYPE_MISMATCH",
  "severity": "error",
  "location": { "file": "main.lbl", "line": 5, "column": 3 },
  "message": "Expected type 'integer' but got 'text'",
  "context": "let x: integer = \"hello\"",
  "suggestion": "Convert the text to an integer using to_integer(), or change the variable type to 'text'"
}
```

Every error includes a `suggestion` field to help LLM agents self-correct.

## Running Tests

```bash
cargo test          # all tests
cargo test -- --nocapture   # show output
cargo bench         # criterion benchmarks
```

## Project Structure

```
legible/
├── src/
│   ├── main.rs                 # CLI (clap)
│   ├── lib.rs
│   ├── lexer/                  # Tokenizer
│   ├── parser/                 # Recursive descent parser + AST arena
│   ├── analyzer/               # Type checker, contracts, intent verifier
│   ├── interpreter/            # Tree-walking evaluator + all builtins
│   ├── formatter/              # Canonical code formatter
│   └── errors/                 # Structured error types + JSON reporter
└── tests/
    ├── integration.rs
    └── fixtures/valid/         # .lbl programs + .expected output files
```

## License

MIT
