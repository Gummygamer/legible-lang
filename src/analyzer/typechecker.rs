/// Type checking pass for the Legible language.
///
/// Walks the AST before evaluation and reports type errors, undefined names,
/// wrong argument counts, reassignment of immutable bindings and
/// non-exhaustive matches. The checker is *gradual*: values the runtime treats
/// dynamically (such as the result of `json_parse`) have the type `any`, which
/// is compatible with everything, so only definite conflicts are reported.
///
/// Type rules worth knowing:
///
/// * There is no implicit coercion between `integer` and `decimal` in
///   declarations, arguments and returns. Arithmetic and comparison between
///   the two is accepted because the runtime defines it.
/// * A plain value fits an `an optional T` slot, but an optional does not fit a
///   plain slot. Using an optional where the plain value is needed is reported
///   as a warning, because it works at runtime whenever the value is present.
/// * `a mapping from text to text` is Legible's only JSON-object type, so its
///   values are dynamic (see `json_object_convention`).
/// * A mapping literal whose values have different types is typed as a
///   mapping to `any` (a JSON-style object) rather than reported.
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::analyzer::builtin_signatures::{builtin_signature, BuiltinSignature, SignatureType};
use crate::errors::reporter::location_from_span;
use crate::errors::{ErrorCode, LegibleError, Severity};
use crate::lexer::Span;
use crate::parser::arena::Arena;
use crate::parser::ast::{
    BinaryOperator, Field, LegibleType, NodeId, NodeKind, Param, Pattern, TextPart,
    UnaryOperator, Variant,
};

/// The checker's internal type representation.
#[derive(Debug, Clone, PartialEq)]
enum Type {
    /// Compatible with every type.
    Any,
    /// The type of an expression that never produces a value, such as `return`.
    Never,
    Integer,
    Decimal,
    Text,
    Boolean,
    Nothing,
    List(Box<Type>),
    Mapping(Box<Type>, Box<Type>),
    Optional(Box<Type>),
    Record(String),
    Union(String),
    Function(Vec<Type>, Box<Type>),
    /// An inference variable, resolved through the substitution table.
    Variable(usize),
    /// A module imported with `use`.
    Module(String),
    /// The bare name of a union, used to reach its variants (`Shape.Point`).
    UnionNamespace(String),
    /// Builtin parameter constraint: an integer or a decimal.
    Number,
    /// Builtin parameter constraint: a list, a text or a mapping.
    Sized,
}

/// A user-defined or module function signature, kept unconverted so that
/// declaration order never matters.
#[derive(Debug, Clone)]
struct FunctionSignature {
    params: Vec<LegibleType>,
    result: LegibleType,
}

/// A name bound in a lexical scope.
#[derive(Debug, Clone)]
struct Binding {
    ty: Type,
    mutable: bool,
}

/// Run the type checker on the given AST. Returns a list of errors and warnings.
///
/// `source` and `file` are used for locations, and `file` also locates the
/// modules named by `use` declarations so their public signatures are known.
#[must_use]
pub fn typecheck(arena: &Arena, root: NodeId, source: &str, file: &str) -> Vec<LegibleError> {
    let base_dir = Path::new(file)
        .parent()
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let mut checker = Checker::new(arena, source, file, base_dir);
    checker.check_program(root);
    checker.finish()
}

struct Checker<'a> {
    arena: &'a Arena,
    source: &'a str,
    file: &'a str,
    base_dir: PathBuf,
    diagnostics: Vec<LegibleError>,
    substitution: Vec<Option<Type>>,
    records: HashMap<String, Vec<Field>>,
    unions: HashMap<String, Vec<Variant>>,
    functions: HashMap<String, FunctionSignature>,
    modules: HashMap<String, HashMap<String, FunctionSignature>>,
    unavailable_modules: HashSet<String>,
    scopes: Vec<HashMap<String, Binding>>,
    return_types: Vec<Type>,
    current_span: Span,
}

impl<'a> Checker<'a> {
    fn new(arena: &'a Arena, source: &'a str, file: &'a str, base_dir: PathBuf) -> Self {
        let mut records = HashMap::new();
        records.insert(
            "Request".to_string(),
            vec![
                Field { name: "method".into(), field_type: LegibleType::Text },
                Field { name: "path".into(), field_type: LegibleType::Text },
                Field { name: "body".into(), field_type: LegibleType::Text },
                Field { name: "query".into(), field_type: LegibleType::Text },
                Field {
                    name: "headers".into(),
                    field_type: LegibleType::MappingFrom(
                        Box::new(LegibleType::Text),
                        Box::new(LegibleType::Text),
                    ),
                },
            ],
        );
        Self {
            arena,
            source,
            file,
            base_dir,
            diagnostics: Vec::new(),
            substitution: Vec::new(),
            records,
            unions: HashMap::new(),
            functions: HashMap::new(),
            modules: HashMap::new(),
            unavailable_modules: HashSet::new(),
            scopes: vec![HashMap::new()],
            return_types: Vec::new(),
            current_span: Span { start: 0, end: 0 },
        }
    }

    fn finish(mut self) -> Vec<LegibleError> {
        let mut seen = HashSet::new();
        self.diagnostics.retain(|diagnostic| {
            seen.insert((
                diagnostic.location.line,
                diagnostic.location.column,
                diagnostic.message.clone(),
            ))
        });
        self.diagnostics
    }

    // ─── Diagnostics ────────────────────────────────────────

    fn report(
        &mut self,
        code: ErrorCode,
        severity: Severity,
        span: Span,
        message: String,
        suggestion: String,
    ) {
        let line_start = self.source[..span.start.min(self.source.len())]
            .rfind('\n')
            .map_or(0, |index| index + 1);
        let line_end = self.source[line_start..]
            .find('\n')
            .map_or(self.source.len(), |index| line_start + index);
        let context: String = self.source[line_start..line_end].trim().chars().take(200).collect();
        self.diagnostics.push(LegibleError {
            code,
            severity,
            location: location_from_span(self.file, self.source, span.start, span.end),
            message,
            context,
            suggestion,
        });
    }

    fn error(&mut self, code: ErrorCode, span: Span, message: String, suggestion: String) {
        self.report(code, Severity::Error, span, message, suggestion);
    }

    fn describe(&self, ty: &Type) -> String {
        match self.resolve(ty) {
            Type::Any => "any".to_string(),
            Type::Never => "nothing".to_string(),
            Type::Integer => "integer".to_string(),
            Type::Decimal => "decimal".to_string(),
            Type::Text => "text".to_string(),
            Type::Boolean => "boolean".to_string(),
            Type::Nothing => "nothing".to_string(),
            Type::List(inner) => format!("a list of {}", self.describe(&inner)),
            Type::Mapping(key, value) => {
                format!("a mapping from {} to {}", self.describe(&key), self.describe(&value))
            }
            Type::Optional(inner) => format!("an optional {}", self.describe(&inner)),
            Type::Record(name) | Type::Union(name) => name,
            Type::Function(params, result) => {
                let params: Vec<String> = params.iter().map(|p| self.describe(p)).collect();
                format!("fn({}): {}", params.join(", "), self.describe(&result))
            }
            Type::Variable(_) => "unknown".to_string(),
            Type::Module(name) => format!("module {name}"),
            Type::UnionNamespace(name) => format!("union {name}"),
            Type::Number => "a number".to_string(),
            Type::Sized => "a list, text or mapping".to_string(),
        }
    }

    fn conversion_hint(&self, expected: &Type, actual: &Type) -> String {
        let expected = self.resolve(expected);
        let actual = self.resolve(actual);
        match (&expected, &actual) {
            (_, Type::Optional(_)) => "Get the value out of the optional with unwrap(), unwrap_or(default) or the ? operator, or declare the target as an optional".to_string(),
            (Type::Text, Type::Integer | Type::Decimal | Type::Boolean) => {
                "Convert the value to text with to_text()".to_string()
            }
            (Type::Integer, Type::Text) => {
                "Convert with to_integer(), which returns an optional integer; unwrap it with unwrap_or()".to_string()
            }
            (Type::Decimal, Type::Text) => {
                "Convert with to_decimal(), which returns an optional decimal; unwrap it with unwrap_or()".to_string()
            }
            (Type::Decimal, Type::Integer) => {
                "Write the number with a decimal point (for example 5.0); there is no implicit integer to decimal conversion".to_string()
            }
            (Type::Integer, Type::Decimal) => {
                "Use floor(), ceil() or round() to turn the decimal into an integer".to_string()
            }
            _ => format!(
                "Make the expression produce '{}', or change the declared type to '{}'",
                self.describe(&expected),
                self.describe(&actual)
            ),
        }
    }

