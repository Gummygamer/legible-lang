//! Type checker tests: accepted programs, rejected programs, and a guard that
//! keeps the builtin signature table in step with the registered builtins.
use std::collections::HashSet;

use legible_lang::analyzer::builtin_signatures::BUILTIN_SIGNATURES;
use legible_lang::analyzer::typechecker::typecheck;
use legible_lang::errors::{LegibleError, Severity};
use legible_lang::interpreter::environment::Environment;

fn diagnostics(source: &str) -> Vec<LegibleError> {
    let tokens = legible_lang::lexer::scan(source).unwrap();
    let mut parser = legible_lang::parser::Parser::new(tokens, "test.lbl", source);
    let root = parser.parse_program().unwrap();
    typecheck(&parser.arena, root, source, "test.lbl")
}

fn errors(source: &str) -> Vec<LegibleError> {
    diagnostics(source)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Error))
        .collect()
}

fn warnings(source: &str) -> Vec<LegibleError> {
    diagnostics(source)
        .into_iter()
        .filter(|d| matches!(d.severity, Severity::Warning))
        .collect()
}

/// Wrap statements in a `main` function so each test states only what matters.
fn in_main(body: &str) -> String {
    format!("function main(): nothing\n  intent: exercise the type checker\n{body}\nend\n")
}

fn code_of(error: &LegibleError) -> String {
    error.code.to_string()
}

fn assert_clean(source: &str) {
    let found = diagnostics(source);
    assert!(found.is_empty(), "expected no diagnostics, got: {found:#?}");
}

fn assert_single_error(source: &str, code: &str, message_part: &str) {
    let found = errors(source);
    assert_eq!(found.len(), 1, "expected exactly one error, got: {found:#?}");
    assert_eq!(code_of(&found[0]), code);
    assert!(
        found[0].message.contains(message_part),
        "message {:?} should contain {message_part:?}",
        found[0].message
    );
    assert!(!found[0].suggestion.is_empty(), "every error needs a suggestion");
}

// --- Accepted programs ---

#[test]
fn accepts_basic_program() {
    assert_clean(&in_main(
        "  let name: text = \"Alice\"\n  let age: integer = 30\n  print(\"Hello, \" ++ name ++ to_text(age))",
    ));
}

#[test]
fn infers_generics_through_pipelines() {
    assert_clean(
        "record Person\n  name: text\n  age: integer\nend\n\nfunction names(people: a list of Person): a list of text\n  intent: filter sort and map the people\n  people\n    |> filter(fn(p: Person): boolean => p.age > 65)\n    |> sort_by(fn(p: Person): text => p.name)\n    |> map(fn(p: Person): text => p.name)\nend\n",
    );
}

#[test]
fn accepts_optional_widening_and_none() {
    assert_clean(&in_main(
        "  let maybe: an optional integer = none\n  let filled: an optional integer = 5\n  print(to_text(unwrap_or(maybe, 0) + unwrap_or(filled, 0)))",
    ));
}

#[test]
fn accepts_json_style_mappings() {
    assert_clean(&in_main(
        "  let parsed: a mapping from text to text = json_parse(\"\\{\\}\")\n  let count: integer = length(unwrap_or(get(parsed, \"items\"), []))\n  let mixed: a mapping from text to text = {\"ok\": true, \"n\": 3, \"items\": [1, 2]}\n  print(json_encode(mixed) ++ to_text(count))",
    ));
}

#[test]
fn accepts_exhaustive_union_match() {
    assert_clean(
        "union Shape\n  Circle { radius: decimal }\n  Rectangle { width: decimal, height: decimal }\n  Point\nend\n\nfunction area(shape: Shape): decimal\n  intent: compute the area of the shape\n  match shape\n    when Circle { radius } then 3.14 * radius * radius\n    when Rectangle { width, height } then width * height\n    when Point then 0.0\n  end\nend\n",
    );
}

#[test]
fn accepts_statement_if_with_different_branch_types() {
    assert_clean(&in_main(
        "  let flag: boolean = true\n  if flag then\n    print(\"yes\")\n  else\n    length(\"no\")\n  end",
    ));
}

