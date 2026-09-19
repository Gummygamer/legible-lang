fn run(expr: &str) -> String {
    legible_lang::run_source(&format!("print({expr})\n")).unwrap().trim().to_string()
}

#[test]
fn substring_ascii() {
    assert_eq!(run("substring(\"hello world\", 6, 5)"), "world");
    assert_eq!(run("substring(\"hello\", 0, 0)"), "");
    assert_eq!(run("substring(\"hello\", 3, 100)"), "lo");
    assert_eq!(run("substring(\"hello\", 5, 1)"), "");
    assert_eq!(run("substring(\"hello\", 50, 1)"), "");
}

#[test]
fn substring_is_indexed_by_character_not_byte() {
    assert_eq!(run("substring(\"héllo wörld\", 1, 1)"), "é");
    assert_eq!(run("substring(\"héllo wörld\", 7, 3)"), "örl");
    assert_eq!(run("substring(\"日本語テキスト\", 2, 3)"), "語テキ");
    assert_eq!(run("substring(\"日本語\", 1, 100)"), "本語");
    assert_eq!(run("substring(\"日本語\", 3, 1)"), "");
    assert_eq!(run("substring(\"aé\", 0, 1)"), "a");
}

#[test]
fn substring_negative_arguments_do_not_panic() {
    assert_eq!(run("substring(\"hello\", -1, 2)"), "");
    assert_eq!(run("substring(\"hello\", 1, -1)"), "ello");
}