    /// Report `actual` not fitting `expected`, unless the two are compatible.
    fn expect_type(&mut self, expected: &Type, actual: &Type, span: Span, what: &str) {
        if self.compatible(expected, actual) {
            return;
        }
        // An optional of known type is a value that may be none (a warning);
        // the bare `none` literal has no inner type and is always an error.
        let leaks_optional = matches!(self.resolve(actual), Type::Optional(inner) if !matches!(*inner, Type::Variable(_)))
            && !matches!(self.resolve(expected), Type::Optional(_));
        let severity = if leaks_optional { Severity::Warning } else { Severity::Error };
        let message = format!(
            "{what}: expected '{}' but got '{}'",
            self.describe(expected),
            self.describe(actual)
        );
        let suggestion = self.conversion_hint(expected, actual);
        self.report(ErrorCode::TypeMismatch, severity, span, message, suggestion);
    }

    /// Use an optional as its inner type, warning that it may be none.
    fn unwrap_for_use(&mut self, ty: Type, span: Span, use_description: &str) -> Type {
        if let Type::Optional(inner) = self.resolve(&ty) {
            if *inner == Type::Any {
                return Type::Any;
            }
            self.report(
                ErrorCode::TypeMismatch,
                Severity::Warning,
                span,
                format!(
                    "An optional value is used {use_description} without being unwrapped (type '{}')",
                    self.describe(&ty)
                ),
                "Unwrap it first with unwrap(), unwrap_or(default) or the ? operator".to_string(),
            );
            *inner
        } else {
            ty
        }
    }

    // ─── Inference variables ────────────────────────────────

    fn fresh(&mut self) -> Type {
        self.substitution.push(None);
        Type::Variable(self.substitution.len() - 1)
    }

    fn shallow(&self, ty: &Type) -> Type {
        let mut current = ty.clone();
        while let Type::Variable(index) = current {
            match &self.substitution[index] {
                Some(bound) => current = bound.clone(),
                None => break,
            }
        }
        current
    }

    fn resolve(&self, ty: &Type) -> Type {
        match self.shallow(ty) {
            Type::List(inner) => Type::List(Box::new(self.resolve(&inner))),
            Type::Mapping(key, value) => {
                Type::Mapping(Box::new(self.resolve(&key)), Box::new(self.resolve(&value)))
            }
            Type::Optional(inner) => Type::Optional(Box::new(self.resolve(&inner))),
            Type::Function(params, result) => Type::Function(
                params.iter().map(|p| self.resolve(p)).collect(),
                Box::new(self.resolve(&result)),
            ),
            other => other,
        }
    }

    /// Whether a value of type `actual` may be used where `expected` is required.
    fn compatible(&mut self, expected: &Type, actual: &Type) -> bool {
        let expected = self.shallow(expected);
        let actual = self.shallow(actual);
        match (&expected, &actual) {
            (Type::Variable(a), Type::Variable(b)) if a == b => true,
            (Type::Variable(index), other) | (other, Type::Variable(index)) => {
                self.substitution[*index] = Some(other.clone());
                true
            }
            (Type::Any | Type::Never, _) | (_, Type::Any | Type::Never) => true,
            (Type::Number, Type::Integer | Type::Decimal) => true,
            (Type::Sized, Type::List(_) | Type::Text | Type::Mapping(_, _)) => true,
            (_, Type::Optional(given)) if self.shallow(given) == Type::Any => true,
            (Type::Optional(wanted), Type::Optional(given)) => self.compatible(wanted, given),
            (Type::Optional(wanted), given) => self.compatible(wanted, given),
            (Type::List(wanted), Type::List(given)) => self.compatible(wanted, given),
            (Type::Mapping(wanted_key, wanted_value), Type::Mapping(given_key, given_value)) => {
                self.compatible(wanted_key, given_key) && self.compatible(wanted_value, given_value)
            }
            (Type::Function(wanted, wanted_result), Type::Function(given, given_result)) => {
                wanted.len() == given.len()
                    && wanted.iter().zip(given).all(|(w, g)| self.compatible(w, g))
                    && self.compatible(wanted_result, given_result)
            }
            (a, b) => a == b,
        }
    }

    // ─── Type conversion and declarations ───────────────────

    fn convert(&mut self, declared: &LegibleType) -> Type {
        match declared {
            LegibleType::Integer => Type::Integer,
            LegibleType::Decimal => Type::Decimal,
            LegibleType::Text => Type::Text,
            LegibleType::Boolean => Type::Boolean,
            LegibleType::Nothing => Type::Nothing,
            LegibleType::ListOf(inner) => Type::List(Box::new(self.convert(inner))),
            LegibleType::MappingFrom(key, value) => {
                let key = self.convert(key);
                let value = self.convert(value);
                json_object_convention(key, value)
            }
            LegibleType::Optional(inner) => Type::Optional(Box::new(self.convert(inner))),
            LegibleType::Function { params, return_type } => Type::Function(
                params.iter().map(|p| self.convert(p)).collect(),
                Box::new(self.convert(return_type)),
            ),
            LegibleType::Generic(_) => Type::Any,
            LegibleType::Named(name) => {
                if self.records.contains_key(name) {
                    Type::Record(name.clone())
                } else if self.unions.contains_key(name) {
                    Type::Union(name.clone())
                } else {
                    let span = self.current_span;
                    self.error(
                        ErrorCode::TypeMismatch,
                        span,
                        format!("Unknown type '{name}'"),
                        format!("Declare 'record {name}' or 'union {name}', or use one of the built-in types (integer, decimal, text, boolean, a list of T, a mapping from K to V, an optional T)"),
                    );
                    Type::Any
                }
            }
        }
    }

    fn convert_signature(
        &mut self,
        signature: &SignatureType,
        generics: &mut HashMap<String, Type>,
    ) -> Type {
        match signature {
            SignatureType::Any => Type::Any,
            SignatureType::Number => Type::Number,
            SignatureType::Sized => Type::Sized,
            SignatureType::Integer => Type::Integer,
            SignatureType::Decimal => Type::Decimal,
            SignatureType::Text => Type::Text,
            SignatureType::Boolean => Type::Boolean,
            SignatureType::Nothing => Type::Nothing,
            SignatureType::Generic(name) => {
                if let Some(existing) = generics.get(name) {
                    existing.clone()
                } else {
                    let variable = self.fresh();
                    generics.insert(name.clone(), variable.clone());
                    variable
                }
            }
            SignatureType::List(inner) => {
                Type::List(Box::new(self.convert_signature(inner, generics)))
            }
            SignatureType::Optional(inner) => {
                Type::Optional(Box::new(self.convert_signature(inner, generics)))
            }
            SignatureType::Mapping(key, value) => {
                let key = self.convert_signature(key, generics);
                let value = self.convert_signature(value, generics);
                json_object_convention(key, value)
            }
            SignatureType::Function(params, result) => Type::Function(
                params.iter().map(|p| self.convert_signature(p, generics)).collect(),
                Box::new(self.convert_signature(result, generics)),
            ),
            SignatureType::Named(name) => Type::Record(name.clone()),
        }
    }

