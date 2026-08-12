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

### Global install (recommended)

Requires [Rust](https://rustup.rs) (stable toolchain).

```bash
git clone https://github.com/darabat/legible
cd legible
cargo install --path .
```

This builds a release binary and places it in `~/.cargo/bin/legible`, which is on your `PATH` if you installed Rust via rustup. After that, `legible` works from any directory.

### Optional Frida support

Frida support is off by default, so ordinary builds do not download or link Frida. Build or install it explicitly when native instrumentation builtins are needed:

```bash
BINDGEN_EXTRA_CLANG_ARGS="-I/usr/lib/gcc/x86_64-linux-gnu/15/include" cargo install --path . --features frida
```

On this Linux development machine the `BINDGEN_EXTRA_CLANG_ARGS` setting is required because libclang's resource headers are not installed alongside libclang. It is machine-specific: adjust or omit it when the local Clang installation can already find standard headers.

**Update** after pulling new changes:

```bash
cargo install --path .
```

**Uninstall:**

```bash
cargo uninstall legible-lang
```

### Build only (no global install)

```bash
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
| HTTP | `http_start`, `http_start_https`, `http_next_request`, `http_respond`, `http_respond_with_headers`, `http_stop` | `tiny_http` |
| JSON | `json_parse`, `json_encode` | `serde_json` |
| File I/O | `read_file`, `write_file`, `file_exists` | std |
| SQLite | `db_open`, `db_close`, `db_exec`, `db_exec_params`, `db_query`, `db_query_params` | `rusqlite` |
| Frida (optional) | `frida_version`, `frida_device_ids`, `frida_device_name`, `frida_open_device`, `frida_usb_device`, `frida_device_process_names`, `frida_device_process_pid`, `frida_spawn`, `frida_resume`, `frida_kill`, `frida_attach`, `frida_detach`, `frida_create_script`, `frida_load_script`, `frida_unload_script`, `frida_next_message`, `frida_wait_message` | `frida-sys` |

### Frida Builtins (optional feature)

These functions require a binary built with `--features frida`. Handles are opaque integer values returned by the corresponding open, attach, or create operation.

| Function | Signature |
|---|---|
| Version | `frida_version(): text` |
| Devices | `frida_device_ids(): a list of text`; `frida_device_name(id: text): text`; `frida_open_device(id: text): integer`; `frida_usb_device(timeout_seconds: integer): integer` |
| Processes | `frida_device_process_names(device: integer): a list of text`; `frida_device_process_pid(device: integer, name: text): integer` |
| Process lifecycle | `frida_spawn(device: integer, program: text): integer`; `frida_resume(device: integer, pid: integer): nothing`; `frida_kill(device: integer, pid: integer): nothing` |
| Sessions | `frida_attach(device: integer, pid: integer): integer`; `frida_detach(session: integer): nothing` |
| Scripts | `frida_create_script(session: integer, source: text): integer`; `frida_load_script(script: integer): nothing`; `frida_unload_script(script: integer): nothing` |
| Messages | `frida_next_message(script: integer): text`; `frida_wait_message(script: integer, timeout_ms: integer): text` |

`frida_next_message` is non-blocking; `frida_wait_message` returns empty text on timeout. Script messages are raw Frida JSON text. A non-Frida binary still registers these names and reports how to rebuild when one is called.

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
