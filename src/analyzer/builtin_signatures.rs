//! Type signatures for the builtin functions, written in a compact notation.
//!
//! Every builtin that the interpreter registers has an entry here so the type
//! checker can report unknown function names and check argument types. A
//! signature looks like `(List<T>, Fn(T) -> Boolean) -> List<T>`:
//!
//! * `Integer`, `Decimal`, `Text`, `Boolean`, `Nothing` are the primitive types.
//! * `Any` is compatible with every type (used for values the runtime treats
//!   dynamically, such as the result of `json_parse`).
//! * `Number` accepts an integer or a decimal.
//! * `Sized` accepts anything `length` can measure: a list, a text or a mapping.
//! * A single capital letter (`T`, `U`, `K`, `V`) is a generic type variable.
//! * `List<X>`, `Mapping<K, V>`, `Optional<X>` and `Fn(A, B) -> R` build compound types.
//! * A trailing `...` after the parameters means any further arguments are accepted unchecked.
//! * Any other capitalised name (such as `Request`) is a named record type.

/// A parsed builtin parameter or result type.
#[derive(Debug, Clone, PartialEq)]
pub enum SignatureType {
    /// Compatible with every type.
    Any,
    /// Integer or decimal.
    Number,
    /// List, text or mapping.
    Sized,
    /// Primitive integer.
    Integer,
    /// Primitive decimal.
    Decimal,
    /// Primitive text.
    Text,
    /// Primitive boolean.
    Boolean,
    /// The unit type.
    Nothing,
    /// A generic type variable such as `T`.
    Generic(String),
    /// A list of the inner type.
    List(Box<SignatureType>),
    /// A mapping from the first type to the second.
    Mapping(Box<SignatureType>, Box<SignatureType>),
    /// An optional inner type.
    Optional(Box<SignatureType>),
    /// A function value.
    Function(Vec<SignatureType>, Box<SignatureType>),
    /// A named record type.
    Named(String),
}

/// A parsed builtin signature.
#[derive(Debug, Clone, PartialEq)]
pub struct BuiltinSignature {
    /// Declared parameter types.
    pub params: Vec<SignatureType>,
    /// Whether extra trailing arguments are accepted without checking.
    pub variadic: bool,
    /// Declared result type.
    pub result: SignatureType,
}

