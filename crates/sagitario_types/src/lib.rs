//! Type checking with local inference (docs/spec/02-types.md, 06-control-flow.md). Records the
//! type of every expression and local binding for lowering; `flow` then checks definite
//! assignment and reachability.

mod flow;

use std::collections::HashMap;
use std::fmt;

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Module, NodeId, Stmt, TemplatePart, TypeExpr, TypeExprKind, UnaryOp, Visibility};
use sagitario_diagnostics::{Code, Diagnostic, Diagnostics, codes};
use sagitario_resolve::{Builtin, FnId, LocalId, Res, Resolution};
use sagitario_span::Span;

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

  pub fn is_signed(self) -> bool {
    matches!(self, IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::Isize)
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
  /// The type of expressions that never produce a value (`return`, `break`, ...). Compatible
  /// with every type; programs cannot write it.
  Never,
  /// The type of an expression that already produced an error; compatible with everything so
  /// one mistake is reported once.
  Error,
}

impl Type {
  fn is_numeric(&self) -> bool {
    matches!(self, Type::Int(_) | Type::Float(_))
  }

  /// Whether a value of type `self` can be used where `expected` is required.
  fn fits(&self, expected: &Type) -> bool {
    self == expected || matches!(self, Type::Never | Type::Error) || *expected == Type::Error
  }
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
      Type::Never => f.write_str("never"),
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
  /// Type of each local binding, indexed by `FnId` then `LocalId`.
  pub local_types: Vec<Vec<Type>>,
  pub expr_types: HashMap<NodeId, Type>,
}

impl TypedModule {
  pub fn sig(&self, id: FnId) -> Option<&FnSig> {
    self.sigs.get(id.0 as usize)
  }

  pub fn local_type(&self, function: FnId, local: LocalId) -> Option<&Type> {
    self.local_types.get(function.0 as usize)?.get(local.0 as usize)
  }

  pub fn expr_type(&self, expr: &Expr) -> Option<&Type> {
    self.expr_types.get(&expr.id)
  }
}

pub fn check(module: &Module, resolution: &Resolution, diagnostics: &mut Diagnostics) -> TypedModule {
  let mut checker = Checker {
    resolution,
    diagnostics,
    typed: TypedModule::default(),
    locals: Vec::new(),
    ret: Type::Unit,
    ret_span: None,
    function: FnId(0),
    loops: Vec::new(),
  };
  let decls = resolution.fn_decls(module);

  for (_, decl) in &decls {
    let sig = checker.signature(decl);

    checker.typed.sigs.push(sig);
  }

  if let Some((_, main)) = decls.iter().find(|(id, _)| Some(*id) == resolution.main) {
    checker.check_main(main);
  }

  for (id, decl) in &decls {
    checker.function(*id, decl);
  }

  let typed = checker.typed;

  for (id, decl) in &decls {
    flow::check_function(decl, resolution.locals(*id), resolution, diagnostics);
  }

  typed
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopKind {
  Loop,
  While,
  For,
}

struct LoopCx {
  kind: LoopKind,
  /// The expected type of the `loop` expression, used to type `break` values.
  expected: Option<Type>,
  break_ty: Option<Type>,
  has_break: bool,
}

struct Checker<'a> {
  resolution: &'a Resolution,
  diagnostics: &'a mut Diagnostics,
  typed: TypedModule,
  /// Types of the current function's locals; `None` until a declaration without type or value
  /// is first assigned.
  locals: Vec<Option<Type>>,
  ret: Type,
  /// The current function's return type annotation, for "expected because of" labels.
  ret_span: Option<Span>,
  function: FnId,
  loops: Vec<LoopCx>,
}

