//! Type checking with local inference (docs/spec/02-types.md). Records the type of every
//! expression and the signature of every function for the lowering phase.

use std::collections::HashMap;
use std::fmt;

use sagitario_ast::{Block, Expr, ExprKind, FnDecl, Module, NodeId, Stmt, TypeExpr, TypeExprKind, Visibility};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_resolve::{Builtin, FnId, Res, Resolution};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntTy {
  I8,
  I16,
  I32,
  I64,
  Isize,
  U8,
  U16,
  U32,
  U64,
  Usize,
}

impl IntTy {
  const ALL: [IntTy; 10] = [IntTy::I8, IntTy::I16, IntTy::I32, IntTy::I64, IntTy::Isize, IntTy::U8, IntTy::U16, IntTy::U32, IntTy::U64, IntTy::Usize];

  pub fn name(self) -> &'static str {
    match self {
      IntTy::I8 => "i8",
      IntTy::I16 => "i16",
      IntTy::I32 => "i32",
      IntTy::I64 => "i64",
      IntTy::Isize => "isize",
      IntTy::U8 => "u8",
      IntTy::U16 => "u16",
      IntTy::U32 => "u32",
      IntTy::U64 => "u64",
      IntTy::Usize => "usize",
    }
  }

  fn from_name(name: &str) -> Option<IntTy> {
    IntTy::ALL.into_iter().find(|ty| ty.name() == name)
  }

  /// The largest value of the type. `isize`/`usize` assume 64-bit targets, the only ones the MVP
  /// supports.
  pub fn max(self) -> u128 {
    match self {
      IntTy::I8 => i8::MAX as u128,
      IntTy::I16 => i16::MAX as u128,
      IntTy::I32 => i32::MAX as u128,
      IntTy::I64 | IntTy::Isize => i64::MAX as u128,
      IntTy::U8 => u8::MAX as u128,
      IntTy::U16 => u16::MAX as u128,
      IntTy::U32 => u32::MAX as u128,
      IntTy::U64 | IntTy::Usize => u64::MAX as u128,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTy {
  F32,
  F64,
}

impl FloatTy {
  pub fn name(self) -> &'static str {
    match self {
      FloatTy::F32 => "f32",
      FloatTy::F64 => "f64",
    }
  }

  fn from_name(name: &str) -> Option<FloatTy> {
    [FloatTy::F32, FloatTy::F64].into_iter().find(|ty| ty.name() == name)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
  Unit,
  Bool,
  Char,
  Int(IntTy),
  Float(FloatTy),
  String,
  /// The type of an expression that already produced an error; compatible with everything so
  /// one mistake is reported once.
  Error,
}

impl fmt::Display for Type {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Type::Unit => f.write_str("()"),
      Type::Bool => f.write_str("bool"),
      Type::Char => f.write_str("char"),
      Type::Int(ty) => f.write_str(ty.name()),
      Type::Float(ty) => f.write_str(ty.name()),
      Type::String => f.write_str("String"),
      Type::Error => f.write_str("{error}"),
    }
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
  pub params: Vec<Type>,
  pub ret: Type,
}

pub fn builtin_sig(builtin: Builtin) -> FnSig {
  match builtin {
    Builtin::Print | Builtin::Println | Builtin::Eprintln => FnSig { params: vec![Type::String], ret: Type::Unit },
  }
}

#[derive(Debug, Default)]
pub struct TypedModule {
  /// Signature of each function, indexed by `FnId`.
  pub sigs: Vec<FnSig>,
  pub expr_types: HashMap<NodeId, Type>,
}

impl TypedModule {
  pub fn sig(&self, id: FnId) -> Option<&FnSig> {
    self.sigs.get(id.0 as usize)
  }
}

pub fn check(module: &Module, resolution: &Resolution, diagnostics: &mut Diagnostics) -> TypedModule {
  let mut checker = Checker { resolution, diagnostics, typed: TypedModule::default(), params: Vec::new() };
  let decls: Vec<(FnId, &FnDecl)> = resolution.fn_decls(module).collect();

  for (_, decl) in &decls {
    let sig = checker.signature(decl);

    checker.typed.sigs.push(sig);
  }

  if let Some((_, main)) = decls.iter().find(|(id, _)| Some(*id) == resolution.main) {
    checker.check_main(main);
  }

  for (id, decl) in &decls {
    let Some(sig) = checker.typed.sig(*id).cloned() else { continue };

    checker.params = sig.params;
    checker.block(&decl.body, &sig.ret);
  }

  checker.typed
}

struct Checker<'a> {
  resolution: &'a Resolution,
  diagnostics: &'a mut Diagnostics,
  typed: TypedModule,
  /// Parameter types of the function being checked.
  params: Vec<Type>,
}

