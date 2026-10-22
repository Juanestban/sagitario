//! Methods of `String`, `char` and `StringBuilder`, and `new StringBuilder()`
//! (docs/spec/02-types.md#string-f1). Most methods are runtime functions taking the receiver and
//! the arguments in order; a few build an `Option` or `Result` and are lowered specially.

use sagitario_ast::{Expr, Ident, Path};
use sagitario_diagnostics::{Diagnostic, codes};

use sagitario_resolve::TypeRes;

use crate::{BuiltinMethod, Checker, FloatTy, IntTy, Type};

/// A method implemented by a runtime function: its name, parameters, result and whether it
/// also receives the source location (to report panics).
struct RuntimeMethod {
  name: &'static str,
  params: Vec<Type>,
  ret: Type,
  function: &'static str,
  located: bool,
}

fn runtime(name: &'static str, params: Vec<Type>, ret: Type, function: &'static str) -> RuntimeMethod {
  RuntimeMethod { name, params, ret, function, located: false }
}

const USIZE: Type = Type::Int(IntTy::Usize);

fn string_methods() -> Vec<RuntimeMethod> {
  vec![
    runtime("len", Vec::new(), USIZE, "sag_string_len"),
    runtime("as_ptr", Vec::new(), Type::Ptr { mutable: false, pointee: Box::new(Type::Int(IntTy::U8)) }, "sag_string_ptr"),
    runtime("is_empty", Vec::new(), Type::Bool, "sag_string_is_empty"),
    runtime("contains", vec![Type::String], Type::Bool, "sag_string_contains"),
    runtime("starts_with", vec![Type::String], Type::Bool, "sag_string_starts_with"),
    runtime("ends_with", vec![Type::String], Type::Bool, "sag_string_ends_with"),
    runtime("trim", Vec::new(), Type::String, "sag_string_trim"),
    runtime("trim_start", Vec::new(), Type::String, "sag_string_trim_start"),
    runtime("trim_end", Vec::new(), Type::String, "sag_string_trim_end"),
    runtime("to_ascii_uppercase", Vec::new(), Type::String, "sag_string_to_ascii_uppercase"),
    runtime("to_ascii_lowercase", Vec::new(), Type::String, "sag_string_to_ascii_lowercase"),
    runtime("replace", vec![Type::String, Type::String], Type::String, "sag_string_replace"),
    RuntimeMethod { located: true, ..runtime("repeat", vec![USIZE], Type::String, "sag_string_repeat_at") },
    RuntimeMethod { located: true, ..runtime("substring", vec![USIZE, USIZE], Type::String, "sag_string_substring_at") },
  ]
}

fn char_methods() -> Vec<RuntimeMethod> {
  vec![
    runtime("is_ascii_digit", Vec::new(), Type::Bool, "sag_char_is_ascii_digit"),
    runtime("is_ascii_alphabetic", Vec::new(), Type::Bool, "sag_char_is_ascii_alphabetic"),
    runtime("is_ascii_alphanumeric", Vec::new(), Type::Bool, "sag_char_is_ascii_alphanumeric"),
    runtime("is_ascii_whitespace", Vec::new(), Type::Bool, "sag_char_is_ascii_whitespace"),
    runtime("is_ascii_uppercase", Vec::new(), Type::Bool, "sag_char_is_ascii_uppercase"),
    runtime("is_ascii_lowercase", Vec::new(), Type::Bool, "sag_char_is_ascii_lowercase"),
    runtime("to_ascii_uppercase", Vec::new(), Type::Char, "sag_char_to_ascii_uppercase"),
    runtime("to_ascii_lowercase", Vec::new(), Type::Char, "sag_char_to_ascii_lowercase"),
  ]
}

fn builder_methods() -> Vec<RuntimeMethod> {
  vec![
    runtime("append", vec![Type::String], Type::Unit, "sag_builder_append"),
    runtime("append_char", vec![Type::Char], Type::Unit, "sag_builder_append_char"),
    runtime("len", Vec::new(), USIZE, "sag_builder_len"),
    runtime("is_empty", Vec::new(), Type::Bool, "sag_builder_is_empty"),
    runtime("clear", Vec::new(), Type::Unit, "sag_builder_clear"),
    runtime("to_string", Vec::new(), Type::String, "sag_builder_to_string"),
  ]
}

/// The type parsed by `parse_<name>`.
fn parse_target(method: &str) -> Option<Type> {
  let name = method.strip_prefix("parse_")?;

  match name {
    "f32" => Some(Type::Float(FloatTy::F32)),
    "f64" => Some(Type::Float(FloatTy::F64)),
    _ => IntTy::from_name(name).map(Type::Int),
  }
}