/// Every builtin name paired with its signature text.
///
/// `get` is listed with `Any` parameters because the checker special-cases it:
/// it accepts either a mapping and a key or a list and an index.
pub const BUILTIN_SIGNATURES: &[(&str, &str)] = &[
    // I/O
    ("print", "(Any) -> Nothing"),
    ("read_line", "() -> Text"),
    ("read_stdin", "(Integer) -> Text"),
    ("write_stdout", "(Text) -> Nothing"),
    // Lists
    ("length", "(Sized) -> Integer"),
    ("filter", "(List<T>, Fn(T) -> Boolean) -> List<T>"),
    ("map", "(List<T>, Fn(T) -> U) -> List<U>"),
    ("reduce", "(List<T>, U, Fn(U, T) -> U) -> U"),
    ("sort_by", "(List<T>, Fn(T) -> U) -> List<T>"),
    ("take", "(List<T>, Integer) -> List<T>"),
    ("drop", "(List<T>, Integer) -> List<T>"),
    ("append", "(List<T>, T) -> List<T>"),
    ("concat", "(List<T>, List<T>) -> List<T>"),
    ("contains", "(List<T>, T) -> Boolean"),
    ("find", "(List<T>, Fn(T) -> Boolean) -> Optional<T>"),
    ("range", "(Integer, Integer) -> List<Integer>"),
    ("sort_integers", "(List<Integer>) -> List<Integer>"),
    ("sort_unique_integers", "(List<Integer>) -> List<Integer>"),
    ("sort_text", "(List<Text>) -> List<Text>"),
    ("field_values", "(List<Any>, Text) -> List<Any>"),
    ("last_index_of", "(List<T>, T) -> Optional<Integer>"),
    // Text
    ("split", "(Text, Text) -> List<Text>"),
    ("join", "(List<Text>, Text) -> Text"),
    ("trim", "(Text) -> Text"),
    ("uppercase", "(Text) -> Text"),
    ("lowercase", "(Text) -> Text"),
    ("starts_with", "(Text, Text) -> Boolean"),
    ("ends_with", "(Text, Text) -> Boolean"),
    ("text_length", "(Text) -> Integer"),
    ("to_text", "(Any) -> Text"),
    ("replace", "(Text, Text, Text) -> Text"),
    ("substring", "(Text, Integer, Integer) -> Text"),
    ("contains_text", "(Text, Text) -> Boolean"),
    ("index_of", "(Text, Text) -> Optional<Integer>"),
    ("url_decode", "(Text) -> Text"),
    // Mappings
    ("keys", "(Mapping<K, V>) -> List<K>"),
    ("values", "(Mapping<K, V>) -> List<V>"),
    ("has_key", "(Mapping<K, V>, K) -> Boolean"),
    ("get", "(Any, Any) -> Any"),
    ("put", "(Mapping<K, V>, K, V) -> Mapping<K, V>"),
    // Optionals
    ("unwrap", "(Optional<T>) -> T"),
    ("unwrap_or", "(Optional<T>, T) -> T"),
    ("is_some", "(Optional<T>) -> Boolean"),
    ("is_none", "(Optional<T>) -> Boolean"),
    // Math
    ("abs", "(Number) -> Number"),
    ("max", "(Number, Number) -> Number"),
    ("min", "(Number, Number) -> Number"),
    ("floor", "(Number) -> Integer"),
    ("ceil", "(Number) -> Integer"),
    ("round", "(Number) -> Integer"),
    ("random_int", "(Integer) -> Integer"),
    ("random_decimal", "() -> Decimal"),
    ("random_hex", "(Integer) -> Text"),
    // Conversion
    // `to_integer` and `to_decimal` return the number itself (not an optional)
    // when given a number; the checker special-cases that.
    ("to_integer", "(Any) -> Optional<Integer>"),
    ("to_decimal", "(Any) -> Optional<Decimal>"),
    // Bits and bytes (buffers are integer handles)
    ("bit_and", "(Integer, Integer) -> Integer"),
    ("bit_or", "(Integer, Integer) -> Integer"),
    ("bit_xor", "(Integer, Integer) -> Integer"),
    ("bit_not", "(Integer) -> Integer"),
    ("shift_left", "(Integer, Integer) -> Integer"),
    ("shift_right", "(Integer, Integer) -> Integer"),
    ("shift_right_unsigned", "(Integer, Integer) -> Integer"),
    ("read_file_bytes", "(Text) -> Integer"),
    ("write_file_bytes", "(Text, Integer) -> Any"),
    ("bytes_new", "(Integer) -> Integer"),
    ("bytes_from_hex", "(Text) -> Integer"),
    ("bytes_from_text", "(Text) -> Integer"),
    ("bytes_concat", "(Integer, Integer) -> Integer"),
    ("bytes_length", "(Integer) -> Integer"),
    ("bytes_get", "(Integer, Integer) -> Integer"),
    ("bytes_slice", "(Integer, Integer, Integer) -> Integer"),
    ("bytes_inflate", "(Integer, Integer) -> Integer"),
    ("bytes_crc32", "(Integer, Integer, Integer) -> Integer"),
    ("bytes_to_text", "(Integer) -> Text"),
    ("bytes_read_u16_le", "(Integer, Integer) -> Integer"),
    ("bytes_read_u32_le", "(Integer, Integer) -> Integer"),
    ("bytes_set", "(Integer, Integer, Integer) -> Boolean"),
    ("bytes_fill", "(Integer, Integer, Integer, Integer) -> Boolean"),
    ("bytes_write_bytes", "(Integer, Integer, Integer) -> Any"),
    ("bytes_write_u32_le", "(Integer, Integer, Integer) -> Boolean"),
    ("bytes_index_of", "(Integer, Text, Integer) -> Integer"),
    ("bytes_scan_words", "(Integer, Integer, Integer, Integer, Integer) -> List<Integer>"),
    ("bytes_free", "(Integer) -> Boolean"),
    ("disasm_arm32", "(...) -> Any"),
    ("disasm_arm64", "(...) -> Any"),
    // Files and processes
    ("read_file", "(Text) -> Text"),
    ("write_file", "(Text, Text) -> Nothing"),
    ("file_exists", "(Text) -> Boolean"),
    ("is_dir", "(Text) -> Boolean"),
    ("list_dir", "(Text) -> List<Text>"),
    ("create_dir", "(Text) -> Nothing"),
    ("chdir", "(Text) -> Nothing"),
    ("get_cwd", "() -> Text"),
    ("path_join", "(Text, Text, ...) -> Text"),
    ("env_get", "(Text) -> Optional<Text>"),
    ("get_args", "() -> List<Text>"),
    ("exit_process", "(Integer) -> Nothing"),
    ("shell_exec", "(Text) -> Mapping<Text, Text>"),
    ("skip", "() -> Nothing"),
    ("current_time_ms", "() -> Integer"),
    ("log", "(Text, Text) -> Nothing"),
    // JSON
    ("json_parse", "(Text) -> Any"),
    ("json_encode", "(Any) -> Text"),
    ("json_valid", "(Text) -> Boolean"),
    // HTTP
    ("http_start", "(Integer) -> Nothing"),
    ("http_start_https", "(Integer, Text, Text) -> Nothing"),
    ("http_next_request", "() -> Request"),
    ("http_respond", "(Integer, Text) -> Nothing"),
    ("http_respond_with_headers", "(Integer, Mapping<Text, Text>, Text) -> Nothing"),
    ("http_respond_file", "(Integer, Text, Text) -> Nothing"),
    ("http_stop", "() -> Nothing"),
    ("http_client_get", "(Text, Mapping<Text, Text>) -> Mapping<Text, Text>"),
    ("http_client_post", "(Text, Mapping<Text, Text>, Text) -> Mapping<Text, Text>"),
    // Database
    ("db_open", "(Text) -> Nothing"),
    ("db_close", "() -> Nothing"),
    ("db_exec", "(Text) -> Nothing"),
    ("db_exec_params", "(Text, List<Text>) -> Nothing"),
    ("db_query", "(Text) -> List<Mapping<Text, Text>>"),
    ("db_query_params", "(Text, List<Text>) -> List<Mapping<Text, Text>>"),
    // Passwords
    ("password_hash", "(Text) -> Text"),
    ("password_verify", "(Text, Text) -> Boolean"),
    // Frida
    ("frida_version", "() -> Text"),
    ("frida_device_ids", "() -> List<Text>"),
    ("frida_device_name", "(Text) -> Text"),
    ("frida_open_device", "(Text) -> Integer"),
    ("frida_usb_device", "(Integer) -> Integer"),
    ("frida_device_process_names", "(Integer) -> List<Text>"),
    ("frida_device_process_pid", "(Integer, Text) -> Integer"),
    ("frida_spawn", "(Integer, Text) -> Integer"),
    ("frida_resume", "(Integer, Integer) -> Nothing"),
    ("frida_kill", "(Integer, Integer) -> Nothing"),
    ("frida_attach", "(Integer, Integer) -> Integer"),
    ("frida_detach", "(Integer) -> Nothing"),
    ("frida_create_script", "(Integer, Text) -> Integer"),
    ("frida_load_script", "(Integer) -> Nothing"),
    ("frida_unload_script", "(Integer) -> Nothing"),
    ("frida_next_message", "(Integer) -> Text"),
    ("frida_wait_message", "(Integer, Integer) -> Text"),
    // SDL (only registered when the interpreter is built with the `sdl` feature)
    ("sdl_init", "(...) -> Any"),
    ("sdl_quit", "(...) -> Any"),
    ("sdl_clear", "(...) -> Any"),
    ("sdl_present", "(...) -> Any"),
    ("sdl_fill_rect", "(...) -> Any"),
    ("sdl_draw_text", "(...) -> Any"),
    ("sdl_load_font", "(...) -> Any"),
    ("sdl_poll_events", "(...) -> Any"),
    ("sdl_is_key_pressed", "(...) -> Any"),
    ("sdl_get_ticks", "(...) -> Any"),
    ("sdl_delay", "(...) -> Any"),
    ("sdl_screenshot", "(...) -> Any"),
];