impl Checker<'_> {
  fn signature(&mut self, decl: &FnDecl) -> FnSig {
    let params = decl.params.iter().map(|param| self.type_expr(&param.ty)).collect();
    let ret = match &decl.return_type {
      None => Type::Unit,
      Some(ty) => {
        let ret = self.type_expr(ty);

        if ret != Type::Unit && ret != Type::Error {
          self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, ty.span, "returning values from functions is not implemented yet"));
        }

        ret
      }
    };

    FnSig { params, ret }
  }

  fn check_main(&mut self, main: &FnDecl) {
    let returns_unit = main.return_type.as_ref().is_none_or(|ty| ty.kind == TypeExprKind::Unit);

    if main.visibility != Visibility::Public || !main.params.is_empty() || !returns_unit {
      self.diagnostics.push(Diagnostic::error(codes::INVALID_MAIN, main.name.span, "`main` must be declared as `pub fn main()`"));
    }
  }

  fn type_expr(&mut self, ty: &TypeExpr) -> Type {
    match &ty.kind {
      TypeExprKind::Unit => Type::Unit,
      TypeExprKind::Path(path) => {
        let [segment] = path.segments.as_slice() else {
          self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, path.span, "paths with `::` are not implemented yet"));

          return Type::Error;
        };
        let name = segment.name.as_str();

        if let Some(int) = IntTy::from_name(name) {
          return Type::Int(int);
        }

        if let Some(float) = FloatTy::from_name(name) {
          return Type::Float(float);
        }

        match name {
          "bool" => Type::Bool,
          "char" => Type::Char,
          "String" => Type::String,
          _ => {
            self.diagnostics.push(Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("cannot find type `{name}`")));

            Type::Error
          }
        }
      }
    }
  }

  fn block(&mut self, block: &Block, expected: &Type) {
    for Stmt::Expr(expr) in &block.stmts {
      self.expr(expr, None);
    }

    match &block.tail {
      Some(tail) => self.expect(tail, expected),
      None if *expected != Type::Unit && *expected != Type::Error => {
        self.diagnostics.push(Diagnostic::error(
          codes::MISMATCHED_TYPES,
          block.span,
          format!("expected `{expected}`, found `()`: the block has no final expression"),
        ));
      }
      None => {}
    }
  }

  fn expect(&mut self, expr: &Expr, expected: &Type) {
    let actual = self.expr(expr, Some(expected));

    if actual != *expected && actual != Type::Error && *expected != Type::Error {
      self.diagnostics.push(Diagnostic::error(codes::MISMATCHED_TYPES, expr.span, format!("expected `{expected}`, found `{actual}`")));
    }
  }

  /// Checks an expression, using `expected` to give unsuffixed literals their type.
  fn expr(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
    let ty = match &expr.kind {
      ExprKind::Int { value, suffix } => self.int_literal(expr, *value, suffix.as_deref(), expected),
      ExprKind::Float { suffix, .. } => match (suffix.as_deref().and_then(FloatTy::from_name), expected) {
        (Some(ty), _) => Type::Float(ty),
        (None, Some(Type::Float(ty))) => Type::Float(*ty),
        (None, _) => Type::Float(FloatTy::F64),
      },
      ExprKind::Bool(_) => Type::Bool,
      ExprKind::Char(_) => Type::Char,
      ExprKind::Str(_) => Type::String,
      ExprKind::Unit => Type::Unit,
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        Some(Res::Param(index)) => self.params.get(*index as usize).cloned().unwrap_or(Type::Error),
        Some(Res::Fn(_) | Res::Builtin(_)) => {
          self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, expr.span, "functions used as values are not implemented yet"));
          Type::Error
        }
        None => Type::Error,
      },
      ExprKind::Call { callee, args } => self.call(expr, callee, args),
    };

    self.typed.expr_types.insert(expr.id, ty.clone());

    ty
  }

  fn int_literal(&mut self, expr: &Expr, value: u128, suffix: Option<&str>, expected: Option<&Type>) -> Type {
    let int = match (suffix.and_then(IntTy::from_name), expected) {
      (Some(int), _) => int,
      (None, Some(Type::Int(int))) => *int,
      (None, _) => IntTy::I32,
    };

    if value > int.max() {
      self.diagnostics.push(
        Diagnostic::error(codes::INTEGER_OUT_OF_RANGE, expr.span, format!("the literal `{value}` does not fit in `{}`", int.name()))
          .with_note(format!("the maximum value of `{}` is `{}`", int.name(), int.max())),
      );
    }

    Type::Int(int)
  }

  fn call(&mut self, expr: &Expr, callee: &Expr, args: &[Expr]) -> Type {
    let res = match &callee.kind {
      ExprKind::Path(_) => self.resolution.paths.get(&callee.id).copied(),
      _ => {
        self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, callee.span, "calling expressions is not implemented yet"));

        None
      }
    };
    let sig = match res {
      Some(Res::Fn(id)) => self.typed.sig(id).cloned(),
      Some(Res::Builtin(builtin)) => Some(builtin_sig(builtin)),
      Some(Res::Param(_)) => {
        self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, callee.span, "calling parameters is not implemented yet"));

        None
      }
      None => None,
    };
    let Some(sig) = sig else {
      for arg in args {
        self.expr(arg, None);
      }

      return Type::Error;
    };

    if args.len() != sig.params.len() {
      self.diagnostics.push(Diagnostic::error(
        codes::WRONG_ARGUMENT_COUNT,
        expr.span,
        format!("this function takes {} argument{} but {} were supplied", sig.params.len(), plural(sig.params.len()), args.len()),
      ));
    }
    for (index, arg) in args.iter().enumerate() {
      match sig.params.get(index) {
        Some(param) => self.expect(arg, param),
        None => {
          self.expr(arg, None);
        }
      }
    }

    sig.ret
  }
}