impl Checker<'_> {
  fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
    self.diagnostics.push(Diagnostic::error(code, span, message));
  }

  fn signature(&mut self, decl: &FnDecl) -> FnSig {
    let params = decl.params.iter().map(|param| self.type_expr(&param.ty)).collect();
    let ret = decl.return_type.as_ref().map_or(Type::Unit, |ty| self.type_expr(ty));

    FnSig { params, ret }
  }

  fn check_main(&mut self, main: &FnDecl) {
    let returns_unit = main.return_type.as_ref().is_none_or(|ty| ty.kind == TypeExprKind::Unit);

    if main.visibility != Visibility::Public || !main.params.is_empty() || !returns_unit {
      self.error(codes::INVALID_MAIN, main.name.span, "`main` must be declared as `pub fn main()`");
    }
  }

  fn function(&mut self, id: FnId, decl: &FnDecl) {
    let Some(sig) = self.typed.sig(id).cloned() else { return };
    let local_count = self.resolution.locals(id).len();

    self.function = id;
    self.ret = sig.ret.clone();
    self.ret_span = decl.return_type.as_ref().map(|ty| ty.span);
    self.locals = vec![None; local_count];

    for (index, param) in sig.params.iter().enumerate() {
      if let Some(slot) = self.locals.get_mut(index) {
        *slot = Some(param.clone());
      }
    }

    let body_ty = self.block(&decl.body, Some(&sig.ret));

    if !body_ty.fits(&sig.ret) {
      let span = decl.body.tail.as_ref().map_or(decl.body.span, |tail| tail.span);

      let mut diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, span, format!("expected `{}`, found `{body_ty}`", sig.ret))
        .with_note(format!("the function `{}` must return a value of type `{}` on every path", decl.name.name, sig.ret));

      if let Some(ret_span) = self.ret_span {
        diagnostic = diagnostic.with_label(ret_span, "expected because of the return type");
      }

      self.diagnostics.push(diagnostic);
    }

    let locals = std::mem::take(&mut self.locals);
    let infos = self.resolution.locals(id);
    let mut types = Vec::with_capacity(locals.len());

    for (index, ty) in locals.into_iter().enumerate() {
      let ty = match ty {
        Some(ty) => ty,
        None => {
          if let Some(info) = infos.get(index) {
            self.error(codes::UNKNOWN_TYPE, info.span, format!("cannot infer the type of `{}`; add a type annotation", info.name));
          }

          Type::Error
        }
      };

      types.push(ty);
    }

    self.typed.local_types.push(types);
  }

  fn type_expr(&mut self, ty: &TypeExpr) -> Type {
    match &ty.kind {
      TypeExprKind::Unit => Type::Unit,
      TypeExprKind::Path(path) => {
        let [segment] = path.segments.as_slice() else {
          self.error(codes::NOT_IMPLEMENTED, path.span, "paths with `::` are not implemented yet");

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
            let known = IntTy::ALL.iter().map(|ty| ty.name()).chain(["f32", "f64", "bool", "char", "String"]);
            let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("cannot find type `{name}`"));

            if let Some(similar) = sagitario_diagnostics::closest_match(name, known) {
              diagnostic = diagnostic.with_help(format!("a type with a similar name exists: `{similar}`"));
            }

            self.diagnostics.push(diagnostic);

            Type::Error
          }
        }
      }
    }
  }

  fn local(&self, expr: &Expr) -> Option<LocalId> {
    match self.resolution.paths.get(&expr.id) {
      Some(Res::Local(id)) => Some(*id),
      _ => None,
    }
  }

  fn set_local(&mut self, id: LocalId, ty: Type) {
    if let Some(slot) = self.locals.get_mut(id.0 as usize) {
      *slot = Some(ty);
    }
  }

  /// Checks a block; its type is the tail's type, `never` if it diverges, or `()`.
  fn block(&mut self, block: &Block, expected: Option<&Type>) -> Type {
    let mut diverges = false;

    for stmt in &block.stmts {
      if self.stmt(stmt) == Type::Never {
        diverges = true;
      }
    }

    match &block.tail {
      Some(tail) => {
        let ty = self.expr(tail, expected);

        if diverges { Type::Never } else { ty }
      }
      None if diverges => Type::Never,
      None => Type::Unit,
    }
  }

  /// Checks a statement and returns `never` if it diverges.
  fn stmt(&mut self, stmt: &Stmt) -> Type {
    match stmt {
      Stmt::Let(stmt) => {
        let declared = stmt.ty.as_ref().map(|ty| self.type_expr(ty));
        let Some(id) = self.resolution.bindings.get(&stmt.id).copied() else { return Type::Error };
        let init_ty = stmt.init.as_ref().map(|init| match &declared {
          Some(declared) => {
            let because = stmt.ty.as_ref().map(|ty| (ty.span, "expected because of this type"));

            self.expect_because(init, declared, because);

            declared.clone()
          }
          None => self.expr(init, None),
        });
        let local_ty = match (&declared, &init_ty) {
          (Some(declared), _) => Some(declared.clone()),
          (None, Some(Type::Never)) => Some(Type::Error),
          (None, Some(ty)) => Some(ty.clone()),
          (None, None) => None,
        };

        if let Some(ty) = local_ty {
          self.set_local(id, ty);
        }

        if init_ty == Some(Type::Never) { Type::Never } else { Type::Unit }
      }
      Stmt::Assign { target, op, value, .. } => self.assign(target, *op, value),
      Stmt::Expr(expr) => self.expr(expr, None),
    }
  }

  fn assign(&mut self, target: &Expr, op: Option<BinaryOp>, value: &Expr) -> Type {
    let target_local = match &target.kind {
      ExprKind::Path(_) => self.local(target),
      _ => None,
    };
    let Some(id) = target_local else {
      if matches!(target.kind, ExprKind::Path(_)) && !self.resolution.paths.contains_key(&target.id) {
        // Unresolved name: already reported.
      } else {
        self.error(codes::INVALID_ASSIGNMENT_TARGET, target.span, "only bindings declared with `var` can be assigned");
      }

      return self.expr(value, None);
    };
    let current = self.locals.get(id.0 as usize).cloned().flatten();

    self.typed.expr_types.insert(target.id, current.clone().unwrap_or(Type::Error));

    let value_ty = match (&current, op) {
      (Some(ty), None) => {
        self.expect(value, ty);

        ty.clone()
      }
      (Some(ty), Some(op)) => {
        if *ty == Type::String {
          self.error(codes::NOT_IMPLEMENTED, target.span, format!("`{}=` on `String` is not implemented yet", op.symbol()));
        } else if !ty.is_numeric() && *ty != Type::Error {
          self.error(codes::INVALID_OPERAND, target.span, format!("cannot apply `{}=` to a value of type `{ty}`", op.symbol()));
        }

        self.expect(value, ty);

        ty.clone()
      }
      (None, None) => {
        let ty = self.expr(value, None);

        if ty != Type::Never {
          self.set_local(id, ty.clone());
        }

        ty
      }
      (None, Some(_)) => {
        self.expr(value, None);

        Type::Error
      }
    };

    if value_ty == Type::Never { Type::Never } else { Type::Unit }
  }

  fn expect(&mut self, expr: &Expr, expected: &Type) {
    self.expect_because(expr, expected, None);
  }

  /// Like `expect`, pointing at what imposes the expected type when it is a mismatch.
  fn expect_because(&mut self, expr: &Expr, expected: &Type, because: Option<(Span, &str)>) {
    let actual = self.expr(expr, Some(expected));

    if actual.fits(expected) {
      return;
    }

    let mut diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, expr.span, format!("expected `{expected}`, found `{actual}`"));

    if let Some((span, message)) = because {
      diagnostic = diagnostic.with_label(span, message);
    }

    self.diagnostics.push(diagnostic);
  }

  fn mismatch(&mut self, span: Span, expected: &Type, actual: &Type) {
    self.error(codes::MISMATCHED_TYPES, span, format!("expected `{expected}`, found `{actual}`"));
  }

  /// Checks an expression, using `expected` to give unsuffixed literals their type.
  fn expr(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
    let ty = self.expr_kind(expr, expected);

    self.typed.expr_types.insert(expr.id, ty.clone());

    ty
  }

  fn expr_kind(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
    match &expr.kind {
      ExprKind::Int { value, suffix } => self.int_literal(expr.span, *value, false, suffix.as_deref(), expected),
      ExprKind::Float { suffix, .. } => float_literal_type(suffix.as_deref(), expected),
      ExprKind::Bool(_) => Type::Bool,
      ExprKind::Char(_) => Type::Char,
      ExprKind::Str(_) => Type::String,
      ExprKind::Unit => Type::Unit,
      ExprKind::Template(parts) => {
        for part in parts {
          if let TemplatePart::Expr(part) = part {
            let ty = self.expr(part, None);

            if !matches!(ty, Type::Int(_) | Type::Float(_) | Type::Bool | Type::Char | Type::String | Type::Never | Type::Error) {
              self.diagnostics.push(
                Diagnostic::error(codes::NOT_INTERPOLABLE, part.span, format!("a value of type `{ty}` cannot be interpolated"))
                  .with_note("integers, floats, `bool`, `char` and `String` can be interpolated"),
              );
            }
          }
        }

        Type::String
      }
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        // An unassigned binding without a known type is reported by the flow analysis.
        Some(Res::Local(id)) => self.locals.get(id.0 as usize).cloned().flatten().unwrap_or(Type::Error),
        Some(Res::Fn(_) | Res::Builtin(_)) => {
          self.error(codes::NOT_IMPLEMENTED, expr.span, "functions used as values are not implemented yet");

          Type::Error
        }
        None => Type::Error,
      },
      ExprKind::Call { callee, args } => self.call(expr, callee, args),
      ExprKind::Unary { op, operand } => self.unary(expr, *op, operand, expected),
      ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, expected),
      ExprKind::Cast { expr: inner, ty } => self.cast(inner, ty),
      ExprKind::Range { .. } => {
        self.error(codes::NOT_IMPLEMENTED, expr.span, "ranges outside of `for` loops are not implemented yet");

        Type::Error
      }
      ExprKind::Block(block) => self.block(block, expected),
      ExprKind::If { cond, then_block, else_branch } => self.if_expr(cond, then_block, else_branch.as_deref(), expected),
      ExprKind::While { cond, body } => {
        self.expect(cond, &Type::Bool);
        self.loop_body(LoopKind::While, None, body);

        Type::Unit
      }
      ExprKind::Loop { body } => {
        let cx = self.loop_body(LoopKind::Loop, expected.cloned(), body);

        if cx.has_break { cx.break_ty.unwrap_or(Type::Unit) } else { Type::Never }
      }
      ExprKind::For { iter, body, .. } => {
        let var_ty = self.for_iter(iter);

        if let Some(id) = self.resolution.bindings.get(&expr.id).copied() {
          self.set_local(id, var_ty);
        }

        self.loop_body(LoopKind::For, None, body);

        Type::Unit
      }
      ExprKind::Break { value } => {
        self.break_expr(expr.span, value.as_deref());

        Type::Never
      }
      ExprKind::Continue => {
        if self.loops.is_empty() {
          self.error(codes::BREAK_OUTSIDE_LOOP, expr.span, "`continue` outside of a loop");
        }

        Type::Never
      }
      ExprKind::Return { value } => {
        let ret = self.ret.clone();

        match value {
          Some(value) => {
            let because = self.ret_span.map(|span| (span, "expected because of the return type"));

            self.expect_because(value, &ret, because);
          }
          None if !Type::Unit.fits(&ret) => {
            self.error(codes::MISMATCHED_TYPES, expr.span, format!("expected `{ret}`, found `()`: `return` needs a value"));
          }
          None => {}
        }

        Type::Never
      }
    }
  }

  fn int_literal(&mut self, span: Span, value: u128, negative: bool, suffix: Option<&str>, expected: Option<&Type>) -> Type {
    let int = match (suffix.and_then(IntTy::from_name), expected) {
      (Some(int), _) => int,
      (None, Some(Type::Int(int))) => *int,
      (None, _) => IntTy::I32,
    };

    if negative && !int.is_signed() {
      self.error(codes::INVALID_OPERAND, span, format!("cannot negate a value of the unsigned type `{}`", int.name()));

      return Type::Int(int);
    }

    // The most negative value has one more unit of magnitude than the maximum.
    let limit = if negative { int.max() + 1 } else { int.max() };

    if value > limit {
      let shown = if negative { format!("-{value}") } else { value.to_string() };

      self.diagnostics.push(
        Diagnostic::error(codes::INTEGER_OUT_OF_RANGE, span, format!("the literal `{shown}` does not fit in `{}`", int.name())).with_note(format!(
          "the maximum value of `{}` is `{}`",
          int.name(),
          int.max()
        )),
      );
    }

    Type::Int(int)
  }

  fn unary(&mut self, expr: &Expr, op: UnaryOp, operand: &Expr, expected: Option<&Type>) -> Type {
    if op == UnaryOp::Neg {
      // `-128i8` is a single literal value, so its range is checked including the sign.
      if let ExprKind::Int { value, suffix } = &operand.kind {
        let ty = self.int_literal(expr.span, *value, true, suffix.as_deref(), expected);

        self.typed.expr_types.insert(operand.id, ty.clone());

        return ty;
      }
    }

    match op {
      UnaryOp::Not => {
        self.expect(operand, &Type::Bool);

        Type::Bool
      }
      UnaryOp::Neg => {
        let ty = self.expr(operand, expected.filter(|ty| ty.is_numeric()));

        match &ty {
          Type::Int(int) if !int.is_signed() => {
            self.error(codes::INVALID_OPERAND, expr.span, format!("cannot negate a value of the unsigned type `{ty}`"));

            Type::Error
          }
          Type::Int(_) | Type::Float(_) | Type::Error | Type::Never => ty,
          _ => {
            self.error(codes::INVALID_OPERAND, expr.span, format!("cannot negate a value of type `{ty}`"));

            Type::Error
          }
        }
      }
    }
  }

  /// Checks two operands that must share a type. An unsuffixed literal on the left takes its
  /// type from the right operand, so both `x + 1` and `1 + x` work for any integer `x`.
  fn same_type_operands(&mut self, lhs: &Expr, rhs: &Expr, hint: Option<&Type>) -> Type {
    let (lhs_ty, rhs_ty, mismatch_span) = if is_untyped_literal(lhs) && !is_untyped_literal(rhs) {
      let rhs_ty = self.expr(rhs, hint);
      let lhs_ty = self.expr(lhs, Some(&rhs_ty));

      (lhs_ty, rhs_ty, lhs.span)
    } else {
      let lhs_ty = self.expr(lhs, hint);
      let rhs_ty = self.expr(rhs, Some(&lhs_ty));

      (lhs_ty, rhs_ty, rhs.span)
    };

    match (&lhs_ty, &rhs_ty) {
      (Type::Error, _) | (_, Type::Error) => Type::Error,
      (Type::Never, other) | (other, Type::Never) => other.clone(),
      _ if lhs_ty == rhs_ty => lhs_ty,
      _ => {
        let (expected, found) = if mismatch_span == rhs.span { (&lhs_ty, &rhs_ty) } else { (&rhs_ty, &lhs_ty) };

        self.mismatch(mismatch_span, expected, found);

        Type::Error
      }
    }
  }

  fn binary(&mut self, op: BinaryOp, lhs: &Expr, rhs: &Expr, expected: Option<&Type>) -> Type {
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
      self.expect(lhs, &Type::Bool);
      self.expect(rhs, &Type::Bool);

      return Type::Bool;
    }

    let hint = if op.is_arithmetic() { expected.filter(|ty| ty.is_numeric()) } else { None };
    let ty = self.same_type_operands(lhs, rhs, hint);
    let span = lhs.span.to(rhs.span);

    if op.is_arithmetic() {
      return match ty {
        Type::Int(_) | Type::Float(_) | Type::Error | Type::Never => ty,
        Type::String if op == BinaryOp::Add => {
          self.error(codes::NOT_IMPLEMENTED, span, "concatenating `String`s with `+` is not implemented yet");

          Type::Error
        }
        other => {
          self.error(codes::INVALID_OPERAND, span, format!("cannot apply `{}` to values of type `{other}`", op.symbol()));

          Type::Error
        }
      };
    }

    let supported = match &ty {
      Type::Int(_) | Type::Float(_) | Type::Char | Type::Error | Type::Never => true,
      Type::Bool => matches!(op, BinaryOp::Eq | BinaryOp::Ne),
      Type::String => {
        self.error(codes::NOT_IMPLEMENTED, span, "comparing `String`s is not implemented yet");

        true
      }
      _ => false,
    };

    if !supported {
      self.error(codes::INVALID_OPERAND, span, format!("cannot compare values of type `{ty}` with `{}`", op.symbol()));
    }

    Type::Bool
  }

  fn cast(&mut self, inner: &Expr, ty: &TypeExpr) -> Type {
    let from = self.expr(inner, None);
    let to = self.type_expr(ty);
    let valid = match (&from, &to) {
      (Type::Error | Type::Never, _) | (_, Type::Error) => true,
      (from, to) if from == to => true,
      (Type::Int(_) | Type::Float(_), Type::Int(_) | Type::Float(_)) => true,
      (Type::Bool | Type::Char, Type::Int(_)) => true,
      (Type::Int(IntTy::U8), Type::Char) => true,
      _ => false,
    };

    if !valid {
      let span = inner.span.to(ty.span);

      self.error(codes::INVALID_CAST, span, format!("cannot convert a value of type `{from}` to `{to}` with `as`"));
    }

    to
  }

  fn if_expr(&mut self, cond: &Expr, then_block: &Block, else_branch: Option<&Expr>, expected: Option<&Type>) -> Type {
    self.expect(cond, &Type::Bool);

    let then_ty = self.block(then_block, expected);
    let Some(else_branch) = else_branch else {
      if !then_ty.fits(&Type::Unit) {
        let span = then_block.tail.as_ref().map_or(then_block.span, |tail| tail.span);

        self.diagnostics.push(
          Diagnostic::error(codes::MISMATCHED_TYPES, span, format!("expected `()`, found `{then_ty}`"))
            .with_note("an `if` without `else` has type `()`"),
        );
      }

      return Type::Unit;
    };
    let else_expected = if then_ty == Type::Never { expected.cloned() } else { Some(then_ty.clone()) };
    let else_ty = self.expr(else_branch, else_expected.as_ref());

    match (&then_ty, &else_ty) {
      (Type::Never, other) | (other, Type::Never) => other.clone(),
      (Type::Error, _) | (_, Type::Error) => Type::Error,
      _ if then_ty == else_ty => then_ty,
      _ => {
        let mut diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, else_branch.span, format!("expected `{then_ty}`, found `{else_ty}`"))
          .with_note("both branches of an `if` must have the same type");

        if let Some(tail) = &then_block.tail {
          diagnostic = diagnostic.with_label(tail.span, format!("this branch has type `{then_ty}`"));
        }

        self.diagnostics.push(diagnostic);

        Type::Error
      }
    }
  }

  fn loop_body(&mut self, kind: LoopKind, expected: Option<Type>, body: &Block) -> LoopCx {
    self.loops.push(LoopCx { kind, expected, break_ty: None, has_break: false });

    let body_ty = self.block(body, Some(&Type::Unit));

    if !body_ty.fits(&Type::Unit) {
      let span = body.tail.as_ref().map_or(body.span, |tail| tail.span);

      self.mismatch(span, &Type::Unit, &body_ty);
    }

    self.loops.pop().unwrap_or(LoopCx { kind, expected: None, break_ty: None, has_break: false })
  }

  fn for_iter(&mut self, iter: &Expr) -> Type {
    let ExprKind::Range { start, end, .. } = &iter.kind else {
      self.error(codes::NOT_IMPLEMENTED, iter.span, "`for` over values other than ranges is not implemented yet");
      self.expr(iter, None);

      return Type::Error;
    };
    let ty = self.same_type_operands(start, end, None);
    let ty = match ty {
      Type::Int(_) | Type::Error => ty,
      other => {
        self.error(codes::INVALID_OPERAND, iter.span, format!("range bounds must be integers, found `{other}`"));

        Type::Error
      }
    };

    self.typed.expr_types.insert(iter.id, ty.clone());

    ty
  }

  fn break_expr(&mut self, span: Span, value: Option<&Expr>) {
    let Some(kind) = self.loops.last().map(|cx| cx.kind) else {
      self.error(codes::BREAK_OUTSIDE_LOOP, span, "`break` outside of a loop");

      if let Some(value) = value {
        self.expr(value, None);
      }

      return;
    };

    if kind != LoopKind::Loop {
      if let Some(value) = value {
        self.error(codes::BREAK_WITH_VALUE, value.span, "`break` with a value is only allowed inside `loop`");
        self.expr(value, None);
      }

      return;
    }

    let (known, expected) = self.loops.last().map(|cx| (cx.break_ty.clone(), cx.expected.clone())).unwrap_or((None, None));
    let ty = match (value, &known) {
      (Some(value), Some(known)) => {
        self.expect(value, known);

        known.clone()
      }
      (Some(value), None) => self.expr(value, expected.as_ref()),
      (None, Some(known)) => {
        if !Type::Unit.fits(known) {
          self.mismatch(span, known, &Type::Unit);
        }

        known.clone()
      }
      (None, None) => Type::Unit,
    };

    if let Some(cx) = self.loops.last_mut() {
      cx.has_break = true;

      if cx.break_ty.is_none() && ty != Type::Never {
        cx.break_ty = Some(ty);
      }
    }
  }

  fn call(&mut self, expr: &Expr, callee: &Expr, args: &[Expr]) -> Type {
    let sig = match (&callee.kind, self.resolution.paths.get(&callee.id).copied()) {
      (ExprKind::Path(_), Some(Res::Fn(id))) => self.typed.sig(id).cloned(),
      (ExprKind::Path(_), Some(Res::Builtin(builtin))) => Some(builtin_sig(builtin)),
      (ExprKind::Path(_), None) => None,
      _ => {
        self.error(codes::NOT_IMPLEMENTED, callee.span, "calling values other than functions is not implemented yet");

        None
      }
    };
    let Some(sig) = sig else {
      for arg in args {
        self.expr(arg, None);
      }

      return Type::Error;
    };

    if args.len() != sig.params.len() {
      self.error(
        codes::WRONG_ARGUMENT_COUNT,
        expr.span,
        format!("this function takes {} argument{} but {} were supplied", sig.params.len(), plural(sig.params.len()), args.len()),
      );
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

fn float_literal_type(suffix: Option<&str>, expected: Option<&Type>) -> Type {
  match (suffix.and_then(FloatTy::from_name), expected) {
    (Some(ty), _) => Type::Float(ty),
    (None, Some(Type::Float(ty))) => Type::Float(*ty),
    (None, _) => Type::Float(FloatTy::F64),
  }
}

/// An unsuffixed numeric literal, possibly negated, whose type comes from context.
fn is_untyped_literal(expr: &Expr) -> bool {
  match &expr.kind {
    ExprKind::Int { suffix: None, .. } | ExprKind::Float { suffix: None, .. } => true,
    ExprKind::Unary { op: UnaryOp::Neg, operand } => is_untyped_literal(operand),
    _ => false,
  }
}

fn plural(count: usize) -> &'static str {
  if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_diagnostics::Severity;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn diagnostics(text: &str) -> Vec<&'static str> {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.to_string());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
    let module = sagitario_parser::parse_module(file, tokens, &mut diagnostics);
    let resolution = sagitario_resolve::resolve(&module, &mut diagnostics);

    assert!(!diagnostics.has_errors(), "front end failed: {:?}", diagnostics.take());

    check(&module, &resolution, &mut diagnostics);

    // Warnings are covered by the `flow` tests.
    diagnostics.take().iter().filter(|diagnostic| diagnostic.severity == Severity::Error).map(|diagnostic| diagnostic.code.0).collect()
  }

  /// Wraps statements in `pub fn main()`.
  fn in_main(body: &str) -> Vec<&'static str> {
    diagnostics(&format!("pub fn main() {{ {body} }}"))
  }

  #[test]
  fn accepts_well_typed_programs() {
    let program = "
      fn add(a: i64, b: i64) -> i64 { a + b }
      fn sign(x: i32) -> i32 { if x < 0 { -1 } else if x == 0 { 0 } else { 1 } }
      fn first_square_above(limit: u32) -> u32 {
        var n: u32 = 0;
        loop {
          n += 1;
          if n * n > limit { break n; }
        }
      }
      pub fn main() {
        const total = add(1, 2) + 3;
        const ratio: f32 = 1.5 * 2.0;
        const byte = 300 as u8;
        const message = `${total} ${ratio} ${byte} ${sign(-5)} ${first_square_above(50)} ${'x'} ${true}`;
        println(message);
        for i in 0..=10 { if i % 2 == 0 { continue; } }
        var count: u8 = 0;
        while count < 10u8 { count += 1; }
      }";

    assert_eq!(diagnostics(program), Vec::<&str>::new());
  }

  #[test]
  fn infers_literals_from_the_other_operand() {
    assert!(in_main("const x: u8 = 1; const y = 1 + x; const z: u8 = y;").is_empty());
    assert!(in_main("const x = -128i8; const y: i8 = -128;").is_empty());
  }

  #[test]
  fn rejects_mismatched_types() {
    assert_eq!(in_main("const x: i64 = 1i32;"), vec!["E0300"]);
    assert_eq!(in_main("const x = 1 + 2.0;"), vec!["E0300"]);
    assert_eq!(in_main("const x = if true { 1 } else { \"a\" };"), vec!["E0300"]);
    assert_eq!(in_main("if true { 1 }"), vec!["E0300"]);
    assert_eq!(diagnostics("fn f() -> i32 { }\npub fn main() {}"), vec!["E0300"]);
    assert_eq!(diagnostics("fn f() -> i32 { return; }\npub fn main() {}"), vec!["E0300"]);
  }

  #[test]
  fn rejects_invalid_operands() {
    assert_eq!(in_main("const x = -1u32;"), vec!["E0310"]);
    assert_eq!(in_main("const x: u32 = 1; const y = -x;"), vec!["E0310"]);
    assert_eq!(in_main("const x = !1;"), vec!["E0300"]);
    assert_eq!(in_main("const x = true + false;"), vec!["E0310"]);
    assert_eq!(in_main("const x = true < false;"), vec!["E0310"]);
    assert_eq!(in_main("for i in 0.0..1.0 {}"), vec!["E0310"]);
  }

  #[test]
  fn checks_casts() {
    assert!(in_main("const a = 1.9 as i32; const b = true as u8; const c = 'a' as u32; const d = 65u8 as char;").is_empty());
    assert_eq!(in_main("const a = 1 as bool;"), vec!["E0305"]);
    assert_eq!(in_main("const a = 66u16 as char;"), vec!["E0305"]);
  }

  #[test]
  fn checks_loops_and_breaks() {
    assert_eq!(in_main("break;"), vec!["E0303"]);
    assert_eq!(in_main("continue;"), vec!["E0303"]);
    assert_eq!(in_main("while true { break 1; }"), vec!["E0304"]);
    assert_eq!(in_main("const x = loop { if true { break 1; } break \"a\"; };"), vec!["E0300"]);
    assert!(in_main("const x: u8 = loop { break 255; };").is_empty());
  }

  #[test]
  fn checks_interpolation_and_ranges() {
    assert_eq!(in_main("const x = `${()}`;"), vec!["E0306"]);
    assert_eq!(in_main("const r = 0..10;"), vec!["E0900"]);
  }

  #[test]
  fn checks_literal_ranges() {
    assert_eq!(in_main("const x: u8 = 256;"), vec!["E0302"]);
    assert_eq!(in_main("const x: i8 = -129;"), vec!["E0302"]);
  }

  #[test]
  fn infers_deferred_bindings_from_their_first_assignment() {
    assert!(in_main("var x; x = 5u8; const y: u8 = x;").is_empty());
    assert_eq!(in_main("var x;"), vec!["E0311"]);
  }

  #[test]
  fn rejects_invalid_assignment_targets() {
    assert_eq!(in_main("println = 1;"), vec!["E0307"]);
  }

  #[test]
  fn validates_main_signature() {
    assert_eq!(diagnostics("fn main() {}"), vec!["E0204"]);
    assert_eq!(diagnostics("pub fn main(x: i32) {}"), vec!["E0204"]);
  }
}
