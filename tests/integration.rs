use std::fs;

fn run_fixture(name: &str) {
    let source =
        fs::read_to_string(format!("tests/fixtures/valid/{name}.lbl")).unwrap();
    let expected =
        fs::read_to_string(format!("tests/fixtures/valid/{name}.expected")).unwrap();
    let output = legible_lang::run_source(&source).unwrap();
    assert_eq!(output.trim(), expected.trim(), "Fixture {name} mismatch");
}

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