impl Checker<'_> {
  /// A method call on a `String`, `char` or `StringBuilder`.
  pub(crate) fn text_method(&mut self, expr: &Expr, receiver_ty: &Type, method: &Ident, args: &[Expr]) -> Type {
    let name = method.name.as_str();
    let special = match (receiver_ty, name) {
      (Type::String, "find") => Some((BuiltinMethod::Find, vec![Type::String], Type::Option(Box::new(USIZE)))),
      (Type::Char, "to_digit") => Some((BuiltinMethod::ToDigit, vec![Type::Int(IntTy::U32)], Type::Option(Box::new(Type::Int(IntTy::U32))))),
      (Type::String, _) => parse_target(name).map(|target| {
        let ret = Type::Result(Box::new(target.clone()), Box::new(Type::String));

        (BuiltinMethod::Parse(target), Vec::new(), ret)
      }),
      _ => None,
    };

    if let Some((builtin, params, ret)) = special {
      self.args(expr.span, "method", &params, args);
      self.typed.builtin_methods.insert(expr.id, builtin);

      return ret;
    }

    if matches!(receiver_ty, Type::String) && matches!(name, "chars" | "bytes") {
      self.diagnostics.push(
        Diagnostic::error(codes::NOT_IMPLEMENTED, method.span, format!("`{name}()` can only be the iterable of a `for` loop in F1"))
          .with_note("iterators arrive in F2; write `for c in text.chars() { ... }`"),
      );
      self.unchecked_args(args);

      return Type::Error;
    }

    let methods = match receiver_ty {
      Type::String => string_methods(),
      Type::Char => char_methods(),
      _ => builder_methods(),
    };
    let Some(found) = methods.into_iter().find(|candidate| candidate.name == name) else {
      let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{receiver_ty}`"));
      let mut names: Vec<&str> = match receiver_ty {
        Type::String => string_methods().iter().map(|method| method.name).collect(),
        Type::Char => char_methods().iter().map(|method| method.name).collect(),
        _ => builder_methods().iter().map(|method| method.name).collect(),
      };

      if *receiver_ty == Type::String {
        names.extend(["find", "chars", "bytes", "parse_i32"]);
      }

      if let Some(similar) = sagitario_diagnostics::closest_match(name, names.iter().copied()) {
        diagnostic = diagnostic.with_help(format!("a method with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);
      self.unchecked_args(args);

      return Type::Error;
    };

    self.args(expr.span, "method", &found.params, args);
    self.typed.builtin_methods.insert(expr.id, BuiltinMethod::Runtime { function: found.function, located: found.located });

    found.ret
  }

  /// The element type of `for x in text.chars()` or `text.bytes()`, or `None` if `iter` is not
  /// such a call.
  pub(crate) fn text_iterable(&mut self, iter: &Expr) -> Option<Type> {
    let sagitario_ast::ExprKind::MethodCall { receiver, method, args } = &iter.kind else { return None };
    let (builtin, elem) = match method.name.as_str() {
      "chars" => (BuiltinMethod::Chars, Type::Char),
      "bytes" => (BuiltinMethod::Bytes, Type::Int(IntTy::U8)),
      _ => return None,
    };
    let receiver_ty = self.expr(receiver, None);

    if !matches!(receiver_ty, Type::String | Type::Error | Type::Never) {
      self.error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{}` on type `{receiver_ty}`", method.name));
      self.unchecked_args(args);

      return Some(Type::Error);
    }

    self.args(iter.span, "method", &[], args);
    self.typed.builtin_methods.insert(iter.id, builtin);
    self.typed.expr_types.insert(iter.id, Type::Unit);

    Some(elem)
  }

  /// `new Type(args)`: in F1 only `new StringBuilder()`; classes arrive in F2.
  pub(crate) fn new_expr(&mut self, path: &Path, args: &[Expr]) -> Type {
    let name = path.segments.last().map_or("", |segment| segment.name.as_str());
    let resolved = self.resolution.type_paths.get(&path.id).copied();
    let user_type = matches!(resolved, Some(TypeRes::Item(_)));

    if path.segments.len() == 1 && name == "StringBuilder" && !user_type {
      self.args(path.span, "constructor", &[], args);

      return Type::StringBuilder;
    }

    self.unchecked_args(args);

    if resolved == Some(TypeRes::Error) {
      return Type::Error;
    }

    if user_type {
      self.diagnostics.push(
        Diagnostic::error(codes::WRONG_CALL_FORM, path.span, format!("`new` creates class instances, and `{name}` is not a class"))
          .with_help(format!("build the value with a literal or a static function, such as `{name}::new(...)`")),
      );
    } else {
      self.error(codes::NOT_IMPLEMENTED, path.span, "`new` with classes is not implemented yet (F2); in F1 only `new StringBuilder()` exists");
    }

    Type::Error
  }
}