    fn collect_declarations(&mut self, root: NodeId) {
        let arena = self.arena;
        let NodeKind::Program { statements } = &arena.get(root).kind else {
            return;
        };
        let mut seen: HashSet<String> = HashSet::new();
        for &statement in statements {
            let node = arena.get(statement);
            match &node.kind {
                NodeKind::FunctionDecl { name, params, return_type, .. } => {
                    if !seen.insert(format!("function {name}")) {
                        self.duplicate(name, node.span);
                    }
                    self.functions.insert(
                        name.clone(),
                        FunctionSignature {
                            params: params.iter().map(|p| p.param_type.clone()).collect(),
                            result: return_type.clone(),
                        },
                    );
                }
                NodeKind::RecordDecl { name, fields } => {
                    if !seen.insert(format!("type {name}")) {
                        self.duplicate(name, node.span);
                    }
                    self.records.insert(name.clone(), fields.clone());
                }
                NodeKind::UnionDecl { name, variants } => {
                    if !seen.insert(format!("type {name}")) {
                        self.duplicate(name, node.span);
                    }
                    self.unions.insert(name.clone(), variants.clone());
                }
                _ => {}
            }
        }
    }

    fn duplicate(&mut self, name: &str, span: Span) {
        self.error(
            ErrorCode::DuplicateDefinition,
            span,
            format!("'{name}' is defined more than once"),
            format!("Rename or remove one of the definitions of '{name}'"),
        );
    }

    /// Load the modules named by `use` declarations so their public function
    /// signatures and record and union types are known.
    fn load_modules(&mut self, root: NodeId) {
        let arena = self.arena;
        let NodeKind::Program { statements } = &arena.get(root).kind else {
            return;
        };
        for &statement in statements {
            let node = arena.get(statement);
            if let NodeKind::UseDecl { module_name } = &node.kind {
                let mut visited = HashSet::new();
                let base_dir = self.base_dir.clone();
                match self.load_module(module_name, &base_dir, &mut visited) {
                    Ok(functions) => {
                        self.modules.insert(module_name.clone(), functions);
                    }
                    Err(problem) => {
                        self.unavailable_modules.insert(module_name.clone());
                        self.error(
                            ErrorCode::ImportNotFound,
                            node.span,
                            format!("Cannot use module '{module_name}': {problem}"),
                            format!("Create '{module_name}.lbl' next to this file (or in a lib directory) and mark its exports 'public'"),
                        );
                    }
                }
            }
        }
    }

    fn load_module(
        &mut self,
        module_name: &str,
        base_dir: &Path,
        visited: &mut HashSet<String>,
    ) -> Result<HashMap<String, FunctionSignature>, String> {
        let path = crate::find_module_file(module_name, base_dir).map_err(|e| e.message)?;
        let module_source = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
        let tokens = crate::lexer::scan(&module_source).map_err(|e| e.message)?;
        let mut parser = crate::parser::Parser::new(
            tokens,
            path.to_str().unwrap_or(module_name),
            &module_source,
        );
        let root = parser.parse_program().map_err(|e| e.message)?;
        let module_arena = parser.arena;
        let NodeKind::Program { statements } = &module_arena.get(root).kind else {
            return Ok(HashMap::new());
        };
        let module_dir = path.parent().unwrap_or(base_dir).to_path_buf();
        let mut functions = HashMap::new();
        for &statement in statements {
            match &module_arena.get(statement).kind {
                NodeKind::FunctionDecl { name, params, return_type, is_public: true, .. } => {
                    functions.insert(
                        name.clone(),
                        FunctionSignature {
                            params: params.iter().map(|p| p.param_type.clone()).collect(),
                            result: return_type.clone(),
                        },
                    );
                }
                NodeKind::RecordDecl { name, fields } => {
                    self.records.entry(name.clone()).or_insert_with(|| fields.clone());
                }
                NodeKind::UnionDecl { name, variants } => {
                    self.unions.entry(name.clone()).or_insert_with(|| variants.clone());
                }
                NodeKind::UseDecl { module_name: dependency } if visited.insert(dependency.clone()) => {
                    // Only the dependency's types matter here; a broken
                    // dependency is reported when that module is checked.
                    let _ = self.load_module(dependency, &module_dir, visited);
                }
                _ => {}
            }
        }
        Ok(functions)
    }

    // ─── Scopes ─────────────────────────────────────────────

