//! Lowering of `String` operators, the methods of `String`, `char` and `StringBuilder`,
//! `new StringBuilder()` and `for` over `chars()`/`bytes()`. They call runtime functions
//! (runtime/sagitario_runtime.h); `String` results are new values owned by the statement.

use sagitario_ast::{BinaryOp, Expr};
use sagitario_types::{BuiltinMethod, FloatTy, IntTy, Type};

use crate::{Callee, Const, FnLowering, LResult, Local, LowerError, Operand, Place, Rvalue, Stmt, error};

fn int(value: u128, negative: bool, ty: IntTy) -> Operand {
  Operand::Const(Const::Int { value, negative, ty })
}

fn runtime(function: &'static str) -> Callee {
  Callee::Runtime { function, located: false }
}

impl FnLowering<'_> {
  /// `lhs op rhs` where both operands have type `operand_ty`.
  pub(crate) fn binary(&mut self, op: BinaryOp, operand_ty: Type, lhs: Operand, rhs: Operand) -> Operand {
    if operand_ty != Type::String {
      let result_ty = if op.is_arithmetic() { operand_ty.clone() } else { Type::Bool };

      return self.new_value(result_ty, Rvalue::Binary { op, ty: operand_ty, lhs, rhs });
    }

    if op == BinaryOp::Add {
      return self.new_value(Type::String, Rvalue::Call { callee: runtime("sag_string_concat"), args: vec![lhs, rhs] });
    }

    // Comparisons: the byte-wise order from `sag_string_cmp` against zero.
    let order = self.new_value(Type::Int(IntTy::I32), Rvalue::Call { callee: runtime("sag_string_cmp"), args: vec![lhs, rhs] });

    self.new_value(Type::Bool, Rvalue::Binary { op, ty: Type::Int(IntTy::I32), lhs: order, rhs: int(0, false, IntTy::I32) })
  }

  pub(crate) fn text_method(&mut self, method: BuiltinMethod, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let mut operands = vec![self.expr(receiver)?];

    for arg in args {
      operands.push(self.expr(arg)?);
    }

    match method {
      BuiltinMethod::Runtime { function, located } => Ok(self.call(Callee::Runtime { function, located }, operands, ty.clone())),
      BuiltinMethod::Find => {
        let index = self.new_value(Type::Int(IntTy::Usize), Rvalue::Call { callee: runtime("sag_string_find"), args: operands });

        Ok(self.sentinel_option(ty, index, Type::Int(IntTy::Usize), u128::from(u64::MAX)))
      }
      BuiltinMethod::ToDigit => {
        let callee = Callee::Runtime { function: "sag_char_to_digit_at", located: true };
        let digit = self.new_value(Type::Int(IntTy::U32), Rvalue::Call { callee, args: operands });

        Ok(self.sentinel_option(ty, digit, Type::Int(IntTy::U32), u128::from(u32::MAX)))
      }
      BuiltinMethod::Parse(target) => self.parse(ty, &target, operands),
      other => error(format!("unexpected text method {other:?}")),
    }
  }

  /// `Some(value)` unless `value` is `sentinel`, then `None`.
  fn sentinel_option(&mut self, ty: &Type, value: Operand, value_ty: Type, sentinel: u128) -> Operand {
    let int_ty = match value_ty {
      Type::Int(int_ty) => int_ty,
      _ => IntTy::Usize,
    };
    let present =
      self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Ne, ty: value_ty, lhs: value.clone(), rhs: int(sentinel, false, int_ty) });
    let dest = self.temp(ty.clone());

    self.emit(Stmt::If {
      cond: present,
      then_body: vec![Stmt::Assign { dest, value: Rvalue::Variant { index: 1, fields: vec![value] } }],
      else_body: vec![Stmt::Assign { dest, value: Rvalue::Variant { index: 0, fields: Vec::new() } }],
    });

    Operand::Local(dest)
  }

  /// `text.parse_<target>()`: the runtime parses into a 64-bit value within the target's range
  /// and reports failures with a message.
  fn parse(&mut self, ty: &Type, target: &Type, mut operands: Vec<Operand>) -> LResult<Operand> {
    let (raw_ty, function) = match target {
      Type::Int(int_ty) if int_ty.is_signed() => {
        let max = int_ty.max(self.package.target.pointer_width);

        operands.push(int(max + 1, true, IntTy::I64));
        operands.push(int(max, false, IntTy::I64));

        (Type::Int(IntTy::I64), "sag_parse_signed")
      }
      Type::Int(int_ty) => {
        operands.push(int(int_ty.max(self.package.target.pointer_width), false, IntTy::U64));

        (Type::Int(IntTy::U64), "sag_parse_unsigned")
      }
      Type::Float(_) => (Type::Float(FloatTy::F64), "sag_parse_float"),
      other => return error(format!("cannot parse {other}")),
    };
    let raw = self.temp(raw_ty.clone());
    // Always written by the runtime: the message, or the empty string on success.
    let message = self.temp(Type::String);

    self.pending.push(message);
    operands.push(Operand::Ref(Place { local: raw, projections: Vec::new(), ty: raw_ty.clone() }));
    operands.push(Operand::Ref(Place { local: message, projections: Vec::new(), ty: Type::String }));

    let parsed = self.new_value(Type::Bool, Rvalue::Call { callee: runtime(function), args: operands });
    let dest = self.temp(ty.clone());
    let target = target.clone();
    let then_body = self.buffer(|lowering| {
      let value = lowering.new_value(target.clone(), Rvalue::Cast { from: raw_ty, to: target, operand: Operand::Local(raw) });

      lowering.emit(Stmt::Assign { dest, value: Rvalue::Variant { index: 0, fields: vec![value] } });

      Ok(())
    })?;
    let else_body =
      vec![Stmt::Retain(Operand::Local(message)), Stmt::Assign { dest, value: Rvalue::Variant { index: 1, fields: vec![Operand::Local(message)] } }];

    self.emit(Stmt::If { cond: parsed, then_body, else_body });

    Ok(self.owned_result(Some(dest)))
  }

  /// `for var in text.chars()` or `text.bytes()`, over the string as it was before the loop.
  pub(crate) fn for_text(&mut self, var: Local, iter: &Expr, bytes: bool, body: &sagitario_ast::Block) -> LResult<()> {
    let sagitario_ast::ExprKind::MethodCall { receiver, .. } = &iter.kind else {
      return Err(LowerError("`chars`/`bytes` without a receiver".to_string()));
    };
    let value = self.expr(receiver)?;
    let text = self.temp(Type::String);

    self.store(text, value);
    self.pending.push(text);

    let loop_body = self.loop_buffer(None, |lowering| lowering.block_into(body, None))?;

    self.emit(Stmt::ForText { var, text, bytes, body: loop_body });

    Ok(())
  }

  /// `CString::new(text)`: `Err` with Rust's message when the text contains a NUL byte.
  pub(crate) fn cstring_new(&mut self, ty: &Type, text: Operand) -> Operand {
    // Both are always written by the runtime: the C string or null, the message or "".
    let raw = self.temp(Type::CString);
    let message = self.temp(Type::String);

    self.pending.push(raw);
    self.pending.push(message);

    let args = vec![
      text,
      Operand::Ref(Place { local: raw, projections: Vec::new(), ty: Type::CString }),
      Operand::Ref(Place { local: message, projections: Vec::new(), ty: Type::String }),
    ];
    let created = self.new_value(Type::Bool, Rvalue::Call { callee: runtime("sag_cstring_new"), args });
    let dest = self.temp(ty.clone());
    let then_body =
      vec![Stmt::Retain(Operand::Local(raw)), Stmt::Assign { dest, value: Rvalue::Variant { index: 0, fields: vec![Operand::Local(raw)] } }];
    let else_body =
      vec![Stmt::Retain(Operand::Local(message)), Stmt::Assign { dest, value: Rvalue::Variant { index: 1, fields: vec![Operand::Local(message)] } }];

    self.emit(Stmt::If { cond: created, then_body, else_body });
    self.owned_result(Some(dest))
  }

  pub(crate) fn new_string_builder(&mut self) -> Operand {
    self.new_value(Type::StringBuilder, Rvalue::Call { callee: runtime("sag_builder_new"), args: Vec::new() })
  }
}
