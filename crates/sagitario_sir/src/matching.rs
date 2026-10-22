//! Lowering of enum values, `match`, `?` and the methods of `Option` and `Result`.
//!
//! A `match` becomes a sequence of arms guarded by a `done` flag. Each arm first tests its
//! pattern without binding anything (so a failed test leaves nothing to release), then binds
//! the pattern's names (copies of the matched parts), checks the guard and runs the body.
//! Fields of a variant are only read inside a test of its tag.

use sagitario_ast::{BinaryOp, Expr, MatchArm, Pattern, PatternKind, UnaryOp};
use sagitario_resolve::Res;
use sagitario_types::{BuiltinMethod, IntTy, Type};

use crate::{Const, FnLowering, LResult, Local, LowerError, Operand, PanicMessage, Place, Projection, Rvalue, Stmt, TAG_TYPE, error, is_valueless};

const TRUE: Operand = Operand::Const(Const::Bool(true));
const FALSE: Operand = Operand::Const(Const::Bool(false));

/// A field of the variant `variant` of the enum value at `place`.
fn variant_place(place: &Place, variant: u32, field: usize, ty: Type) -> Place {
  let mut projections = place.projections.clone();

  projections.push(Projection::VariantField { variant, field });

  Place { local: place.local, projections, ty }
}

