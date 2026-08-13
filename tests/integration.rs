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

// --- Opaque byte buffers and bitwise integers ---

#[test]
fn test_bytes_and_bits() {
    run_fixture("bytes_and_bits");
}

#[test]
fn test_disasm_and_bytes() {
    run_fixture("disasm_and_bytes");
}

#[test]
fn test_disasm_operand_detail() {
    run_fixture("disasm_operand_detail");
}

#[test]
fn test_list_ops() {
    run_fixture("list_ops");
}

#[test]
fn test_accumulate_in_place() {
    run_fixture("accumulate_in_place");
}

#[test]
fn test_immutable_self_append_fails() {
    check_fixture_fails("immutable_self_append");
}

#[test]
fn test_self_append_accumulator_type_change_fails() {
    check_fixture_fails("self_append_accumulator_type_change");
}

#[test]
fn test_read_file_bytes_round_trip() {
    let path = std::env::temp_dir().join(format!(
        "legible-bytes-test-{}-{}.bin",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos(),
    ));
    std::fs::write(&path, [b'D', b'C', b'B', b'A', 0, 0, 0, 0, b'D', b'C', b'B', b'A']).unwrap();
    let source = format!(
        "function main(): nothing\n  intent: read and scan a binary file\n  let buffer: integer = read_file_bytes(\"{}\")\n  print(to_text(bytes_length(buffer)))\n  print(to_text(bytes_read_u32_le(buffer, 0)))\n  print(to_text(bytes_scan_words(buffer, 0, 4, 4294967295, 1094861636)))\n  print(to_text(bytes_free(buffer)))\nend\n",
        path.to_string_lossy().replace('\\', "\\\\"),
    );

    let result = legible_lang::run_source(&source);
    std::fs::remove_file(&path).unwrap();
    assert_eq!(result.unwrap().trim(), "12\n1094861636\n[0, 8]\ntrue");
}

#[test]
fn test_http_start_https_missing_certificate_returns_error() {
    let source = "function main(): nothing\n  intent: test HTTPS startup errors\n  http_start_https(18443, \"/nonexistent/cert.pem\", \"/nonexistent/key.pem\")\nend\n\nmain()\n";
    assert!(legible_lang::run_source(source).is_err());
}

// --- Optional Frida bindings ---

#[test]
#[cfg(not(feature = "frida"))]
fn test_frida_builtin_is_registered_without_feature() {
    let source = "function main(): nothing\n  intent: call an optional Frida builtin\n  print(frida_version())\nend\n";
    let error = legible_lang::run_source(source).unwrap_err();
    assert!(error.message.contains("built without Frida support"));
    assert!(!error.message.contains("unknown function"));
}

#[test]
#[cfg(feature = "frida")]
fn test_frida_version_is_non_empty() {
    let output = legible_lang::run_source(
        "function main(): nothing\n  intent: print the Frida version\n  print(frida_version())\nend\n",
    )
    .unwrap();
    assert!(!output.trim().is_empty());
}

#[test]
#[cfg(feature = "frida")]
fn test_frida_lists_and_opens_local_device() {
    let output = legible_lang::run_source(
        "function main(): nothing\n  intent: list and open the local Frida device\n  print(frida_device_ids())\n  let device: integer = frida_open_device(\"local\")\n  print(frida_device_name(\"local\"))\nend\n",
    )
    .unwrap();
    assert!(output.contains("local"));
    assert!(output.lines().nth(1).is_some_and(|name| !name.is_empty()));
}

#[test]
#[cfg(feature = "frida")]
fn test_frida_enumerates_processes_and_returns_missing_pid() {
    let output = legible_lang::run_source(
        "function main(): nothing\n  intent: inspect local Frida processes\n  let device: integer = frida_open_device(\"local\")\n  print(frida_device_process_names(device))\n  print(to_text(frida_device_process_pid(device, \"legible-name-that-cannot-exist-4eab\")))\nend\n",
    )
    .unwrap();
    assert_ne!(output.lines().next().unwrap_or(""), "[]");
    assert_eq!(output.lines().nth(1), Some("-1"));
}

#[test]
#[cfg(feature = "frida")]
fn test_frida_script_messages_end_to_end() {
    let output = legible_lang::run_source(
        "function main(): nothing\n  intent: receive send and log messages from a Frida script\n  let device: integer = frida_open_device(\"local\")\n  let pid: integer = frida_spawn(device, \"/bin/cat\")\n  let session: integer = frida_attach(device, pid)\n  let script: integer = frida_create_script(session, \"send(\\\"hello from legible\\\"); console.log(\\\"logline\\\");\")\n  frida_load_script(script)\n  frida_resume(device, pid)\n  print(frida_wait_message(script, 5000))\n  print(frida_wait_message(script, 5000))\n  frida_unload_script(script)\n  frida_detach(session)\n  frida_kill(device, pid)\nend\n",
    )
    .unwrap();
    assert!(output.contains("\"type\":\"send\""));
    assert!(output.contains("hello from legible"));
    assert!(output.contains("\"type\":\"log\""));
    assert!(output.contains("logline"));
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
