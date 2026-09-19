//! End-to-end checks for the function comprehension budget (`E_FUNCTION_TOO_LONG`).

use legible_lang::errors::ErrorCode;

/// Serialise an error code the way it appears in the JSON error output.
fn code_of(source: &str) -> Option<String> {
    legible_lang::run_source(source)
        .err()
        .map(|error| serde_json::to_string(&error.code).expect("error code should serialize"))
}

#[test]
fn a_small_function_is_accepted() {
    let source = "\
function main(): nothing
  intent: print a greeting to the console
  print(\"hello\")
end
";
    assert!(legible_lang::run_source(source).is_ok());
}

/// The old 40-line limit could be dodged by packing work into one enormous
/// expression. Volume is measured on operators and operands, so it cannot.
#[test]
fn a_single_very_long_line_is_still_over_budget() {
    let literals: Vec<String> = (0..160).map(|index| format!("\"part{index}\"")).collect();
    let source = format!(
        "function main(): nothing\n  intent: print a long message\n  print({})\nend\n",
        literals.join(" ++ ")
    );
    assert_eq!(source.lines().count(), 4, "the whole body sits on one line");
    assert_eq!(code_of(&source).as_deref(), Some("\"E_FUNCTION_TOO_LONG\""));
}

#[test]
fn too_many_parameters_are_over_budget() {
    let parameters: Vec<String> = (0..10).map(|index| format!("p{index}: integer")).collect();
    let sum = (0..10)
        .map(|index| format!("p{index}"))
        .collect::<Vec<_>>()
        .join(" + ");
    let source = format!(
        "function total({}): integer\n  intent: add every parameter together\n  return {}\nend\n\nfunction main(): nothing\n  intent: print a value\n  print(to_text(total(1, 2, 3, 4, 5, 6, 7, 8, 9, 10)))\nend\n",
        parameters.join(", "),
        sum
    );
    assert_eq!(code_of(&source).as_deref(), Some("\"E_FUNCTION_TOO_LONG\""));
}

#[test]
fn the_error_names_the_dimension_and_offers_a_fix() {
    let literals: Vec<String> = (0..160).map(|index| format!("\"part{index}\"")).collect();
    let source = format!(
        "function main(): nothing\n  intent: print a long message\n  print({})\nend\n",
        literals.join(" ++ ")
    );
    let error = legible_lang::run_source(&source).expect_err("should exceed the budget");
    assert!(matches!(error.code, ErrorCode::FunctionTooLong));
    assert!(
        error.message.contains("Halstead volume"),
        "{}",
        error.message
    );
    assert!(
        error.suggestion.contains("not lines"),
        "{}",
        error.suggestion
    );
}
