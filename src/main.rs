/// Legible language interpreter CLI.
use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser)]
#[command(name = "legible", version, about = "The Legible language interpreter")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a Legible source file
    Run {
        /// Path to the .lbl file
        file: String,
    },
    /// Type-check and verify a Legible source file
    Check {
        /// Path to the .lbl file
        file: String,
    },
    /// Format a Legible source file
    Fmt {
        /// Path to the .lbl file
        file: String,
        /// Write formatted output back to the file
        #[arg(long)]
        write: bool,
    },
    /// Start an interactive REPL
    Repl,
}

fn main() {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { file } => cmd_run(&file),
        Commands::Check { file } => cmd_check(&file),
        Commands::Fmt { file, write } => cmd_fmt(&file, write),
        Commands::Repl => cmd_repl(),
    }
}

fn cmd_run(file: &str) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{file}': {e}");
            process::exit(1);
        }
    };
    let mut stdout = std::io::stdout();
    match legible_lang::run_source_streaming(&source, file, &mut stdout) {
        Ok(()) => {}
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    }
}

fn cmd_check(file: &str) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{file}': {e}");
            process::exit(1);
        }
    };

    let tokens = match legible_lang::lexer::scan(&source) {
        Ok(t) => t,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let mut parser = legible_lang::parser::Parser::new(tokens, file, &source);
    let root = match parser.parse_program() {
        Ok(r) => r,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let arena = &parser.arena;

    let type_errors = legible_lang::analyzer::typechecker::typecheck(arena, root);
    let contract_errors = legible_lang::analyzer::contracts::check_contracts(arena, root);
    let intent_warnings = legible_lang::analyzer::intent::verify_intents(arena, root);

    let mut has_errors = false;
    for err in type_errors.iter().chain(contract_errors.iter()) {
        err.emit_json();
        has_errors = true;
    }
    for warning in &intent_warnings {
        warning.emit_json();
    }

    if has_errors {
        process::exit(1);
    } else {
        eprintln!("No errors found.");
    }
}

fn cmd_fmt(file: &str, write_back: bool) {
    let source = match std::fs::read_to_string(file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("Error reading file '{file}': {e}");
            process::exit(1);
        }
    };

    let tokens = match legible_lang::lexer::scan(&source) {
        Ok(t) => t,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let mut parser = legible_lang::parser::Parser::new(tokens, file, &source);
    let root = match parser.parse_program() {
        Ok(r) => r,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let formatted = legible_lang::formatter::format_source(&parser.arena, root);

    if write_back {
        if let Err(e) = std::fs::write(file, &formatted) {
            eprintln!("Error writing file '{file}': {e}");
            process::exit(1);
        }
    } else {
        print!("{formatted}");
    }
}

fn cmd_repl() {
    use legible_lang::interpreter::builtins::register_builtins;
    use legible_lang::interpreter::crypto_builtins::register_crypto_builtins;
    use legible_lang::interpreter::db_builtins::register_db_builtins;
    use legible_lang::interpreter::http_builtins::register_http_builtins;
    use legible_lang::interpreter::http_client_builtins::register_http_client_builtins;
    use legible_lang::interpreter::io_builtins::register_io_builtins;
    use legible_lang::interpreter::json_builtins::register_json_builtins;
    use legible_lang::interpreter::process_builtins::register_process_builtins;
    use legible_lang::interpreter::sdl_builtins::register_sdl_builtins;
    use legible_lang::interpreter::environment::Environment;

    eprintln!("Legible REPL v0.1.0 — type expressions to evaluate, Ctrl+D to exit");
    let env = Environment::new();
    register_builtins(&env);
    register_crypto_builtins(&env);
    register_sdl_builtins(&env);
    register_http_builtins(&env);
    register_http_client_builtins(&env);
    register_json_builtins(&env);
    register_io_builtins(&env);
    register_db_builtins(&env);
    register_process_builtins(&env);

    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        eprint!("legible> ");
        line.clear();
        match stdin.read_line(&mut line) {
            Ok(0) => {
                eprintln!();
                break;
            }
            Ok(_) => {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }
                match legible_lang::run_source(trimmed) {
                    Ok(output) => {
                        if !output.is_empty() {
                            print!("{output}");
                        }
                    }
                    Err(e) => {
                        e.emit_json();
                        eprintln!("{e}");
                    }
                }
            }
            Err(e) => {
                eprintln!("Read error: {e}");
                break;
            }
        }
    }
}
