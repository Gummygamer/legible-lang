/// Clarity language interpreter CLI.
use clap::{Parser, Subcommand};
use std::process;

#[derive(Parser)]
#[command(name = "clarity", version, about = "The Clarity language interpreter")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a Clarity source file
    Run {
        /// Path to the .cl file
        file: String,
    },
    /// Type-check and verify a Clarity source file
    Check {
        /// Path to the .cl file
        file: String,
    },
    /// Format a Clarity source file
    Fmt {
        /// Path to the .cl file
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
    match clarity_lang::run_source_with_filename(&source, file) {
        Ok(output) => print!("{output}"),
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

    let tokens = match clarity_lang::lexer::scan(&source) {
        Ok(t) => t,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let mut parser = clarity_lang::parser::Parser::new(tokens, file, &source);
    let root = match parser.parse_program() {
        Ok(r) => r,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let arena = &parser.arena;

    let type_errors = clarity_lang::analyzer::typechecker::typecheck(arena, root);
    let contract_errors = clarity_lang::analyzer::contracts::check_contracts(arena, root);
    let intent_warnings = clarity_lang::analyzer::intent::verify_intents(arena, root);

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

    let tokens = match clarity_lang::lexer::scan(&source) {
        Ok(t) => t,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let mut parser = clarity_lang::parser::Parser::new(tokens, file, &source);
    let root = match parser.parse_program() {
        Ok(r) => r,
        Err(e) => {
            e.emit_json();
            eprintln!("{e}");
            process::exit(1);
        }
    };

    let formatted = clarity_lang::formatter::format_source(&parser.arena, root);

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
    use clarity_lang::interpreter::builtins::register_builtins;
    use clarity_lang::interpreter::environment::Environment;

    eprintln!("Clarity REPL v0.1.0 — type expressions to evaluate, Ctrl+D to exit");
    let env = Environment::new();
    register_builtins(&env);

    let stdin = std::io::stdin();
    let mut line = String::new();
    loop {
        eprint!("clarity> ");
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
                match clarity_lang::run_source(trimmed) {
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