impl FnLowering<'_> {
  pub(crate) fn variant_fields(&self, ty: &Type, index: u32) -> Vec<Type> {
    self.layouts.variants(ty).and_then(|variants| variants.get(index as usize).cloned()).unwrap_or_default()
  }

  /// A new enum value; its managed fields are retained for it.
  pub(crate) fn variant(&mut self, ty: Type, index: u32, fields: Vec<Operand>) -> Operand {
    for (field, field_ty) in fields.iter().zip(self.variant_fields(&ty, index)) {
      if self.managed(&field_ty) {
        self.emit(Stmt::Retain(field.clone()));
      }
    }

    self.new_value(ty, Rvalue::Variant { index, fields })
  }

  /// The tag of an enum value.
  pub(crate) fn tag(&mut self, value: Operand, ty: &Type) -> LResult<Operand> {
    let mut place = self.operand_place(value, ty.clone())?;

    place.projections.push(Projection::Tag);
    place.ty = TAG_TYPE;

    Ok(Operand::Place(place))
  }

  /// Whether the enum value at `place` is the variant `index`.
  fn tag_is(&mut self, place: &Place, index: u32) -> LResult<Operand> {
    let tag = self.tag(Operand::Place(place.clone()), &place.ty)?;
    let index = Operand::Const(Const::Int { value: u128::from(index), negative: false, ty: IntTy::U32 });

    Ok(self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Eq, ty: TAG_TYPE, lhs: tag, rhs: index }))
  }

  /// Reads a place: managed values are borrowed, others copied.
  fn read(&mut self, place: Place) -> Operand {
    if self.managed(&place.ty) {
      return Operand::Place(place);
    }

    let ty = place.ty.clone();

    self.new_value(ty, Rvalue::Use(Operand::Place(place)))
  }

  fn value_place(&mut self, expr: &Expr) -> LResult<Place> {
    let ty = self.type_of(expr)?;
    let value = self.expr(expr)?;

    self.operand_place(value, ty)
  }

  /// `Option` has `None` (0) and `Some` (1); `Result` has `Ok` (0) and `Err` (1). Returns the
  /// index of the variant holding the value.
  fn success_variant(ty: &Type) -> u32 {
    if matches!(ty, Type::Option(_)) { 1 } else { 0 }
  }

  pub(crate) fn builtin_method(&mut self, method: BuiltinMethod, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let receiver_ty = self.type_of(receiver)?;
    let place = self.value_place(receiver)?;
    let success = Self::success_variant(&receiver_ty);
    let is_option = matches!(receiver_ty, Type::Option(_));

    match method {
      BuiltinMethod::IsSome | BuiltinMethod::IsOk => self.tag_is(&place, success),
      BuiltinMethod::IsNone | BuiltinMethod::IsErr => self.tag_is(&place, 1 - success),
      BuiltinMethod::Unwrap | BuiltinMethod::Expect | BuiltinMethod::UnwrapErr => {
        let wanted = if method == BuiltinMethod::UnwrapErr { 1 } else { success };
        let message = match method {
          BuiltinMethod::Expect => {
            let message = args.first().ok_or_else(|| LowerError("`expect` without a message".to_string()))?;

            PanicMessage::Value(self.operand_as(message, &Type::String)?)
          }
          BuiltinMethod::Unwrap if is_option => PanicMessage::Static("called `Option::unwrap()` on a `None` value".to_string()),
          BuiltinMethod::Unwrap => PanicMessage::Static("called `Result::unwrap()` on an `Err` value".to_string()),
          _ => PanicMessage::Static("called `Result::unwrap_err()` on an `Ok` value".to_string()),
        };
        let cond = self.tag_is(&place, wanted)?;

        self.emit(Stmt::If { cond, then_body: Vec::new(), else_body: vec![Stmt::Panic(message)] });

        Ok(self.read(variant_place(&place, wanted, 0, ty.clone())))
      }
      BuiltinMethod::UnwrapOr => {
        let default = args.first().ok_or_else(|| LowerError("`unwrap_or` without a default".to_string()))?;
        let default = self.operand_as(default, ty)?;
        let dest = self.temp(ty.clone());
        let cond = self.tag_is(&place, success)?;
        let field = variant_place(&place, success, 0, ty.clone());
        let then_body = self.buffer(|lowering| {
          let value = lowering.read(field);

          lowering.store(dest, value);

          Ok(())
        })?;
        let else_body = self.buffer(|lowering| {
          lowering.store(dest, default);

          Ok(())
        })?;

        self.emit(Stmt::If { cond, then_body, else_body });

        Ok(self.owned_result(Some(dest)))
      }
      other => error(format!("`{other:?}` is not a method of `Option` or `Result`")),
    }
  }

  /// `value?`: returns `None` or `Err(e)` from the function, or evaluates to the inner value.
  pub(crate) fn try_expr(&mut self, inner: &Expr) -> LResult<Operand> {
    let inner_ty = self.type_of(inner)?;
    let place = self.value_place(inner)?;
    let success = Self::success_variant(&inner_ty);
    let failure = 1 - success;
    let value_ty = self.variant_fields(&inner_ty, success).first().cloned().unwrap_or(Type::Error);
    let failed = self.tag_is(&place, failure)?;
    let ret = self.ret.clone();
    let error_field = match &inner_ty {
      Type::Result(_, error) => Some(variant_place(&place, failure, 0, error.as_ref().clone())),
      _ => None,
    };
    let then_body = self.buffer(|lowering| {
      // The returned value is owned by the caller, so it is not a pending temporary.
      let result = lowering.temp(ret);
      let fields: Vec<Operand> = error_field.iter().cloned().map(Operand::Place).collect();

      for (field, place) in fields.iter().zip(&error_field) {
        if lowering.managed(&place.ty) {
          lowering.emit(Stmt::Retain(field.clone()));
        }
      }

      lowering.emit(Stmt::Assign { dest: result, value: Rvalue::Variant { index: failure, fields } });
      lowering.release_for_exit(0, 0);
      lowering.emit(Stmt::Return(Some(Operand::Local(result))));

      Ok(())
    })?;

    self.emit(Stmt::If { cond: failed, then_body, else_body: Vec::new() });

    Ok(self.read(variant_place(&place, success, 0, value_ty)))
  }

  pub(crate) fn match_expr(&mut self, ty: &Type, scrutinee: &Expr, arms: &[MatchArm]) -> LResult<Operand> {
    let place = self.value_place(scrutinee)?;
    let dest = (!is_valueless(ty)).then(|| self.temp(ty.clone()));
    let done = self.temp(Type::Bool);

    self.emit(Stmt::Assign { dest: done, value: Rvalue::Use(FALSE) });

    for arm in arms {
      let pending = self.new_value(Type::Bool, Rvalue::Unary { op: UnaryOp::Not, ty: Type::Bool, operand: Operand::Local(done) });
      let body = self.buffer(|lowering| lowering.arm(arm, &place, dest, done))?;

      self.emit(Stmt::If { cond: pending, then_body: body, else_body: Vec::new() });
    }

    Ok(self.owned_result(dest))
  }

  fn arm(&mut self, arm: &MatchArm, place: &Place, dest: Option<Local>, done: Local) -> LResult<()> {
    let matched = self.test(&arm.pattern, place)?;
    let then_body = self.buffer(|lowering| {
      lowering.scopes.push(Vec::new());
      lowering.bind(&arm.pattern, place)?;

      let mut bound = Vec::new();

      lowering.pattern_locals(&arm.pattern, &mut bound);

      for local in bound {
        if lowering.managed(&lowering.local_type(local))
          && let Some(scope) = lowering.scopes.last_mut()
          && !scope.contains(&local)
        {
          scope.push(local);
        }
      }

      match &arm.guard {
        Some(guard) => {
          let mark = lowering.pending.len();
          let cond = lowering.expr(guard)?;
          let body = lowering.buffer(|lowering| lowering.arm_body(arm, dest, done))?;

          lowering.emit(Stmt::If { cond, then_body: body, else_body: Vec::new() });
          lowering.release_pending_from(mark);
        }
        None => lowering.arm_body(arm, dest, done)?,
      }

      let scope = lowering.scopes.pop().unwrap_or_default();

      for local in scope.into_iter().rev() {
        lowering.emit(Stmt::Release(local));
      }

      Ok(())
    })?;

    self.emit(Stmt::If { cond: matched, then_body, else_body: Vec::new() });

    Ok(())
  }

  fn arm_body(&mut self, arm: &MatchArm, dest: Option<Local>, done: Local) -> LResult<()> {
    let line = self.line(arm.body.span.start);

    self.emit(line);

    let mark = self.pending.len();
    let value = self.expr(&arm.body)?;

    if let Some(dest) = dest
      && !is_valueless(&self.type_of(&arm.body)?)
    {
      self.store(dest, value);
    }

    self.release_pending_from(mark);
    self.emit(Stmt::Assign { dest: done, value: Rvalue::Use(TRUE) });

    Ok(())
  }

  fn pattern_variant(&self, pattern: &Pattern) -> Option<u32> {
    match self.resolution.paths.get(&pattern.id) {
      Some(Res::Variant(variant)) => Some(variant.index),
      _ => None,
    }
  }

  /// Patterns that match every value: `_` and bindings.
  fn irrefutable(&self, pattern: &Pattern) -> bool {
    match &pattern.kind {
      PatternKind::Wildcard => true,
      PatternKind::Ident(_) => self.pattern_variant(pattern).is_none(),
      _ => false,
    }
  }

  /// The places of the sub-patterns of a variant pattern, with their patterns.
  fn sub_patterns<'p>(&self, pattern: &'p Pattern, place: &Place, variant: u32) -> LResult<Vec<(&'p Pattern, Place)>> {
    let types = self.variant_fields(&place.ty, variant);
    let field = |index: usize| variant_place(place, variant, index, types.get(index).cloned().unwrap_or(Type::Error));

    match &pattern.kind {
      PatternKind::Tuple { elems, .. } => Ok(elems.iter().enumerate().map(|(index, elem)| (elem, field(index))).collect()),
      PatternKind::Named { fields, .. } => {
        let def = self.typed.variants(&place.ty).and_then(|variants| variants.get(variant as usize).cloned());
        let def = def.ok_or_else(|| LowerError("unknown variant in pattern".to_string()))?;
        let mut out = Vec::new();

        for pattern_field in fields {
          let index = def.field_index(&pattern_field.name.name).ok_or_else(|| LowerError("unknown field in pattern".to_string()))?;

          out.push((&pattern_field.pattern, field(index)));
        }

        Ok(out)
      }
      _ => Ok(Vec::new()),
    }
  }

  /// Whether the value at `place` matches `pattern`, without binding anything.
  fn test(&mut self, pattern: &Pattern, place: &Place) -> LResult<Operand> {
    if self.irrefutable(pattern) {
      return Ok(TRUE);
    }

    match &pattern.kind {
      PatternKind::Literal(expr) => {
        let literal = self.expr(expr)?;

        Ok(self.binary(BinaryOp::Eq, place.ty.clone(), Operand::Place(place.clone()), literal))
      }
      PatternKind::Or(alternatives) => {
        let result = self.temp(Type::Bool);

        self.emit(Stmt::Assign { dest: result, value: Rvalue::Use(FALSE) });

        for alternative in alternatives {
          let matched = self.test(alternative, place)?;

          self.emit(Stmt::If { cond: matched, then_body: vec![Stmt::Assign { dest: result, value: Rvalue::Use(TRUE) }], else_body: Vec::new() });
        }

        Ok(Operand::Local(result))
      }
      _ => {
        let variant = self.pattern_variant(pattern).ok_or_else(|| LowerError("pattern without a variant".to_string()))?;
        let subs: Vec<(&Pattern, Place)> =
          self.sub_patterns(pattern, place, variant)?.into_iter().filter(|(sub, _)| !self.irrefutable(sub)).collect();
        let tag_matches = self.tag_is(place, variant)?;

        if subs.is_empty() {
          return Ok(tag_matches);
        }

        let result = self.temp(Type::Bool);

        self.emit(Stmt::Assign { dest: result, value: Rvalue::Use(FALSE) });

        let inner = self.buffer(|lowering| lowering.test_all(&subs, result))?;

        self.emit(Stmt::If { cond: tag_matches, then_body: inner, else_body: Vec::new() });

        Ok(Operand::Local(result))
      }
    }
  }

  /// Sets `result` if every sub-pattern matches, testing each only after the previous ones.
  fn test_all(&mut self, subs: &[(&Pattern, Place)], result: Local) -> LResult<()> {
    let Some(((pattern, place), rest)) = subs.split_first() else {
      self.emit(Stmt::Assign { dest: result, value: Rvalue::Use(TRUE) });

      return Ok(());
    };
    let matched = self.test(pattern, place)?;
    let inner = self.buffer(|lowering| lowering.test_all(rest, result))?;

    self.emit(Stmt::If { cond: matched, then_body: inner, else_body: Vec::new() });

    Ok(())
  }

  /// Stores copies of the matched parts in the pattern's bindings; the pattern is known to
  /// match.
  fn bind(&mut self, pattern: &Pattern, place: &Place) -> LResult<()> {
    match &pattern.kind {
      PatternKind::Ident(_) => {
        if self.pattern_variant(pattern).is_none()
          && let Some(local) = self.resolution.bindings.get(&pattern.id)
        {
          self.store(Local(local.0), Operand::Place(place.clone()));
        }

        Ok(())
      }
      PatternKind::Tuple { .. } | PatternKind::Named { .. } => {
        let variant = self.pattern_variant(pattern).ok_or_else(|| LowerError("pattern without a variant".to_string()))?;

        for (sub, sub_place) in self.sub_patterns(pattern, place, variant)? {
          self.bind(sub, &sub_place)?;
        }

        Ok(())
      }
      PatternKind::Or(alternatives) => {
        let mut bound = Vec::new();

        self.pattern_locals(pattern, &mut bound);

        if bound.is_empty() { Ok(()) } else { self.bind_first_match(alternatives, place) }
      }
      PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Path(_) => Ok(()),
    }
  }

  /// Binds the first alternative of `|` that matches.
  fn bind_first_match(&mut self, alternatives: &[Pattern], place: &Place) -> LResult<()> {
    let Some((first, rest)) = alternatives.split_first() else {
      return error("empty `|` pattern");
    };

    if rest.is_empty() {
      return self.bind(first, place);
    }

    let matched = self.test(first, place)?;
    let then_body = self.buffer(|lowering| lowering.bind(first, place))?;
    let else_body = self.buffer(|lowering| lowering.bind_first_match(rest, place))?;

    self.emit(Stmt::If { cond: matched, then_body, else_body });

    Ok(())
  }

  /// The locals a pattern binds.
  fn pattern_locals(&self, pattern: &Pattern, out: &mut Vec<Local>) {
    match &pattern.kind {
      PatternKind::Ident(_) => {
        if let Some(local) = self.resolution.bindings.get(&pattern.id) {
          out.push(Local(local.0));
        }
      }
      PatternKind::Tuple { elems: patterns, .. } | PatternKind::Or(patterns) => {
        patterns.iter().for_each(|pattern| self.pattern_locals(pattern, out));
      }
      PatternKind::Named { fields, .. } => fields.iter().for_each(|field| self.pattern_locals(&field.pattern, out)),
      PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Path(_) => {}
    }
  }
}