#[test]
fn short_circuit_guards_type_check() {
    assert_clean(&in_main(
        "  let items: a list of integer = []\n  if length(items) > 0 and unwrap(get(items, 0)) > 0 then\n    print(\"positive\")\n  end",
    ));
}

#[test]
fn accepts_mutation_of_mutable_and_top_level_bindings() {
    assert_clean(
        "mutable total: integer = 0\n\nfunction bump(): nothing\n  intent: add one to the total\n  set total = total + 1\nend\n",
    );
}

#[test]
fn all_valid_fixtures_have_no_type_errors() {
    for entry in std::fs::read_dir("tests/fixtures/valid").unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("lbl") {
            continue;
        }
        let source = std::fs::read_to_string(&path).unwrap();
        let tokens = legible_lang::lexer::scan(&source).unwrap();
        let mut parser = legible_lang::parser::Parser::new(tokens, path.to_str().unwrap(), &source);
        let root = parser.parse_program().unwrap();
        let found: Vec<_> = typecheck(&parser.arena, root, &source, path.to_str().unwrap())
            .into_iter()
            .filter(|d| matches!(d.severity, Severity::Error))
            .collect();
        assert!(found.is_empty(), "{} has type errors: {found:#?}", path.display());
    }
}

// --- Rejected programs ---

#[test]
fn rejects_declared_type_mismatch() {
    assert_single_error(
        &in_main("  let x: integer = \"hello\""),
        "E_TYPE_MISMATCH",
        "expected 'integer' but got 'text'",
    );
}

#[test]
fn rejects_none_for_a_non_optional() {
    assert_single_error(
        &in_main("  let x: integer = none"),
        "E_TYPE_MISMATCH",
        "expected 'integer' but got 'an optional",
    );
}

#[test]
fn rejects_integer_plus_text() {
    assert_single_error(
        &in_main("  let x: integer = 1 + \"2\""),
        "E_TYPE_MISMATCH",
        "Cannot apply '+'",
    );
}

#[test]
fn rejects_integer_for_decimal_slot() {
    assert_single_error(
        &in_main("  let x: decimal = 5"),
        "E_TYPE_MISMATCH",
        "expected 'decimal' but got 'integer'",
    );
}

#[test]
fn rejects_undefined_variable_with_suggestion() {
    let found = errors(&in_main("  let count: integer = 1\n  print(to_text(cuont))"));
    assert_eq!(found.len(), 1);
    assert_eq!(code_of(&found[0]), "E_UNDEFINED_VARIABLE");
    assert!(found[0].suggestion.contains("count"), "{:?}", found[0].suggestion);
}

#[test]
fn rejects_undefined_function() {
    assert_single_error(
        &in_main("  let x: text = uppercas(\"a\")"),
        "E_UNDEFINED_FUNCTION",
        "Undefined function 'uppercas'",
    );
}

#[test]
fn rejects_wrong_argument_count() {
    assert_single_error(
        &in_main("  let x: text = replace(\"a\", \"b\")"),
        "E_TYPE_MISMATCH",
        "'replace' takes 3 arguments but 2 were given",
    );
}

#[test]
fn rejects_wrong_argument_type() {
    assert_single_error(
        &in_main("  let x: integer = text_length(5)"),
        "E_TYPE_MISMATCH",
        "Argument 1 of text_length()",
    );
}

#[test]
fn rejects_reassigning_immutable_binding() {
    assert_single_error(
        &in_main("  let x: integer = 1\n  set x = 2"),
        "E_IMMUTABLE_REASSIGN",
        "'x'",
    );
}

#[test]
fn rejects_type_change_through_set() {
    assert_single_error(
        &in_main("  mutable x: integer = 1\n  set x = \"two\""),
        "E_TYPE_MISMATCH",
        "Value assigned to 'x'",
    );
}

#[test]
fn rejects_wrong_return_type() {
    assert_single_error(
        "function f(): integer\n  intent: return text by mistake\n  return \"no\"\nend\n",
        "E_TYPE_MISMATCH",
        "Returned value",
    );
}

#[test]
fn rejects_function_that_ends_without_a_value() {
    assert_single_error(
        "function f(): integer\n  intent: end with a statement\n  let x: integer = 1\nend\n",
        "E_TYPE_MISMATCH",
        "last statement produces no value",
    );
}

