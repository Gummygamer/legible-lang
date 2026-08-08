use std::fs;

fn run_fixture(name: &str) {
    let source =
        fs::read_to_string(format!("tests/fixtures/valid/{name}.lbl")).unwrap();
    let expected =
        fs::read_to_string(format!("tests/fixtures/valid/{name}.expected")).unwrap();
    let output = legible_lang::run_source(&source).unwrap();
    assert_eq!(output.trim(), expected.trim(), "Fixture {name} mismatch");
}

fn check_fixture_fails(name: &str) {
    let source =
        fs::read_to_string(format!("tests/fixtures/errors/{name}.lbl")).unwrap();
    assert!(
        legible_lang::run_source(&source).is_err(),
        "Fixture {name} expected to fail but succeeded"
    );
}

// --- Core language ---

#[test]
fn test_hello() {
    run_fixture("hello");
}

#[test]
fn test_fizzbuzz() {
    run_fixture("fizzbuzz");
}

#[test]
fn test_pipelines() {
    run_fixture("pipelines");
}

#[test]
fn test_contracts() {
    run_fixture("contracts");
}

// --- Data structure operations ---

#[test]
fn test_mappings() {
    run_fixture("mappings");
}

#[test]
fn test_optionals() {
    run_fixture("optionals");
}

#[test]
fn test_records() {
    run_fixture("records");
}

// --- Text operations ---

#[test]
fn test_text_ops() {
    run_fixture("text_ops");
}

// --- Formatter idempotency ---

#[test]
fn test_formatter_idempotency() {
    let fixture_names = ["hello", "fizzbuzz", "pipelines", "contracts", "records"];
    for name in fixture_names {
        let source = fs::read_to_string(format!("tests/fixtures/valid/{name}.lbl")).unwrap();
        let tokens = legible_lang::lexer::scan(&source).unwrap();
        let mut parser = legible_lang::parser::Parser::new(tokens, name, &source);
        let root = parser.parse_program().unwrap();
        let formatted_once = legible_lang::formatter::format_source(&parser.arena, root);

        let tokens2 = legible_lang::lexer::scan(&formatted_once).unwrap();
        let mut parser2 = legible_lang::parser::Parser::new(tokens2, name, &formatted_once);
        let root2 = parser2.parse_program().unwrap();
        let formatted_twice = legible_lang::formatter::format_source(&parser2.arena, root2);

        assert_eq!(
            formatted_once, formatted_twice,
            "Formatter not idempotent for fixture {name}"
        );
    }
}

// --- Command-line script arguments ---

#[test]
fn test_run_forwards_script_arguments() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_legible"))
        .args([
            "run",
            "tests/fixtures/valid/script_args.lbl",
            "alpha",
            "beta",
        ])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("count=3"));
    assert!(stdout.contains("arg=alpha"));
    assert!(stdout.contains("arg=beta"));
}

#[test]
fn test_run_without_script_arguments_includes_script_path() {
    let output = std::process::Command::new(env!("CARGO_BIN_EXE_legible"))
        .args(["run", "tests/fixtures/valid/script_args.lbl"])
        .output()
        .unwrap();

    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.contains("count=1"));
}