/// Look up and parse the signature of a builtin function.
#[must_use]
pub fn builtin_signature(name: &str) -> Option<BuiltinSignature> {
    BUILTIN_SIGNATURES
        .iter()
        .find(|(builtin_name, _)| *builtin_name == name)
        .and_then(|(_, text)| parse_signature(text))
}

/// Parse a signature written in the notation described in the module docs.
#[must_use]
pub fn parse_signature(text: &str) -> Option<BuiltinSignature> {
    let mut parser = SignatureParser {
        characters: text.chars().collect(),
        position: 0,
    };
    parser.signature()
}

struct SignatureParser {
    characters: Vec<char>,
    position: usize,
}

impl SignatureParser {
    fn skip_spaces(&mut self) {
        while self.characters.get(self.position) == Some(&' ') {
            self.position += 1;
        }
    }

    fn eat(&mut self, expected: &str) -> bool {
        self.skip_spaces();
        let expected_characters: Vec<char> = expected.chars().collect();
        let end = self.position + expected_characters.len();
        if self.characters.get(self.position..end) == Some(expected_characters.as_slice()) {
            self.position = end;
            true
        } else {
            false
        }
    }

    fn word(&mut self) -> String {
        self.skip_spaces();
        let mut word = String::new();
        while let Some(&character) = self.characters.get(self.position) {
            if character.is_alphanumeric() || character == '_' {
                word.push(character);
                self.position += 1;
            } else {
                break;
            }
        }
        word
    }