#[test]
fn rejects_duplicate_function() {
    let source = "function f(): nothing\n  intent: first\n  skip()\nend\n\nfunction f(): nothing\n  intent: second\n  skip()\nend\n";
    assert_single_error(source, "E_DUPLICATE_DEFINITION", "'f'");
}

#[test]
fn rejects_record_with_missing_or_unknown_fields() {
    let declaration = "record User\n  name: text\n  age: integer\nend\n\n";
    let missing = format!("{declaration}{}", in_main("  let u: User = User { name: \"a\" }"));
    assert_single_error(&missing, "E_TYPE_MISMATCH", "missing field: age");
    let unknown = format!(
        "{declaration}{}",
        in_main("  let u: User = User { name: \"a\", age: 1, email: \"x\" }")
    );
    assert_single_error(&unknown, "E_TYPE_MISMATCH", "no field 'email'");
}

#[test]
fn rejects_reading_a_missing_record_field() {
    let source = format!(
        "record User\n  name: text\nend\n\n{}",
        in_main("  let u: User = User { name: \"a\" }\n  print(u.nam)")
    );
    assert_single_error(&source, "E_UNDEFINED_VARIABLE", "no field 'nam'");
}

#[test]
fn rejects_non_exhaustive_union_match() {
    let source = "union Shape\n  Circle { radius: decimal }\n  Point\nend\n\nfunction area(shape: Shape): decimal\n  intent: compute the area\n  match shape\n    when Circle { radius } then radius\n  end\nend\n";
    assert_single_error(source, "E_EXHAUSTIVENESS", "Point");
}

#[test]
fn rejects_match_without_otherwise_on_open_types() {
    assert_single_error(
        &in_main("  let n: integer = 1\n  let s: text = match n\n    when 1 then \"one\"\n  end"),
        "E_EXHAUSTIVENESS",
        "no 'otherwise'",
    );
}

#[test]
fn rejects_lambda_on_the_right_of_a_pipeline() {
    assert_single_error(
        &in_main("  let x: integer = 1 |> fn(n: integer): integer => n + 1"),
        "E_TYPE_MISMATCH",
        "right side of '|>'",
    );
}

#[test]
fn rejects_predicate_of_the_wrong_element_type() {
    assert_single_error(
        &in_main("  let names: a list of text = [\"a\"]\n  let kept: a list of text = filter(names, fn(n: integer): boolean => n > 1)"),
        "E_TYPE_MISMATCH",
        "Argument 2 of filter()",
    );
}

#[test]
fn rejects_two_argument_sort_comparator() {
    assert_single_error(
        &in_main("  let xs: a list of integer = [2, 1]\n  let ys: a list of integer = sort_by(xs, fn(a: integer, b: integer): boolean => a < b)"),
        "E_TYPE_MISMATCH",
        "Argument 2 of sort_by()",
    );
}

#[test]
fn rejects_non_boolean_condition_and_loop_over_non_list() {
    assert_single_error(
        &in_main("  if 1 then\n    skip()\n  end"),
        "E_TYPE_MISMATCH",
        "'if' condition",
    );
    assert_single_error(
        &in_main("  for c in \"abc\" do\n    skip()\n  end"),
        "E_TYPE_MISMATCH",
        "needs a list",
    );
}

#[test]
fn rejects_branches_of_different_types_when_the_value_is_used() {
    assert_single_error(
        &in_main("  let flag: boolean = true\n  let x: text = if flag then \"a\" else 1 end"),
        "E_TYPE_MISMATCH",
        "produce different types",
    );
}

#[test]
fn rejects_unknown_type_name() {
    assert_single_error(&in_main("  let x: Missing = 1"), "E_TYPE_MISMATCH", "Unknown type 'Missing'");
}

#[test]
fn checks_contracts_against_the_result_type() {
    assert_single_error(
        "function f(): integer\n  intent: return one\n  ensures: result == \"one\"\n  return 1\nend\n",
        "E_TYPE_MISMATCH",
        "Cannot compare 'integer' with 'text'",
    );
}

// --- Warnings ---