fn plural(count: usize) -> &'static str {
  if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn errors(text: &str) -> Vec<&'static str> {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.to_string());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
    let module = sagitario_parser::parse_module(file, tokens, &mut diagnostics);
    let resolution = sagitario_resolve::resolve(&module, &mut diagnostics);

    assert!(!diagnostics.has_errors(), "front end failed: {:?}", diagnostics.take());

    check(&module, &resolution, &mut diagnostics);

    diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect()
  }

  #[test]
  fn accepts_well_typed_programs() {
    assert!(errors("fn show(s: String, n: u8) { println(s); }\npub fn main() { show(\"a\", 255); show(\"b\", 0) }").is_empty());
  }

  #[test]
  fn rejects_mismatched_arguments() {
    assert_eq!(errors("pub fn main() { println(42); }"), vec!["E0300"]);
    assert_eq!(errors("fn f(c: char) {}\npub fn main() { f(\"c\"); }"), vec!["E0300"]);
  }

  #[test]
  fn rejects_wrong_argument_counts() {
    assert_eq!(errors("pub fn main() { println(); }"), vec!["E0301"]);
    assert_eq!(errors("pub fn main() { println(\"a\", \"b\"); }"), vec!["E0301"]);
  }

  #[test]
  fn checks_integer_literal_ranges() {
    assert_eq!(errors("fn f(x: u8) {}\npub fn main() { f(256); }"), vec!["E0302"]);
    assert_eq!(errors("pub fn main() { 2147483648; }"), vec!["E0302"]);
    assert!(errors("pub fn main() { 2147483648i64; }").is_empty());
  }

  #[test]
  fn rejects_unit_functions_used_as_values_and_value_tails() {
    assert_eq!(errors("fn f() {}\npub fn main() { println(f()); }"), vec!["E0300"]);
    assert_eq!(errors("pub fn main() { 1 }"), vec!["E0300"]);
  }

  #[test]
  fn validates_main_signature() {
    assert_eq!(errors("fn main() {}"), vec!["E0204"]);
    assert_eq!(errors("pub fn main(x: i32) {}"), vec!["E0204"]);
  }

  #[test]
  fn reports_unknown_types() {
    assert_eq!(errors("fn f(x: Strin) {}\npub fn main() {}"), vec!["E0200"]);
  }
}