    fn signature(&mut self) -> Option<BuiltinSignature> {
        if !self.eat("(") {
            return None;
        }
        let mut params = Vec::new();
        let mut variadic = false;
        if !self.eat(")") {
            loop {
                if self.eat("...") {
                    variadic = true;
                } else {
                    params.push(self.parse_type()?);
                }
                if self.eat(")") {
                    break;
                }
                if !self.eat(",") {
                    return None;
                }
            }
        }
        if !self.eat("->") {
            return None;
        }
        let result = self.parse_type()?;
        Some(BuiltinSignature {
            params,
            variadic,
            result,
        })
    }

    fn parse_type(&mut self) -> Option<SignatureType> {
        let word = self.word();
        match word.as_str() {
            "Any" => Some(SignatureType::Any),
            "Number" => Some(SignatureType::Number),
            "Sized" => Some(SignatureType::Sized),
            "Integer" => Some(SignatureType::Integer),
            "Decimal" => Some(SignatureType::Decimal),
            "Text" => Some(SignatureType::Text),
            "Boolean" => Some(SignatureType::Boolean),
            "Nothing" => Some(SignatureType::Nothing),
            "List" => {
                let inner = self.angle_arguments(1)?;
                Some(SignatureType::List(Box::new(inner.into_iter().next()?)))
            }
            "Optional" => {
                let inner = self.angle_arguments(1)?;
                Some(SignatureType::Optional(Box::new(inner.into_iter().next()?)))
            }
            "Mapping" => {
                let mut inner = self.angle_arguments(2)?.into_iter();
                let key = inner.next()?;
                let value = inner.next()?;
                Some(SignatureType::Mapping(Box::new(key), Box::new(value)))
            }
            "Fn" => {
                if !self.eat("(") {
                    return None;
                }
                let mut params = Vec::new();
                if !self.eat(")") {
                    loop {
                        params.push(self.parse_type()?);
                        if self.eat(")") {
                            break;
                        }
                        if !self.eat(",") {
                            return None;
                        }
                    }
                }
                if !self.eat("->") {
                    return None;
                }
                let result = self.parse_type()?;
                Some(SignatureType::Function(params, Box::new(result)))
            }
            "" => None,
            other if other.len() == 1 && other.chars().all(|c| c.is_ascii_uppercase()) => {
                Some(SignatureType::Generic(other.to_string()))
            }
            other => Some(SignatureType::Named(other.to_string())),
        }
    }

    fn angle_arguments(&mut self, count: usize) -> Option<Vec<SignatureType>> {
        if !self.eat("<") {
            return None;
        }
        let mut arguments = Vec::new();
        for index in 0..count {
            if index > 0 && !self.eat(",") {
                return None;
            }
            arguments.push(self.parse_type()?);
        }
        if self.eat(">") {
            Some(arguments)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_signature_parses() {
        for (name, text) in BUILTIN_SIGNATURES {
            assert!(parse_signature(text).is_some(), "signature of {name} does not parse: {text}");
        }
    }

    #[test]
    fn signature_names_are_unique() {
        let mut names: Vec<&str> = BUILTIN_SIGNATURES.iter().map(|(name, _)| *name).collect();
        names.sort_unstable();
        let total = names.len();
        names.dedup();
        assert_eq!(total, names.len());
    }

    #[test]
    fn higher_order_signature_has_function_parameter() {
        let signature = builtin_signature("filter").expect("filter is a builtin");
        assert_eq!(signature.params.len(), 2);
        assert!(matches!(signature.params[1], SignatureType::Function(_, _)));
    }
}
