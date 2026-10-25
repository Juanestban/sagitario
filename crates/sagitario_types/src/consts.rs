//! Module-level constants (docs/spec/03-memory.md#module-level-constants, ADR-0049): the
//! compiler computes each value from literals, other constants, arithmetic, comparisons, casts
//! and `include_str`, and reports anything it cannot compute.

use sagitario_ast::{BinaryOp, Expr, ExprKind, UnaryOp};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{Builtin, ConstId, Res};
use sagitario_span::Span;

use crate::{Checker, FloatTy, IntTy, Type};

/// The value of a module-level constant.
#[derive(Debug, Clone, PartialEq)]
pub enum ConstValue {
  Bool(bool),
  Char(char),
  /// Within the range of the constant's integer type.
  Int(i128),
  Float(f64),
  Str(String),
}

/// Where the computation of each constant stands, to find constants that need themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConstState {
  Pending,
  Computing,
  Done,
}

impl Checker<'_> {
  /// The type and value of every module-level constant.
  pub(crate) fn const_defs(&mut self) {
    let count = self.resolution.consts.len();

    self.const_states = vec![ConstState::Pending; count];
    self.typed.const_types = vec![Type::Error; count];
    self.typed.const_values = vec![None; count];

    for index in 0..count {
      self.const_value(ConstId(index as u32), None);
    }
  }

  /// Computes a constant once; `use_span` is where another constant names it.
  fn const_value(&mut self, id: ConstId, use_span: Option<Span>) -> Option<(Type, ConstValue)> {
    let index = id.0 as usize;
    let decl = self.resolution.const_decl(self.package, id)?;

    match self.const_states.get(index).copied()? {
      ConstState::Done => {
        let ty = self.typed.const_types.get(index)?.clone();

        return self.typed.const_values.get(index)?.clone().map(|value| (ty, value));
      }
      ConstState::Computing => {
        self.diagnostics.push(
          Diagnostic::error(
            codes::NOT_CONSTANT,
            use_span.unwrap_or(decl.name.span),
            format!("the constant `{}` needs its own value", decl.name.name),
          )
          .with_label(decl.name.span, "declared here"),
        );

        return None;
      }
      ConstState::Pending => {}
    }

    if let Some(state) = self.const_states.get_mut(index) {
      *state = ConstState::Computing;
    }

    let module = self.resolution.consts.get(index).map(|info| info.module).unwrap_or_default();
    let owner = self.owner.take();
    let ty = self.type_expr(&decl.ty);

    self.owner = owner;

    let valid = matches!(ty, Type::Int(_) | Type::Float(_) | Type::Bool | Type::Char | Type::String | Type::Error);

    if !valid {
      self.diagnostics.push(
        Diagnostic::error(codes::NOT_CONSTANT, decl.ty.span, format!("a module-level constant cannot have the type `{ty}`"))
          .with_note("constants are integers, floats, `bool`, `char` or `String`"),
      );
    }

    let value = if valid && ty != Type::Error { self.const_expr(&decl.value, &ty, module) } else { None };

    if let Some(slot) = self.typed.const_types.get_mut(index) {
      *slot = ty.clone();
    }

    if let Some(slot) = self.typed.const_values.get_mut(index) {
      *slot = value.clone();
    }

    if let Some(state) = self.const_states.get_mut(index) {
      *state = ConstState::Done;
    }

    value.map(|value| (ty, value))
  }

  /// Computes `expr` as a value of type `expected`, reporting a mismatch.
  fn const_expr(&mut self, expr: &Expr, expected: &Type, module: usize) -> Option<ConstValue> {
    let (ty, value) = self.const_eval(expr, Some(expected), module)?;

    if ty != *expected {
      self.error(codes::MISMATCHED_TYPES, expr.span, format!("expected `{expected}`, found `{ty}`"));

      return None;
    }

    Some(value)
  }

  /// The type and value of `expr`; `hint` types integer and float literals without a suffix.
  fn const_eval(&mut self, expr: &Expr, hint: Option<&Type>, module: usize) -> Option<(Type, ConstValue)> {
    match &expr.kind {
      ExprKind::Bool(value) => Some((Type::Bool, ConstValue::Bool(*value))),
      ExprKind::Char(value) => Some((Type::Char, ConstValue::Char(*value))),
      ExprKind::Str(value) => Some((Type::String, ConstValue::Str(value.clone()))),
      ExprKind::Int { value, suffix } => {
        let ty = match (suffix.as_deref().and_then(IntTy::from_name), hint) {
          (Some(int), _) => int,
          (None, Some(Type::Int(int))) => *int,
          _ => IntTy::I32,
        };
        let value = i128::try_from(*value).ok();

        self.const_int(value, ty, expr.span)
      }
      ExprKind::Float { text, suffix } => {
        let ty = match (suffix.as_deref(), hint) {
          (Some("f32"), _) | (None, Some(Type::Float(FloatTy::F32))) => FloatTy::F32,
          _ => FloatTy::F64,
        };
        let parsed: f64 = text.replace('_', "").parse().ok()?;

        self.const_float(parsed, ty, expr.span)
      }
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id).copied() {
        Some(Res::Const(other)) => self.const_value(other, Some(expr.span)),
        _ => self.not_constant(expr.span),
      },
      // A negative literal: its magnitude may only fit once negated (`-128` in `i8`).
      ExprKind::Unary { op: UnaryOp::Neg, operand } if matches!(operand.kind, ExprKind::Int { .. }) => {
        let ExprKind::Int { value, suffix } = &operand.kind else { return None };
        let ty = match (suffix.as_deref().and_then(IntTy::from_name), hint) {
          (Some(int), _) => int,
          (None, Some(Type::Int(int))) => *int,
          _ => IntTy::I32,
        };

        self.const_int(i128::try_from(*value).ok().and_then(i128::checked_neg), ty, expr.span)
      }
      ExprKind::Unary { op, operand } => {
        let (ty, value) = self.const_eval(operand, hint, module)?;

        match (op, &ty, value) {
          (UnaryOp::Neg, Type::Int(int), ConstValue::Int(value)) => self.const_int(value.checked_neg(), *int, expr.span),
          (UnaryOp::Neg, Type::Float(float), ConstValue::Float(value)) => self.const_float(-value, *float, expr.span),
          (UnaryOp::Not, Type::Bool, ConstValue::Bool(value)) => Some((Type::Bool, ConstValue::Bool(!value))),
          _ => self.const_operand_error(expr.span, &format!("`{}`", unary_symbol(*op)), &ty),
        }
      }
      ExprKind::Binary { op, lhs, rhs } => {
        let operand_hint = if is_comparison(*op) || matches!(op, BinaryOp::And | BinaryOp::Or) { None } else { hint };
        let (lhs_ty, lhs_value) = self.const_eval(lhs, operand_hint, module)?;
        let (rhs_ty, rhs_value) = self.const_eval(rhs, Some(&lhs_ty), module)?;

        if lhs_ty != rhs_ty {
          self.error(codes::MISMATCHED_TYPES, rhs.span, format!("expected `{lhs_ty}`, found `{rhs_ty}`"));

          return None;
        }

        self.const_binary(*op, &lhs_ty, lhs_value, rhs_value, expr.span)
      }
      ExprKind::Cast { expr: inner, ty } => {
        let (from, value) = self.const_eval(inner, None, module)?;
        let owner = self.owner.take();
        let to = self.type_expr(ty);

        self.owner = owner;
        self.const_cast(&from, value, &to, expr.span)
      }
      ExprKind::Call { callee, args } if matches!(self.resolution.paths.get(&callee.id), Some(Res::Builtin(Builtin::IncludeStr))) => {
        self.include_str(expr.span, args, module)
      }
      _ => self.not_constant(expr.span),
    }
  }

  fn not_constant<T>(&mut self, span: Span) -> Option<T> {
    self.diagnostics.push(
      Diagnostic::error(codes::NOT_CONSTANT, span, "the compiler cannot compute this expression")
        .with_note("a constant is made of literals, other constants, arithmetic, comparisons, `as` and `include_str`"),
    );

    None
  }

  fn const_operand_error<T>(&mut self, span: Span, operator: &str, ty: &Type) -> Option<T> {
    self.error(codes::INVALID_OPERAND, span, format!("{operator} cannot be applied to `{ty}`"));

    None
  }

  /// An integer of type `int`, or an error if it does not fit (`None` is an overflow).
  fn const_int(&mut self, value: Option<i128>, int: IntTy, span: Span) -> Option<(Type, ConstValue)> {
    let max = i128::try_from(int.max(self.package.target.pointer_width)).ok()?;
    let min = if int.is_signed() { -max - 1 } else { 0 };

    match value {
      Some(value) if (min..=max).contains(&value) => Some((Type::Int(int), ConstValue::Int(value))),
      _ => {
        self.diagnostics.push(
          Diagnostic::error(codes::NOT_CONSTANT, span, format!("this constant does not fit in `{}`", int.name()))
            .with_note(format!("`{}` holds {min} to {max}", int.name())),
        );

        None
      }
    }
  }

  fn const_float(&mut self, value: f64, float: FloatTy, span: Span) -> Option<(Type, ConstValue)> {
    let value = if float == FloatTy::F32 { f64::from(value as f32) } else { value };

    if !value.is_finite() {
      self.error(codes::NOT_CONSTANT, span, "a float constant must be a finite number");

      return None;
    }

    Some((Type::Float(float), ConstValue::Float(value)))
  }

  fn const_binary(&mut self, op: BinaryOp, ty: &Type, lhs: ConstValue, rhs: ConstValue, span: Span) -> Option<(Type, ConstValue)> {
    if is_comparison(op) {
      let ordering = match (&lhs, &rhs) {
        (ConstValue::Int(a), ConstValue::Int(b)) => a.partial_cmp(b),
        (ConstValue::Float(a), ConstValue::Float(b)) => a.partial_cmp(b),
        (ConstValue::Bool(a), ConstValue::Bool(b)) => a.partial_cmp(b),
        (ConstValue::Char(a), ConstValue::Char(b)) => a.partial_cmp(b),
        (ConstValue::Str(a), ConstValue::Str(b)) => a.as_bytes().partial_cmp(b.as_bytes()),
        _ => None,
      }?;
      let result = match op {
        BinaryOp::Eq => ordering.is_eq(),
        BinaryOp::Ne => ordering.is_ne(),
        BinaryOp::Lt => ordering.is_lt(),
        BinaryOp::Le => ordering.is_le(),
        BinaryOp::Gt => ordering.is_gt(),
        _ => ordering.is_ge(),
      };

      return Some((Type::Bool, ConstValue::Bool(result)));
    }

    match (op, ty, lhs, rhs) {
      (BinaryOp::And, Type::Bool, ConstValue::Bool(a), ConstValue::Bool(b)) => Some((Type::Bool, ConstValue::Bool(a && b))),
      (BinaryOp::Or, Type::Bool, ConstValue::Bool(a), ConstValue::Bool(b)) => Some((Type::Bool, ConstValue::Bool(a || b))),
      (BinaryOp::Add, Type::String, ConstValue::Str(a), ConstValue::Str(b)) => Some((Type::String, ConstValue::Str(a + &b))),
      (BinaryOp::Div | BinaryOp::Rem, Type::Int(_), ConstValue::Int(_), ConstValue::Int(0)) => {
        self.error(codes::NOT_CONSTANT, span, "this constant divides by zero");

        None
      }
      (op, Type::Int(int), ConstValue::Int(a), ConstValue::Int(b)) => {
        let result = match op {
          BinaryOp::Add => a.checked_add(b),
          BinaryOp::Sub => a.checked_sub(b),
          BinaryOp::Mul => a.checked_mul(b),
          BinaryOp::Div => a.checked_div(b),
          BinaryOp::Rem => a.checked_rem(b),
          _ => return self.const_operand_error(span, &format!("`{}`", op.symbol()), ty),
        };

        self.const_int(result, *int, span)
      }
      (op, Type::Float(float), ConstValue::Float(a), ConstValue::Float(b)) => {
        let result = match op {
          BinaryOp::Add => a + b,
          BinaryOp::Sub => a - b,
          BinaryOp::Mul => a * b,
          BinaryOp::Div => a / b,
          BinaryOp::Rem => a % b,
          _ => return self.const_operand_error(span, &format!("`{}`", op.symbol()), ty),
        };

        self.const_float(result, *float, span)
      }
      (op, ty, _, _) => self.const_operand_error(span, &format!("`{}`", op.symbol()), ty),
    }
  }

  /// `as` between primitive types, with the rules of docs/spec/02-types.md#conversions.
  fn const_cast(&mut self, from: &Type, value: ConstValue, to: &Type, span: Span) -> Option<(Type, ConstValue)> {
    let width = self.package.target.pointer_width;

    match (value, to) {
      (value, to) if from == to => Some((to.clone(), value)),
      (ConstValue::Int(value), Type::Int(int)) => Some((Type::Int(*int), ConstValue::Int(wrap(value, *int, width)))),
      (ConstValue::Bool(value), Type::Int(int)) => Some((Type::Int(*int), ConstValue::Int(i128::from(value)))),
      (ConstValue::Char(value), Type::Int(int)) => Some((Type::Int(*int), ConstValue::Int(wrap(i128::from(u32::from(value)), *int, width)))),
      (ConstValue::Int(value), Type::Float(float)) => self.const_float(value as f64, *float, span),
      (ConstValue::Float(value), Type::Float(float)) => self.const_float(value, *float, span),
      (ConstValue::Float(value), Type::Int(int)) => {
        let max = i128::try_from(int.max(width)).ok()?;
        let min = if int.is_signed() { -max - 1 } else { 0 };
        // Saturating, with NaN as 0.
        let truncated = if value.is_nan() { 0 } else { (value.trunc().clamp(min as f64, max as f64)) as i128 };

        Some((Type::Int(*int), ConstValue::Int(truncated.clamp(min, max))))
      }
      (ConstValue::Int(value), Type::Char) if *from == Type::Int(IntTy::U8) => {
        Some((Type::Char, ConstValue::Char(char::from(u8::try_from(value).ok()?))))
      }
      _ => {
        self.error(codes::INVALID_CAST, span, format!("cannot cast `{from}` to `{to}`"));

        None
      }
    }
  }

  /// `include_str("path")`: the text of a file, by a path relative to the constant's file.
  fn include_str(&mut self, span: Span, args: &[Expr], module: usize) -> Option<(Type, ConstValue)> {
    let [arg] = args else {
      self.error(codes::INVALID_INCLUDE, span, "`include_str` takes one argument: the path of a file");

      return None;
    };
    let ExprKind::Str(relative) = &arg.kind else {
      self.error(codes::INVALID_INCLUDE, arg.span, "the path of `include_str` must be a string literal");

      return None;
    };
    let Some(location) = self.package.modules.get(module).and_then(|module| module.location.clone()) else {
      self.error(codes::INVALID_INCLUDE, span, "`include_str` needs a source file on disk");

      return None;
    };
    let path = std::path::Path::new(&location).parent().map_or_else(|| std::path::PathBuf::from(relative), |dir| dir.join(relative));

    match std::fs::read_to_string(&path) {
      Ok(text) => Some((Type::String, ConstValue::Str(text))),
      Err(error) => {
        // The reason by kind, not the system's message, which depends on the platform.
        let reason = match error.kind() {
          std::io::ErrorKind::NotFound => "the file does not exist",
          std::io::ErrorKind::PermissionDenied => "permission denied",
          std::io::ErrorKind::InvalidData => "the file is not UTF-8",
          _ => "the file cannot be read",
        };

        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_INCLUDE, arg.span, format!("cannot read `{}`: {reason}", path.display()))
            .with_note("the path is relative to the directory of this source file, and the file must be UTF-8"),
        );

        None
      }
    }
  }
}

fn is_comparison(op: BinaryOp) -> bool {
  matches!(op, BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge)
}

fn unary_symbol(op: UnaryOp) -> &'static str {
  match op {
    UnaryOp::Neg => "-",
    UnaryOp::Not => "!",
    UnaryOp::Deref => "*",
  }
}

/// `value` reduced to the width of `int`, keeping the low bits (two's complement).
fn wrap(value: i128, int: IntTy, pointer_width: u32) -> i128 {
  let bits = match int {
    IntTy::I8 | IntTy::U8 => 8,
    IntTy::I16 | IntTy::U16 => 16,
    IntTy::I32 | IntTy::U32 => 32,
    IntTy::I64 | IntTy::U64 => 64,
    IntTy::Isize | IntTy::Usize => pointer_width,
  };
  let modulus = 1i128 << bits;
  let low = value.rem_euclid(modulus);

  if int.is_signed() && low >= modulus / 2 { low - modulus } else { low }
}