#[test]
fn optional_used_as_a_plain_value_is_a_warning() {
    let source = in_main("  let xs: a list of integer = [1]\n  let first: integer = get(xs, 0)");
    assert!(errors(&source).is_empty());
    let found = warnings(&source);
    assert_eq!(found.len(), 1, "{found:#?}");
    assert!(found[0].message.contains("an optional integer"));
    assert!(found[0].suggestion.contains("unwrap"));
}

#[test]
fn number_conversion_is_only_optional_for_text() {
    assert_clean(&in_main(
        "  let n: integer = 3\n  let d: decimal = to_decimal(n) / 2.0\n  print(to_text(d))",
    ));
    assert_eq!(
        warnings(&in_main("  let n: integer = to_integer(\"3\") + 1\n  print(to_text(n))")).len(),
        1
    );
}

// --- Modules ---

#[test]
fn checks_calls_into_used_modules() {
    let directory = std::env::temp_dir().join(format!("legible_typecheck_{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();
    std::fs::write(
        directory.join("math_utils.lbl"),
        "public function add(a: integer, b: integer): integer\n  intent: return the sum of two integers\n  return a + b\nend\n",
    )
    .unwrap();
    let main_path = directory.join("main.lbl");
    let check = |body: &str| {
        let source = format!("use math_utils\n\n{}", in_main(body));
        let tokens = legible_lang::lexer::scan(&source).unwrap();
        let mut parser = legible_lang::parser::Parser::new(tokens, "main.lbl", &source);
        let root = parser.parse_program().unwrap();
        typecheck(&parser.arena, root, &source, main_path.to_str().unwrap())
    };
    assert!(check("  let r: integer = math_utils.add(1, 2)").is_empty());
    let wrong_type = check("  let r: text = math_utils.add(1, 2)");
    assert_eq!(wrong_type.len(), 1, "{wrong_type:#?}");
    let missing = check("  let r: integer = math_utils.subtract(1, 2)");
    assert!(missing.iter().any(|d| code_of(d) == "E_UNDEFINED_FUNCTION"), "{missing:#?}");
    let bad_import = format!("use not_a_module\n\n{}", in_main("  skip()"));
    let tokens = legible_lang::lexer::scan(&bad_import).unwrap();
    let mut parser = legible_lang::parser::Parser::new(tokens, "main.lbl", &bad_import);
    let root = parser.parse_program().unwrap();
    let import_errors = typecheck(&parser.arena, root, &bad_import, main_path.to_str().unwrap());
    assert!(import_errors.iter().any(|d| code_of(d) == "E_IMPORT_NOT_FOUND"));
    std::fs::remove_dir_all(&directory).unwrap();
}

// --- Builtin signature table ---

/// Every builtin the interpreter registers must have a signature, otherwise
/// the checker would report calls to it as undefined functions.
#[test]
fn every_registered_builtin_has_a_signature() {
    use legible_lang::interpreter::{
        builtins::register_builtins, bytes_builtins::register_bytes_builtins,
        crypto_builtins::register_crypto_builtins, db_builtins::register_db_builtins,
        disasm_builtins::register_disasm_builtins, frida_builtins::register_frida_builtins,
        http_builtins::register_http_builtins, http_client_builtins::register_http_client_builtins,
        io_builtins::register_io_builtins, json_builtins::register_json_builtins,
        list_builtins::register_list_builtins, process_builtins::register_process_builtins,
    };
    let env = Environment::new();
    register_builtins(&env);
    register_bytes_builtins(&env);
    register_disasm_builtins(&env);
    register_list_builtins(&env);
    register_crypto_builtins(&env);
    register_frida_builtins(&env);
    register_http_builtins(&env);
    register_http_client_builtins(&env);
    register_json_builtins(&env);
    register_io_builtins(&env);
    register_db_builtins(&env);
    register_process_builtins(&env);
    let known: HashSet<&str> = BUILTIN_SIGNATURES.iter().map(|(name, _)| *name).collect();
    let missing: Vec<String> = env
        .borrow()
        .binding_names()
        .into_iter()
        .filter(|name| !known.contains(name.as_str()))
        .collect();
    assert!(missing.is_empty(), "builtins without a type signature: {missing:?}");
}