    fn define(&mut self, name: &str, ty: Type, mutable: bool) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), Binding { ty, mutable });
        }
    }

    fn lookup(&self, name: &str) -> Option<Binding> {
        self.scopes.iter().rev().find_map(|scope| scope.get(name).cloned())
    }

    fn known_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self
            .scopes
            .iter()
            .flat_map(|scope| scope.keys().cloned())
            .chain(self.functions.keys().cloned())
            .collect();
        names.extend(crate::analyzer::builtin_signatures::BUILTIN_SIGNATURES.iter().map(|(n, _)| (*n).to_string()));
        names
    }

    fn did_you_mean(&self, name: &str) -> String {
        let best = self
            .known_names()
            .into_iter()
            .map(|candidate| (edit_distance(name, &candidate), candidate))
            .filter(|(distance, candidate)| *distance <= 2.max(candidate.len() / 4) && *distance < name.len())
            .min_by_key(|(distance, _)| *distance);
        match best {
            Some((_, candidate)) => format!("Did you mean '{candidate}'?"),
            None => format!("Define '{name}' before using it, or check the spelling"),
        }
    }

    // ─── Program and functions ──────────────────────────────

    fn check_program(&mut self, root: NodeId) {
        let arena = self.arena;
        let NodeKind::Program { statements } = &arena.get(root).kind else {
            return;
        };
        self.load_modules(root);
        self.collect_declarations(root);
        self.check_type_declarations(statements);

        // Top-level bindings are visible to every function.
        for &statement in statements {
            if let NodeKind::LetBinding { name, declared_type, mutable, .. } = &arena.get(statement).kind {
                self.current_span = arena.get(statement).span;
                let ty = self.convert(declared_type);
                self.define(name, ty, *mutable);
            }
        }
        for &statement in statements {
            if matches!(arena.get(statement).kind, NodeKind::FunctionDecl { .. }) {
                self.check_function(statement);
            }
        }
        for &statement in statements {
            match arena.get(statement).kind {
                NodeKind::FunctionDecl { .. }
                | NodeKind::RecordDecl { .. }
                | NodeKind::UnionDecl { .. }
                | NodeKind::UseDecl { .. } => {}
                _ => {
                    self.check_statement(statement, false);
                }
            }
        }
    }

    fn check_type_declarations(&mut self, statements: &[NodeId]) {
        let arena = self.arena;
        for &statement in statements {
            let node = arena.get(statement);
            self.current_span = node.span;
            match &node.kind {
                NodeKind::RecordDecl { fields, .. } => {
                    for field in fields {
                        self.convert(&field.field_type);
                    }
                }
                NodeKind::UnionDecl { variants, .. } => {
                    for variant in variants {
                        for field in &variant.fields {
                            self.convert(&field.field_type);
                        }
                    }
                }
                _ => {}
            }
        }
    }

    fn check_function(&mut self, function_id: NodeId) {
        let arena = self.arena;
        let node = arena.get(function_id);
        let NodeKind::FunctionDecl { params, return_type, requires, ensures, body, .. } = &node.kind else {
            return;
        };
        self.current_span = node.span;
        let result = self.convert(return_type);
        self.scopes.push(HashMap::new());
        self.define_parameters(params);
        self.return_types.push(result.clone());

        for &requirement in requires {
            let ty = self.check_expression(requirement);
            self.expect_type(&Type::Boolean, &ty, arena.get(requirement).span, "'requires' condition");
        }
        self.scopes.push(HashMap::new());
        self.define("result", result.clone(), false);
        for &guarantee in ensures {
            let ty = self.check_expression(guarantee);
            self.expect_type(&Type::Boolean, &ty, arena.get(guarantee).span, "'ensures' condition");
        }
        self.scopes.pop();

        let needs_value = self.resolve(&result) != Type::Nothing;
        let value = self.check_statements(body, needs_value);
        if needs_value {
            self.check_function_result(&result, &value, body, node.span);
        }
        self.return_types.pop();
        self.scopes.pop();
    }

    fn define_parameters(&mut self, params: &[Param]) {
        for param in params {
            let ty = self.convert(&param.param_type);
            self.define(&param.name, ty, false);
        }
    }

    fn check_function_result(&mut self, result: &Type, value: &Type, body: &[NodeId], span: Span) {
        let arena = self.arena;
        let Some(&last) = body.last() else {
            self.error(
                ErrorCode::TypeMismatch,
                span,
                format!("Function has no body but must produce '{}'", self.describe(result)),
                "Add a final expression or a 'return' statement".to_string(),
            );
            return;
        };
        let last_node = arena.get(last);
        if matches!(
            last_node.kind,
            NodeKind::LetBinding { .. }
                | NodeKind::SetStatement { .. }
                | NodeKind::ForLoop { .. }
                | NodeKind::WhileLoop { .. }
        ) {
            self.error(
                ErrorCode::TypeMismatch,
                last_node.span,
                format!(
                    "Function must produce '{}' but its last statement produces no value",
                    self.describe(result)
                ),
                "End the function with the value to produce, or with a 'return' statement".to_string(),
            );
            return;
        }
        self.expect_type(result, value, last_node.span, "Function result");
    }

    // ─── Statements ─────────────────────────────────────────

    /// Check a sequence of statements in a fresh scope and return the type of the last one.
    fn check_block(&mut self, statements: &[NodeId], value_needed: bool) -> Type {
        self.scopes.push(HashMap::new());
        let ty = self.check_statements(statements, value_needed);
        self.scopes.pop();
        ty
    }

    fn check_statements(&mut self, statements: &[NodeId], value_needed: bool) -> Type {
        let mut last = Type::Nothing;
        for (index, &statement) in statements.iter().enumerate() {
            let is_last = index + 1 == statements.len();
            last = self.check_statement(statement, value_needed && is_last);
        }
        last
    }

    fn check_statement(&mut self, id: NodeId, value_needed: bool) -> Type {
        let arena = self.arena;
        let node = arena.get(id);
        let previous = self.current_span;
        self.current_span = node.span;
        let ty = match &node.kind {
            NodeKind::LetBinding { name, declared_type, value, mutable } => {
                let declared = self.convert(declared_type);
                let actual = self.check_expression(*value);
                self.expect_type(
                    &declared,
                    &actual,
                    arena.get(*value).span,
                    &format!("Value of '{name}'"),
                );
                self.define(name, declared, *mutable);
                Type::Nothing
            }
            NodeKind::SetStatement { name, value } => {
                self.check_set(name, *value, node.span);
                Type::Nothing
            }
            NodeKind::ForLoop { binding, iterable, body } => {
                self.check_for(binding, *iterable, body);
                Type::Nothing
            }
            NodeKind::WhileLoop { condition, body } => {
                self.check_condition(*condition, "'while' condition");
                self.check_block(body, false);
                Type::Nothing
            }
            NodeKind::ReturnExpr { value } => {
                let returned = match value {
                    Some(value) => self.check_expression(*value),
                    None => Type::Nothing,
                };
                if let Some(expected) = self.return_types.last().cloned() {
                    let span = value.map_or(node.span, |v| arena.get(v).span);
                    self.expect_type(&expected, &returned, span, "Returned value");
                }
                Type::Never
            }
            NodeKind::ExprStatement { expr } => self.check_expression_with(*expr, value_needed),
            _ => self.check_expression_with(id, value_needed),
        };
        self.current_span = previous;
        ty
    }

    fn check_set(&mut self, name: &str, value: NodeId, span: Span) {
        let actual = self.check_expression(value);
        let value_span = self.arena.get(value).span;
        match self.lookup(name) {
            None => self.error(
                ErrorCode::UndefinedVariable,
                span,
                format!("Cannot set undefined variable '{name}'"),
                self.did_you_mean(name),
            ),
            Some(binding) => {
                if !binding.mutable {
                    self.error(
                        ErrorCode::ImmutableReassign,
                        span,
                        format!("Cannot reassign '{name}' because it was not declared 'mutable'"),
                        format!("Declare it with 'mutable {name}: <type> = ...' instead of 'let'"),
                    );
                }
                self.expect_type(&binding.ty, &actual, value_span, &format!("Value assigned to '{name}'"));
            }
        }
    }

    fn check_for(&mut self, binding: &str, iterable: NodeId, body: &[NodeId]) {
        let iterable_type = self.check_expression(iterable);
        let span = self.arena.get(iterable).span;
        let iterable_type = self.unwrap_for_use(iterable_type, span, "as a loop iterable");
        let element = match self.resolve(&iterable_type) {
            Type::List(inner) => *inner,
            Type::Any | Type::Variable(_) | Type::Never => Type::Any,
            other => {
                self.error(
                    ErrorCode::TypeMismatch,
                    span,
                    format!("A 'for' loop needs a list but got '{}'", self.describe(&other)),
                    "Loop over a list; use keys(mapping) or values(mapping) to loop over a mapping".to_string(),
                );
                Type::Any
            }
        };
        self.scopes.push(HashMap::new());
        self.define(binding, element, false);
        self.check_block(body, false);
        self.scopes.pop();
    }

    fn check_condition(&mut self, condition: NodeId, what: &str) {
        let ty = self.check_expression(condition);
        self.expect_type(&Type::Boolean, &ty, self.arena.get(condition).span, what);
    }

    // ─── Expressions ────────────────────────────────────────

    fn check_expression(&mut self, id: NodeId) -> Type {
        self.check_expression_with(id, true)
    }

    fn check_expression_with(&mut self, id: NodeId, value_needed: bool) -> Type {
        let arena = self.arena;
        let node = arena.get(id);
        let previous = self.current_span;
        self.current_span = node.span;
        let ty = match &node.kind {
            NodeKind::IntegerLit(_) => Type::Integer,
            NodeKind::DecimalLit(_) => Type::Decimal,
            NodeKind::TextLit(_) => Type::Text,
            NodeKind::BooleanLit(_) => Type::Boolean,
            NodeKind::NoneLit => Type::Optional(Box::new(self.fresh())),
            NodeKind::InterpolatedText { parts } => {
                for part in parts {
                    if let TextPart::Interpolation(expression) = part {
                        self.check_expression(*expression);
                    }
                }
                Type::Text
            }
            NodeKind::ListLit { elements } => self.check_list_literal(elements),
            NodeKind::MappingLit { entries } => self.check_mapping_literal(entries),
            NodeKind::Identifier(name) => self.check_identifier(name, node.span),
            NodeKind::FieldAccess { object, field } => self.check_field_access(*object, field, node.span),
            NodeKind::FunctionCall { callee, arguments } => {
                self.check_call(*callee, Vec::new(), arguments, node.span)
            }
            NodeKind::Pipeline { left, right } => self.check_pipeline(*left, *right, node.span),
            NodeKind::Lambda { params, return_type, body } => {
                self.check_lambda(params, return_type, *body)
            }
            NodeKind::BinaryOp { left, op, right } => self.check_binary(*left, op, *right, node.span),
            NodeKind::UnaryOp { op, operand } => self.check_unary(op, *operand),
            NodeKind::IfExpr { condition, then_branch, else_branch } => {
                self.check_if(*condition, then_branch, else_branch.as_deref(), node.span, value_needed)
            }
            NodeKind::MatchExpr { subject, arms } => {
                self.check_match(*subject, arms, node.span, value_needed)
            }
            NodeKind::RecordConstruct { type_name, fields } => {
                self.check_record_construct(type_name, fields, node.span)
            }
            NodeKind::RecordUpdate { base, updates } => self.check_record_update(*base, updates),
            NodeKind::UnionConstruct { type_name, variant_name, fields } => {
                self.check_union_construct(type_name, variant_name, fields, node.span)
            }
            NodeKind::OldExpr { inner } => self.check_expression(*inner),
            _ => self.check_statement(id, value_needed),
        };
        self.current_span = previous;
        ty
    }

    fn check_list_literal(&mut self, elements: &[NodeId]) -> Type {
        let element_type = self.fresh();
        for &element in elements {
            let ty = self.check_expression(element);
            self.expect_type(&element_type, &ty, self.arena.get(element).span, "List element");
        }
        Type::List(Box::new(element_type))
    }

    fn check_mapping_literal(&mut self, entries: &[(NodeId, NodeId)]) -> Type {
        let key_type = self.fresh();
        let mut value_type = self.fresh();
        let mut mixed = false;
        for &(key, value) in entries {
            let key_actual = self.check_expression(key);
            self.expect_type(&key_type, &key_actual, self.arena.get(key).span, "Mapping key");
            let value_actual = self.check_expression(value);
            if !mixed && !self.compatible(&value_type, &value_actual) {
                // Values of different types make this a JSON-style object.
                mixed = true;
                value_type = Type::Any;
            }
        }
        Type::Mapping(Box::new(key_type), Box::new(value_type))
    }

    fn check_identifier(&mut self, name: &str, span: Span) -> Type {
        if let Some(binding) = self.lookup(name) {
            return binding.ty;
        }
        if let Some(signature) = self.functions.get(name).cloned() {
            let params: Vec<Type> = signature.params.iter().map(|p| self.convert(p)).collect();
            let result = self.convert(&signature.result);
            return Type::Function(params, Box::new(result));
        }
        if let Some(signature) = builtin_signature(name) {
            let (params, result) = self.instantiate(&signature);
            return Type::Function(params, Box::new(result));
        }
        if self.modules.contains_key(name) || self.unavailable_modules.contains(name) {
            return Type::Module(name.to_string());
        }
        if self.unions.contains_key(name) {
            return Type::UnionNamespace(name.to_string());
        }
        self.error(
            ErrorCode::UndefinedVariable,
            span,
            format!("Undefined variable '{name}'"),
            self.did_you_mean(name),
        );
        Type::Any
    }

    fn instantiate(&mut self, signature: &BuiltinSignature) -> (Vec<Type>, Type) {
        let mut generics = HashMap::new();
        let params = signature
            .params
            .iter()
            .map(|p| self.convert_signature(p, &mut generics))
            .collect();
        let result = self.convert_signature(&signature.result, &mut generics);
        (params, result)
    }

    fn check_field_access(&mut self, object: NodeId, field: &str, span: Span) -> Type {
        let object_type = self.check_expression(object);
        let object_span = self.arena.get(object).span;
        let object_type = self.unwrap_for_use(object_type, object_span, &format!("to read '.{field}'"));
        match self.resolve(&object_type) {
            Type::Record(name) => self.record_field(&name, field, span),
            Type::Union(name) => self.union_field(&name, field, span),
            Type::Module(name) => self.module_member(&name, field, span),
            Type::UnionNamespace(name) => {
                let is_variant = self.unions.get(&name).is_some_and(|vs| vs.iter().any(|v| v.name == field));
                if is_variant {
                    Type::Union(name)
                } else {
                    self.error(
                        ErrorCode::UndefinedVariable,
                        span,
                        format!("Union '{name}' has no variant '{field}'"),
                        format!("Available variants: {}", self.variant_names(&name)),
                    );
                    Type::Any
                }
            }
            Type::Any | Type::Variable(_) | Type::Never => Type::Any,
            other => {
                self.error(
                    ErrorCode::TypeMismatch,
                    span,
                    format!("Cannot read field '{field}' of a value of type '{}'", self.describe(&other)),
                    "Field access works on records and union variants only".to_string(),
                );
                Type::Any
            }
        }
    }

    fn variant_names(&self, union: &str) -> String {
        self.unions
            .get(union)
            .map(|variants| variants.iter().map(|v| v.name.clone()).collect::<Vec<_>>().join(", "))
            .unwrap_or_default()
    }

    fn record_field(&mut self, record: &str, field: &str, span: Span) -> Type {
        let found = self
            .records
            .get(record)
            .and_then(|fields| fields.iter().find(|f| f.name == field))
            .map(|f| f.field_type.clone());
        if let Some(declared) = found {
            self.convert(&declared)
        } else {
            let available = self
                .records
                .get(record)
                .map(|fields| fields.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", "))
                .unwrap_or_default();
            self.error(
                ErrorCode::UndefinedVariable,
                span,
                format!("Record '{record}' has no field '{field}'"),
                format!("Available fields: {available}"),
            );
            Type::Any
        }
    }

    fn union_field(&mut self, union: &str, field: &str, span: Span) -> Type {
        let found = self.unions.get(union).and_then(|variants| {
            variants
                .iter()
                .flat_map(|v| v.fields.iter())
                .find(|f| f.name == field)
                .map(|f| f.field_type.clone())
        });
        if found.is_some() {
            // Different variants may give the field different types.
            Type::Any
        } else {
            self.error(
                ErrorCode::UndefinedVariable,
                span,
                format!("No variant of union '{union}' has a field '{field}'"),
                "Use a 'match' to destructure the variant first".to_string(),
            );
            Type::Any
        }
    }

    fn module_member(&mut self, module: &str, member: &str, span: Span) -> Type {
        if self.unavailable_modules.contains(module) {
            return Type::Any;
        }
        let signature = self.modules.get(module).and_then(|functions| functions.get(member)).cloned();
        if let Some(signature) = signature {
            let params: Vec<Type> = signature.params.iter().map(|p| self.convert(p)).collect();
            let result = self.convert(&signature.result);
            Type::Function(params, Box::new(result))
        } else {
            self.error(
                ErrorCode::UndefinedFunction,
                span,
                format!("Module '{module}' has no public function '{member}'"),
                format!("Mark '{member}' as 'public function' in {module}.lbl, or check the spelling"),
            );
            Type::Any
        }
    }

    // ─── Calls ──────────────────────────────────────────────

    fn check_pipeline(&mut self, left: NodeId, right: NodeId, span: Span) -> Type {
        let arena = self.arena;
        let left_type = self.check_expression(left);
        let prefix = vec![(left_type, arena.get(left).span)];
        match &arena.get(right).kind {
            NodeKind::FunctionCall { callee, arguments } => {
                self.check_call(*callee, prefix, arguments, arena.get(right).span)
            }
            NodeKind::Identifier(_) => self.check_call(right, prefix, &[], arena.get(right).span),
            _ => {
                self.error(
                    ErrorCode::TypeMismatch,
                    arena.get(right).span,
                    "The right side of '|>' must be a function call".to_string(),
                    "Write the stage as a call such as `x |> f(y)`; to apply a lambda, call it with the value or extract it into a named function".to_string(),
                );
                let _ = span;
                Type::Any
            }
        }
    }

    fn check_call(
        &mut self,
        callee: NodeId,
        prefix: Vec<(Type, Span)>,
        arguments: &[NodeId],
        span: Span,
    ) -> Type {
        let arena = self.arena;
        let mut argument_types = prefix;
        for &argument in arguments {
            let ty = self.check_expression(argument);
            argument_types.push((ty, arena.get(argument).span));
        }
        let callee_node = arena.get(callee);
        if let NodeKind::Identifier(name) = &callee_node.kind {
            if self.lookup(name).is_none() {
                return self.check_named_call(name, &argument_types, callee_node.span, span);
            }
        }
        let callee_type = self.check_expression(callee);
        self.apply_function_type(&callee_type, &argument_types, "function value", callee_node.span, span)
    }

    fn check_named_call(
        &mut self,
        name: &str,
        arguments: &[(Type, Span)],
        callee_span: Span,
        call_span: Span,
    ) -> Type {
        if let Some(signature) = self.functions.get(name).cloned() {
            let params: Vec<Type> = signature.params.iter().map(|p| self.convert(p)).collect();
            let result = self.convert(&signature.result);
            return self.apply_signature(name, &params, false, &result, arguments, call_span);
        }
        if name == "get" {
            return self.check_get(arguments, call_span);
        }
        if (name == "to_integer" || name == "to_decimal") && arguments.len() == 1 {
            return self.check_number_conversion(name == "to_integer", &arguments[0]);
        }
        if let Some(signature) = builtin_signature(name) {
            return self.check_builtin(name, &signature, arguments, call_span);
        }
        if self.modules.contains_key(name) || self.unions.contains_key(name) || self.records.contains_key(name) {
            self.error(
                ErrorCode::TypeMismatch,
                callee_span,
                format!("'{name}' is not a function"),
                "Call one of its members, for example module.function(...)".to_string(),
            );
            return Type::Any;
        }
        self.error(
            ErrorCode::UndefinedFunction,
            callee_span,
            format!("Undefined function '{name}'"),
            self.did_you_mean(name),
        );
        Type::Any
    }

    fn check_builtin(
        &mut self,
        name: &str,
        signature: &BuiltinSignature,
        arguments: &[(Type, Span)],
        call_span: Span,
    ) -> Type {
        let (params, result) = self.instantiate(signature);
        let numeric_result = matches!(signature.result, SignatureType::Number);
        let resolved = self.apply_signature(name, &params, signature.variadic, &result, arguments, call_span);
        if numeric_result {
            return self.numeric_result(arguments);
        }
        resolved
    }

    /// The result type of `abs`, `max` and `min`: integer for integers, decimal otherwise.
    fn numeric_result(&self, arguments: &[(Type, Span)]) -> Type {
        let kinds: Vec<Type> = arguments.iter().map(|(ty, _)| self.resolve(ty)).collect();
        if kinds.iter().all(|k| *k == Type::Integer) {
            Type::Integer
        } else if kinds.iter().all(|k| matches!(k, Type::Integer | Type::Decimal)) {
            Type::Decimal
        } else {
            Type::Any
        }
    }

    /// `to_integer` / `to_decimal` return the number itself for a number and an
    /// optional for text, because parsing text can fail.
    fn check_number_conversion(&mut self, to_integer: bool, argument: &(Type, Span)) -> Type {
        let target = if to_integer { Type::Integer } else { Type::Decimal };
        match self.resolve(&argument.0) {
            Type::Integer | Type::Decimal => target,
            Type::Text | Type::Any | Type::Variable(_) | Type::Never => Type::Optional(Box::new(target)),
            other => {
                self.error(
                    ErrorCode::TypeMismatch,
                    argument.1,
                    format!(
                        "Argument 1 of {}(): expected text or a number but got '{}'",
                        if to_integer { "to_integer" } else { "to_decimal" },
                        self.describe(&other)
                    ),
                    "Pass a text to parse, or a number to convert".to_string(),
                );
                Type::Optional(Box::new(target))
            }
        }
    }

    fn check_get(&mut self, arguments: &[(Type, Span)], call_span: Span) -> Type {
        if arguments.len() != 2 {
            self.error(
                ErrorCode::TypeMismatch,
                call_span,
                format!("get() takes 2 arguments but {} were given", arguments.len()),
                "Usage: get(mapping, key) or get(list, index)".to_string(),
            );
            return Type::Any;
        }
        let (container, container_span) = arguments[0].clone();
        let (key, key_span) = arguments[1].clone();
        let container = self.unwrap_for_use(container, container_span, "as the first argument of get()");
        match self.resolve(&container) {
            Type::Mapping(key_type, value_type) => {
                self.expect_type(&key_type, &key, key_span, "Argument 2 of get()");
                Type::Optional(value_type)
            }
            Type::List(element) => {
                self.expect_type(&Type::Integer, &key, key_span, "Argument 2 of get()");
                Type::Optional(element)
            }
            Type::Any | Type::Variable(_) | Type::Never => Type::Any,
            other => {
                self.error(
                    ErrorCode::TypeMismatch,
                    container_span,
                    format!("Argument 1 of get(): expected a mapping or a list but got '{}'", self.describe(&other)),
                    "Pass a mapping with a key, or a list with an integer index".to_string(),
                );
                Type::Any
            }
        }
    }

    fn apply_function_type(
        &mut self,
        callee_type: &Type,
        arguments: &[(Type, Span)],
        what: &str,
        callee_span: Span,
        call_span: Span,
    ) -> Type {
        match self.resolve(callee_type) {
            Type::Function(params, result) => {
                self.apply_signature(what, &params, false, &result, arguments, call_span)
            }
            Type::Any | Type::Variable(_) | Type::Never => Type::Any,
            other => {
                self.error(
                    ErrorCode::TypeMismatch,
                    callee_span,
                    format!("Cannot call a value of type '{}'", self.describe(&other)),
                    "Only functions can be called".to_string(),
                );
                Type::Any
            }
        }
    }

    fn apply_signature(
        &mut self,
        name: &str,
        params: &[Type],
        variadic: bool,
        result: &Type,
        arguments: &[(Type, Span)],
        call_span: Span,
    ) -> Type {
        let too_few = arguments.len() < params.len();
        let too_many = arguments.len() > params.len() && !variadic;
        if too_few || too_many {
            self.error(
                ErrorCode::TypeMismatch,
                call_span,
                format!(
                    "'{name}' takes {} argument{} but {} {} given",
                    params.len(),
                    if params.len() == 1 { "" } else { "s" },
                    arguments.len(),
                    if arguments.len() == 1 { "was" } else { "were" },
                ),
                if too_few {
                    "Pass every parameter; Legible has no default parameters".to_string()
                } else {
                    "Remove the extra arguments".to_string()
                },
            );
            return result.clone();
        }
        for (index, (parameter, (argument, span))) in params.iter().zip(arguments).enumerate() {
            self.expect_type(
                parameter,
                argument,
                *span,
                &format!("Argument {} of {name}()", index + 1),
            );
        }
        result.clone()
    }

    fn check_lambda(&mut self, params: &[Param], return_type: &LegibleType, body: NodeId) -> Type {
        let declared = self.convert(return_type);
        self.scopes.push(HashMap::new());
        let mut parameter_types = Vec::new();
        for param in params {
            let ty = self.convert(&param.param_type);
            self.define(&param.name, ty.clone(), false);
            parameter_types.push(ty);
        }
        self.return_types.push(declared.clone());
        let actual = self.check_expression(body);
        self.expect_type(&declared, &actual, self.arena.get(body).span, "Lambda body");
        self.return_types.pop();
        self.scopes.pop();
        Type::Function(parameter_types, Box::new(declared))
    }

    // ─── Operators ──────────────────────────────────────────

    fn check_binary(&mut self, left: NodeId, op: &BinaryOperator, right: NodeId, span: Span) -> Type {
        let arena = self.arena;
        let left_type = self.check_expression(left);
        let right_type = self.check_expression(right);
        let (left_span, right_span) = (arena.get(left).span, arena.get(right).span);
        match op {
            BinaryOperator::And | BinaryOperator::Or => {
                let word = if *op == BinaryOperator::And { "and" } else { "or" };
                self.expect_type(&Type::Boolean, &left_type, left_span, &format!("Left side of '{word}'"));
                self.expect_type(&Type::Boolean, &right_type, right_span, &format!("Right side of '{word}'"));
                Type::Boolean
            }
            BinaryOperator::Concat => {
                self.expect_type(&Type::Text, &left_type, left_span, "Left side of '++'");
                self.expect_type(&Type::Text, &right_type, right_span, "Right side of '++'");
                Type::Text
            }
            BinaryOperator::Eq | BinaryOperator::NotEq => {
                if !self.compatible(&left_type, &right_type) && !self.compatible(&right_type, &left_type) {
                    self.error(
                        ErrorCode::TypeMismatch,
                        span,
                        format!(
                            "Cannot compare '{}' with '{}'",
                            self.describe(&left_type),
                            self.describe(&right_type)
                        ),
                        "Compare values of the same type; convert one side with to_text(), to_integer() or to_decimal()".to_string(),
                    );
                }
                Type::Boolean
            }
            BinaryOperator::Gt | BinaryOperator::Lt | BinaryOperator::GtEq | BinaryOperator::LtEq => {
                self.check_ordering(left_type, right_type, left_span, right_span, span);
                Type::Boolean
            }
            BinaryOperator::Add | BinaryOperator::Sub | BinaryOperator::Mul | BinaryOperator::Div | BinaryOperator::Mod => {
                self.check_arithmetic(op, left_type, right_type, left_span, right_span, span)
            }
        }
    }

    fn check_ordering(&mut self, left: Type, right: Type, left_span: Span, right_span: Span, span: Span) {
        let left = self.unwrap_for_use(left, left_span, "in a comparison");
        let right = self.unwrap_for_use(right, right_span, "in a comparison");
        let (l, r) = (self.resolve(&left), self.resolve(&right));
        let numeric = |t: &Type| matches!(t, Type::Integer | Type::Decimal);
        let dynamic = |t: &Type| matches!(t, Type::Any | Type::Variable(_) | Type::Never);
        let ok = dynamic(&l)
            || dynamic(&r)
            || (numeric(&l) && numeric(&r))
            || (l == Type::Text && r == Type::Text);
        if !ok {
            self.error(
                ErrorCode::TypeMismatch,
                span,
                format!(
                    "Cannot order '{}' and '{}'",
                    self.describe(&l),
                    self.describe(&r)
                ),
                "Compare two numbers or two texts".to_string(),
            );
        }
    }

    fn check_arithmetic(
        &mut self,
        op: &BinaryOperator,
        left: Type,
        right: Type,
        left_span: Span,
        right_span: Span,
        span: Span,
    ) -> Type {
        let left = self.unwrap_for_use(left, left_span, "in arithmetic");
        let right = self.unwrap_for_use(right, right_span, "in arithmetic");
        let (l, r) = (self.resolve(&left), self.resolve(&right));
        let dynamic = |t: &Type| matches!(t, Type::Any | Type::Variable(_) | Type::Never);
        let numeric = |t: &Type| matches!(t, Type::Integer | Type::Decimal);
        let symbol = match op {
            BinaryOperator::Add => "+",
            BinaryOperator::Sub => "-",
            BinaryOperator::Mul => "*",
            BinaryOperator::Div => "/",
            _ => "%",
        };
        if (!dynamic(&l) && !numeric(&l)) || (!dynamic(&r) && !numeric(&r)) {
            let suggestion = if *op == BinaryOperator::Add && (l == Type::Text || r == Type::Text) {
                "Join text with '++' and convert numbers with to_text()".to_string()
            } else {
                "Arithmetic works on integers and decimals only".to_string()
            };
            self.error(
                ErrorCode::TypeMismatch,
                span,
                format!("Cannot apply '{symbol}' to '{}' and '{}'", self.describe(&l), self.describe(&r)),
                suggestion,
            );
            return Type::Any;
        }
        if *op == BinaryOperator::Mod {
            if l == Type::Decimal || r == Type::Decimal {
                self.error(
                    ErrorCode::TypeMismatch,
                    span,
                    "'%' requires integer operands".to_string(),
                    "Use floor() or round() to get integers first".to_string(),
                );
            }
            return Type::Integer;
        }
        if dynamic(&l) || dynamic(&r) {
            Type::Any
        } else if l == Type::Integer && r == Type::Integer {
            Type::Integer
        } else {
            Type::Decimal
        }
    }

    fn check_unary(&mut self, op: &UnaryOperator, operand: NodeId) -> Type {
        let ty = self.check_expression(operand);
        let span = self.arena.get(operand).span;
        match op {
            UnaryOperator::Not => {
                self.expect_type(&Type::Boolean, &ty, span, "Operand of 'not'");
                Type::Boolean
            }
            UnaryOperator::Negate => {
                let resolved = self.resolve(&ty);
                match resolved {
                    Type::Integer | Type::Decimal | Type::Any | Type::Variable(_) | Type::Never => ty,
                    other => {
                        self.error(
                            ErrorCode::TypeMismatch,
                            span,
                            format!("Cannot negate a value of type '{}'", self.describe(&other)),
                            "Negation works on integers and decimals only".to_string(),
                        );
                        Type::Any
                    }
                }
            }
        }
    }

    // ─── Branching ──────────────────────────────────────────

    fn check_if(
        &mut self,
        condition: NodeId,
        then_branch: &[NodeId],
        else_branch: Option<&[NodeId]>,
        span: Span,
        value_needed: bool,
    ) -> Type {
        self.check_condition(condition, "'if' condition");
        let then_type = self.check_block(then_branch, value_needed);
        match else_branch {
            Some(branch) => {
                let else_type = self.check_block(branch, value_needed);
                self.join(&then_type, &else_type, span, "The branches of this 'if'", value_needed)
            }
            None if !value_needed => Type::Nothing,
            None => match self.resolve(&then_type) {
                Type::Nothing | Type::Never => Type::Nothing,
                other => Type::Optional(Box::new(other)),
            },
        }
    }

    /// Combine the types of two alternative branches.
    fn join(&mut self, first: &Type, second: &Type, span: Span, what: &str, value_needed: bool) -> Type {
        if !value_needed {
            return Type::Any;
        }
        let (a, b) = (self.resolve(first), self.resolve(second));
        if a == Type::Never {
            return b;
        }
        if b == Type::Never {
            return a;
        }
        if a == Type::Any {
            return b;
        }
        if self.compatible(&a, &b) {
            return a;
        }
        if self.compatible(&b, &a) {
            return b;
        }
        self.error(
            ErrorCode::TypeMismatch,
            span,
            format!(
                "{what} produce different types: '{}' and '{}'",
                self.describe(&a),
                self.describe(&b)
            ),
            "Make every branch produce the same type, or use the branches only as statements".to_string(),
        );
        Type::Any
    }

    fn check_match(
        &mut self,
        subject: NodeId,
        arms: &[crate::parser::ast::MatchArm],
        span: Span,
        value_needed: bool,
    ) -> Type {
        let arena = self.arena;
        let subject_type = self.check_expression(subject);
        let subject_span = arena.get(subject).span;
        let subject_type = self.unwrap_for_use(subject_type, subject_span, "as a match subject");
        let resolved_subject = self.resolve(&subject_type);
        let mut result = Type::Never;
        let mut covered: HashSet<String> = HashSet::new();
        let mut has_otherwise = false;
        let mut boolean_cases: HashSet<bool> = HashSet::new();
        for arm in arms {
            self.scopes.push(HashMap::new());
            match &arm.pattern {
                Pattern::Otherwise => has_otherwise = true,
                Pattern::Literal(literal) => {
                    let literal_type = self.check_expression(*literal);
                    if let NodeKind::BooleanLit(value) = arena.get(*literal).kind {
                        boolean_cases.insert(value);
                    }
                    self.expect_type(&subject_type, &literal_type, arena.get(*literal).span, "Match pattern");
                }
                Pattern::Variant { name, bindings } => {
                    covered.insert(name.clone());
                    self.check_variant_pattern(&resolved_subject, name, bindings, span);
                }
            }
            let body_type = self.check_statements(&arm.body, value_needed);
            result = self.join(&result, &body_type, span, "The arms of this 'match'", value_needed);
            self.scopes.pop();
        }
        if !has_otherwise {
            self.check_exhaustive(&resolved_subject, &covered, &boolean_cases, arms, span);
        }
        if value_needed {
            result
        } else {
            Type::Nothing
        }
    }

    fn check_variant_pattern(&mut self, subject: &Type, name: &str, bindings: &[String], span: Span) {
        match subject {
            Type::Union(union) => {
                let variant = self
                    .unions
                    .get(union)
                    .and_then(|variants| variants.iter().find(|v| v.name == name))
                    .cloned();
                if let Some(variant) = variant {
                    if bindings.len() > variant.fields.len() {
                        self.error(
                            ErrorCode::TypeMismatch,
                            span,
                            format!(
                                "Pattern '{name}' binds {} names but the variant has {} field{}",
                                bindings.len(),
                                variant.fields.len(),
                                if variant.fields.len() == 1 { "" } else { "s" }
                            ),
                            "Bind at most one name per field, in declaration order".to_string(),
                        );
                    }
                    for (index, binding) in bindings.iter().enumerate() {
                        let ty = match variant.fields.get(index) {
                            Some(field) => self.convert(&field.field_type),
                            None => Type::Any,
                        };
                        self.define(binding, ty, false);
                    }
                } else {
                    self.error(
                        ErrorCode::TypeMismatch,
                        span,
                        format!("Union '{union}' has no variant '{name}'"),
                        format!("Available variants: {}", self.variant_names(union)),
                    );
                    self.define_all_any(bindings);
                }
            }
            Type::Any | Type::Variable(_) | Type::Never => self.define_all_any(bindings),
            other => {
                self.error(
                    ErrorCode::TypeMismatch,
                    span,
                    format!("Variant pattern '{name}' cannot match a value of type '{}'", self.describe(other)),
                    "Variant patterns work on tagged unions; use a literal or 'otherwise' here".to_string(),
                );
                self.define_all_any(bindings);
            }
        }
    }

    fn define_all_any(&mut self, bindings: &[String]) {
        for binding in bindings {
            self.define(binding, Type::Any, false);
        }
    }

    fn check_exhaustive(
        &mut self,
        subject: &Type,
        covered: &HashSet<String>,
        boolean_cases: &HashSet<bool>,
        arms: &[crate::parser::ast::MatchArm],
        span: Span,
    ) {
        match subject {
            Type::Union(union) => {
                let missing: Vec<String> = self
                    .unions
                    .get(union)
                    .map(|variants| {
                        variants
                            .iter()
                            .filter(|v| !covered.contains(&v.name))
                            .map(|v| v.name.clone())
                            .collect()
                    })
                    .unwrap_or_default();
                if !missing.is_empty() {
                    self.error(
                        ErrorCode::Exhaustiveness,
                        span,
                        format!("This 'match' does not cover: {}", missing.join(", ")),
                        "Add a 'when' arm for each missing variant, or add an 'otherwise' arm".to_string(),
                    );
                }
            }
            Type::Boolean if boolean_cases.len() == 2 => {}
            Type::Any | Type::Variable(_) | Type::Never if !arms.is_empty() && !covered.is_empty() => {}
            _ => self.error(
                ErrorCode::Exhaustiveness,
                span,
                "This 'match' has no 'otherwise' arm, so it can fail at runtime".to_string(),
                "Add 'otherwise then ...' as the last arm".to_string(),
            ),
        }
    }

    // ─── Records and unions ─────────────────────────────────

    fn check_field_values(
        &mut self,
        owner: &str,
        declared: &[Field],
        provided: &[(String, NodeId)],
        span: Span,
    ) {
        for (name, value) in provided {
            let actual = self.check_expression(*value);
            match declared.iter().find(|f| &f.name == name) {
                Some(field) => {
                    let expected = self.convert(&field.field_type);
                    self.expect_type(
                        &expected,
                        &actual,
                        self.arena.get(*value).span,
                        &format!("Field '{name}' of {owner}"),
                    );
                }
                None => self.error(
                    ErrorCode::TypeMismatch,
                    self.arena.get(*value).span,
                    format!("{owner} has no field '{name}'"),
                    format!(
                        "Available fields: {}",
                        declared.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ")
                    ),
                ),
            }
        }
        let missing: Vec<&str> = declared
            .iter()
            .filter(|f| !provided.iter().any(|(name, _)| name == &f.name))
            .map(|f| f.name.as_str())
            .collect();
        if !missing.is_empty() {
            self.error(
                ErrorCode::TypeMismatch,
                span,
                format!("{owner} is missing field{}: {}", if missing.len() == 1 { "" } else { "s" }, missing.join(", ")),
                "Provide every field; use 'none' for an optional field with no value".to_string(),
            );
        }
    }

    fn check_record_construct(&mut self, type_name: &str, fields: &[(String, NodeId)], span: Span) -> Type {
        let Some(declared) = self.records.get(type_name).cloned() else {
            for (_, value) in fields {
                self.check_expression(*value);
            }
            self.error(
                ErrorCode::TypeMismatch,
                span,
                format!("Unknown record type '{type_name}'"),
                format!("Declare 'record {type_name}' with its fields before constructing it"),
            );
            return Type::Any;
        };
        self.check_field_values(&format!("Record '{type_name}'"), &declared, fields, span);
        Type::Record(type_name.to_string())
    }

    fn check_record_update(&mut self, base: NodeId, updates: &[(String, NodeId)]) -> Type {
        let base_type = self.check_expression(base);
        let base_span = self.arena.get(base).span;
        let base_type = self.unwrap_for_use(base_type, base_span, "with 'with'");
        let resolved = self.resolve(&base_type);
        match &resolved {
            Type::Record(name) => {
                let declared = self.records.get(name).cloned().unwrap_or_default();
                for (field_name, value) in updates {
                    let actual = self.check_expression(*value);
                    match declared.iter().find(|f| &f.name == field_name) {
                        Some(field) => {
                            let expected = self.convert(&field.field_type);
                            self.expect_type(
                                &expected,
                                &actual,
                                self.arena.get(*value).span,
                                &format!("Field '{field_name}' of {name}"),
                            );
                        }
                        None => self.error(
                            ErrorCode::TypeMismatch,
                            self.arena.get(*value).span,
                            format!("Record '{name}' has no field '{field_name}'"),
                            format!(
                                "Available fields: {}",
                                declared.iter().map(|f| f.name.clone()).collect::<Vec<_>>().join(", ")
                            ),
                        ),
                    }
                }
                resolved
            }
            Type::Any | Type::Variable(_) | Type::Never => {
                for (_, value) in updates {
                    self.check_expression(*value);
                }
                Type::Any
            }
            other => {
                for (_, value) in updates {
                    self.check_expression(*value);
                }
                self.error(
                    ErrorCode::TypeMismatch,
                    base_span,
                    format!("'with' needs a record but got '{}'", self.describe(other)),
                    "Use 'with' on a record value".to_string(),
                );
                Type::Any
            }
        }
    }

    fn check_union_construct(
        &mut self,
        type_name: &str,
        variant_name: &str,
        fields: &[(String, NodeId)],
        span: Span,
    ) -> Type {
        if let Some(variants) = self.unions.get(type_name).cloned() {
            return match variants.iter().find(|v| v.name == variant_name) {
                Some(variant) => {
                    let owner = format!("Variant '{type_name}.{variant_name}'");
                    self.check_field_values(&owner, &variant.fields, fields, span);
                    Type::Union(type_name.to_string())
                }
                None => {
                    for (_, value) in fields {
                        self.check_expression(*value);
                    }
                    self.error(
                        ErrorCode::UndefinedVariable,
                        span,
                        format!("Union '{type_name}' has no variant '{variant_name}'"),
                        format!("Available variants: {}", self.variant_names(type_name)),
                    );
                    Type::Any
                }
            };
        }
        // `module.Record { ... }` is parsed the same way as a variant construction.
        if self.modules.contains_key(type_name) && self.records.contains_key(variant_name) {
            return self.check_record_construct(variant_name, fields, span);
        }
        for (_, value) in fields {
            self.check_expression(*value);
        }
        if self.unavailable_modules.contains(type_name) {
            return Type::Any;
        }
        self.error(
            ErrorCode::TypeMismatch,
            span,
            format!("Unknown union or record '{type_name}.{variant_name}'"),
            format!("Declare 'union {type_name}' with a '{variant_name}' variant"),
        );
        Type::Any
    }
}

/// Legible has no type for a JSON object, so `a mapping from text to text` is
/// the conventional type of decoded JSON and of database rows, and programs
/// store numbers, lists and nested mappings in it. Its values are therefore
/// treated as dynamic (`any`) instead of literally `text`.
fn json_object_convention(key: Type, value: Type) -> Type {
    if key == Type::Text && value == Type::Text {
        Type::Mapping(Box::new(Type::Text), Box::new(Type::Any))
    } else {
        Type::Mapping(Box::new(key), Box::new(value))
    }
}

/// Levenshtein distance, used to suggest the name the author probably meant.
fn edit_distance(first: &str, second: &str) -> usize {
    let second: Vec<char> = second.chars().collect();
    let mut previous: Vec<usize> = (0..=second.len()).collect();
    for (row, first_character) in first.chars().enumerate() {
        let mut current = vec![row + 1];
        for (column, second_character) in second.iter().enumerate() {
            let substitution = previous[column] + usize::from(first_character != *second_character);
            current.push(substitution.min(previous[column + 1] + 1).min(current[column] + 1));
        }
        previous = current;
    }
    previous[second.len()]
}
