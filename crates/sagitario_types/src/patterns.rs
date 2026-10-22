//! Type checking of `match` (docs/spec/06-control-flow.md#match): arm types, patterns against
//! the scrutinee's type, guards, exhaustiveness and unreachable arms.

use sagitario_ast::{Expr, ExprKind, MatchArm, Pattern, PatternKind, UnaryOp};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{LocalId, Res, VariantRef};
use sagitario_span::Span;

use crate::exhaustive::{Ctor, Pat, Space};
use crate::{Checker, Type, VariantDef, plural};

/// How a variant pattern is written.
enum Shape<'p> {
  /// `None`, `Shape::Empty`
  Unit,
  /// `Some(x)`, `Shape::Circle(r)`
  Tuple(&'p [Pattern]),
  /// `Shape::Rect { w, .. }`
  Named(&'p [sagitario_ast::FieldPattern], bool),
}

impl Checker<'_> {
  pub(crate) fn match_expr(&mut self, scrutinee: &Expr, arms: &[MatchArm], expected: Option<&Type>) -> Type {
    let scrutinee_ty = self.expr(scrutinee, None);
    let mut result: Option<(Type, Span)> = None;
    let mut pattern_errors = false;

    for arm in arms {
      let errors = self.diagnostics.error_count();

      self.pattern(&arm.pattern, &scrutinee_ty);
      pattern_errors |= self.diagnostics.error_count() > errors;

      if let Some(guard) = &arm.guard {
        self.expect(guard, &Type::Bool);
      }

      let arm_expected = result.as_ref().map(|(ty, _)| ty.clone()).or_else(|| expected.cloned());
      let ty = self.expr(&arm.body, arm_expected.as_ref());

      match &result {
        None if ty != Type::Never => result = Some((ty, arm.body.span)),
        Some((first, first_span)) if !ty.fits(first) => {
          self.diagnostics.push(
            Diagnostic::error(codes::MISMATCHED_TYPES, arm.body.span, format!("expected `{first}`, found `{ty}`"))
              .with_label(*first_span, format!("this arm has type `{first}`"))
              .with_note("all arms of a `match` must have the same type"),
          );
        }
        _ => {}
      }
    }

    // Erroneous patterns would make the analysis report misleading gaps or unreachable arms.
    if !pattern_errors && !matches!(scrutinee_ty, Type::Error | Type::Never) {
      self.check_arms(scrutinee, &scrutinee_ty, arms);
    }

    result.map_or(Type::Never, |(ty, _)| ty)
  }

  /// Checks a pattern against the type of the value it matches and types its bindings.
  fn pattern(&mut self, pattern: &Pattern, ty: &Type) {
    match &pattern.kind {
      PatternKind::Wildcard => {}
      PatternKind::Ident(name) => match self.resolution.paths.get(&pattern.id).copied() {
        Some(Res::Variant(variant)) => self.variant_pattern(pattern.span, variant, ty, Shape::Unit),
        _ => {
          if let Some(local) = self.resolution.bindings.get(&pattern.id).copied() {
            self.bind(local, &name.name, name.span, ty);
          }
        }
      },
      PatternKind::Literal(expr) => self.literal_pattern(expr, ty),
      PatternKind::Path(_) => self.resolved_variant_pattern(pattern, ty, Shape::Unit),
      PatternKind::Tuple { elems, .. } => self.resolved_variant_pattern(pattern, ty, Shape::Tuple(elems)),
      PatternKind::Named { fields, rest, .. } => self.resolved_variant_pattern(pattern, ty, Shape::Named(fields, *rest)),
      PatternKind::Or(alternatives) => {
        for alternative in alternatives {
          self.pattern(alternative, ty);
        }
      }
    }
  }

  /// Gives a pattern binding its type; the alternatives of `|` must agree on it.
  fn bind(&mut self, local: LocalId, name: &str, span: Span, ty: &Type) {
    match self.locals.get(local.0 as usize).cloned().flatten() {
      Some(previous) if !ty.fits(&previous) && !previous.fits(ty) => {
        self.error(codes::MISMATCHED_TYPES, span, format!("`{name}` has type `{ty}` here but `{previous}` in the first alternative of the pattern"));
      }
      Some(_) => {}
      None => self.set_local(local, ty.clone()),
    }
  }

  fn resolved_variant_pattern(&mut self, pattern: &Pattern, ty: &Type, shape: Shape) {
    match self.resolution.paths.get(&pattern.id).copied() {
      Some(Res::Variant(variant)) => self.variant_pattern(pattern.span, variant, ty, shape),
      // Unresolved: already reported. Sub-patterns are still checked for their bindings.
      _ => self.sub_patterns_unchecked(&shape),
    }
  }

  fn sub_patterns_unchecked(&mut self, shape: &Shape) {
    match shape {
      Shape::Unit => {}
      Shape::Tuple(elems) => elems.iter().for_each(|elem| self.pattern(elem, &Type::Error)),
      Shape::Named(fields, _) => fields.iter().for_each(|field| self.pattern(&field.pattern, &Type::Error)),
    }
  }

  fn variant_pattern(&mut self, span: Span, variant: VariantRef, ty: &Type, shape: Shape) {
    let path = self.variant_path(variant);

    if *ty == Type::Error {
      return self.sub_patterns_unchecked(&shape);
    }

    let def = self.typed.variants(ty).and_then(|variants| variants.get(variant.index as usize).cloned());
    let Some(def) = def.filter(|_| ty.enum_kind() == Some(variant.owner)) else {
      self.error(codes::MISMATCHED_TYPES, span, format!("expected a pattern of type `{ty}`, found the variant `{path}`"));

      return self.sub_patterns_unchecked(&shape);
    };

    match shape {
      Shape::Unit if !def.fields.is_empty() => {
        let fix = if def.named { format!("{path} {{ .. }}") } else { format!("{path}({})", vec!["_"; def.fields.len()].join(", ")) };

        self.invalid_pattern(span, format!("the variant `{path}` carries data"), format!("match it with `{fix}`"));
      }
      Shape::Unit => {}
      Shape::Tuple(elems) => self.tuple_pattern(span, &path, &def, elems),
      Shape::Named(fields, rest) => self.named_pattern(span, &path, &def, fields, rest),
    }
  }

  fn invalid_pattern(&mut self, span: Span, message: String, help: String) {
    self.diagnostics.push(Diagnostic::error(codes::INVALID_PATTERN, span, message).with_help(help));
  }

  fn tuple_pattern(&mut self, span: Span, path: &str, def: &VariantDef, elems: &[Pattern]) {
    let fields = def.field_types();

    if def.named || fields.is_empty() {
      let help = if def.named { format!("match its fields by name: `{path} {{ .. }}`") } else { format!("write `{path}` without parentheses") };
      let message = if def.named { format!("the variant `{path}` has named fields") } else { format!("the variant `{path}` carries no data") };

      self.invalid_pattern(span, message, help);

      return elems.iter().for_each(|elem| self.pattern(elem, &Type::Error));
    }

    if elems.len() != fields.len() {
      self.invalid_pattern(
        span,
        format!("this pattern has {} field{}, but the variant `{path}` has {}", elems.len(), plural(elems.len()), fields.len()),
        "use `_` for the fields you do not need".to_string(),
      );
    }

    for (index, elem) in elems.iter().enumerate() {
      let field_ty = fields.get(index).cloned().unwrap_or(Type::Error);

      self.pattern(elem, &field_ty);
    }
  }

  fn named_pattern(&mut self, span: Span, path: &str, def: &VariantDef, fields: &[sagitario_ast::FieldPattern], rest: bool) {
    if !def.named {
      let help = if def.fields.is_empty() { format!("write `{path}` without braces") } else { format!("match its fields by position: `{path}(..)`") };

      self.invalid_pattern(span, format!("the variant `{path}` has no named fields"), help);

      return fields.iter().for_each(|field| self.pattern(&field.pattern, &Type::Error));
    }

    let mut seen: Vec<(&str, Span)> = Vec::new();

    for field in fields {
      let name = field.name.name.as_str();
      let field_ty = def.field_index(name).and_then(|index| def.fields.get(index)).map(|(_, ty)| ty.clone());

      if let Some((_, previous)) = seen.iter().find(|(seen, _)| *seen == name) {
        self.diagnostics.push(
          Diagnostic::error(codes::DUPLICATE_DEFINITION, field.name.span, format!("the field `{name}` appears more than once in the pattern"))
            .with_label(*previous, "first used here"),
        );
      } else if field_ty.is_none() {
        self.error(codes::UNKNOWN_MEMBER, field.name.span, format!("the variant `{path}` has no field `{name}`"));
      }

      seen.push((name, field.name.span));
      self.pattern(&field.pattern, &field_ty.unwrap_or(Type::Error));
    }

    let missing: Vec<String> = def
      .fields
      .iter()
      .filter_map(|(name, _)| name.as_ref())
      .filter(|name| !seen.iter().any(|(seen, _)| seen == name))
      .map(|name| format!("`{name}`"))
      .collect();

    if !rest && !missing.is_empty() {
      self.diagnostics.push(
        Diagnostic::error(
          codes::MISSING_FIELDS,
          span,
          format!("missing field{} {} in the pattern of `{path}`", plural(missing.len()), missing.join(", ")),
        )
        .with_help("match them too, or add `..` to ignore the remaining fields"),
      );
    }
  }

  /// Integer, character, boolean and string literals; floats are rejected.
  fn literal_pattern(&mut self, expr: &Expr, ty: &Type) {
    let inner = match &expr.kind {
      ExprKind::Unary { op: UnaryOp::Neg, operand } => &operand.kind,
      kind => kind,
    };

    if let ExprKind::Float { .. } = inner {
      self.expr(expr, None);
      self.invalid_pattern(expr.span, "float literals cannot be used in patterns".to_string(), "compare the value with `if` instead".to_string());

      return;
    }

    let hint = matches!(ty, Type::Int(_)).then_some(ty);
    let actual = self.expr(expr, hint);

    if !actual.fits(ty) {
      self.mismatch(expr.span, ty, &actual);
    }
  }

  /// Reports unreachable arms and values no arm matches.
  fn check_arms(&mut self, scrutinee: &Expr, ty: &Type, arms: &[MatchArm]) {
    let space = Space { typed: &self.typed };
    let mut rows: Vec<Vec<Pat>> = Vec::new();
    let mut unreachable = Vec::new();

    for arm in arms {
      let pat = self.simplify(&arm.pattern, ty);

      if space.useful(&rows, std::slice::from_ref(&pat), std::slice::from_ref(ty)).is_none() {
        unreachable.push(arm.pattern.span);
      }

      // A guard may fail, so its arm does not cover the values of its pattern.
      if arm.guard.is_none() {
        rows.push(vec![pat]);
      }
    }

    let mut missing = Vec::new();

    while missing.len() < 3 {
      let Some(witness) = space.useful(&rows, &[Pat::Wild], std::slice::from_ref(ty)) else { break };
      let pat = witness.into_iter().next().unwrap_or(Pat::Wild);

      missing.push(format!("`{}`", self.display(&pat, ty)));
      rows.push(vec![pat]);
    }

    for span in unreachable {
      self.diagnostics.push(
        Diagnostic::warning(codes::UNREACHABLE_PATTERN, span, "unreachable pattern").with_note("earlier arms already match every value it matches"),
      );
    }

    if !missing.is_empty() {
      let list = match missing.as_slice() {
        [only] => only.clone(),
        [init @ .., last] => format!("{} and {last}", init.join(", ")),
        [] => String::new(),
      };

      self.diagnostics.push(
        Diagnostic::error(codes::NON_EXHAUSTIVE_MATCH, scrutinee.span, format!("non-exhaustive patterns: {list} not covered"))
          .with_note(format!("the value has type `{ty}`; a `match` must handle every possible value"))
          .with_help("add an arm for each missing case, or a final `_ => ...` arm"),
      );
    }
  }

  /// The pattern as constructors over sub-patterns. Erroneous parts become `_`.
  fn simplify(&self, pattern: &Pattern, ty: &Type) -> Pat {
    let variant = match self.resolution.paths.get(&pattern.id) {
      Some(Res::Variant(variant)) if ty.enum_kind() == Some(variant.owner) => Some(*variant),
      _ => None,
    };
    let def = variant.and_then(|variant| self.typed.variants(ty)?.get(variant.index as usize).cloned());

    match (&pattern.kind, variant, def) {
      (PatternKind::Or(alternatives), _, _) => Pat::Or(alternatives.iter().map(|alternative| self.simplify(alternative, ty)).collect()),
      (PatternKind::Literal(expr), _, _) => literal_ctor(expr).map_or(Pat::Wild, |ctor| Pat::Ctor(ctor, Vec::new())),
      (PatternKind::Ident(_) | PatternKind::Path(_), Some(variant), Some(def)) => {
        Pat::Ctor(Ctor::Variant(variant.index), vec![Pat::Wild; def.fields.len()])
      }
      (PatternKind::Tuple { elems, .. }, Some(variant), Some(def)) if !def.named => {
        let fields = def.field_types();
        let args = fields.iter().enumerate().map(|(index, field)| elems.get(index).map_or(Pat::Wild, |elem| self.simplify(elem, field))).collect();

        Pat::Ctor(Ctor::Variant(variant.index), args)
      }
      (PatternKind::Named { fields, .. }, Some(variant), Some(def)) if def.named => {
        let args = def
          .fields
          .iter()
          .map(|(name, field_ty)| {
            let given = fields.iter().find(|field| Some(&field.name.name) == name.as_ref());

            given.map_or(Pat::Wild, |field| self.simplify(&field.pattern, field_ty))
          })
          .collect();

        Pat::Ctor(Ctor::Variant(variant.index), args)
      }
      _ => Pat::Wild,
    }
  }

  /// A witness pattern as source text, such as `Shape::Rect { .. }` or `Some(false)`.
  fn display(&self, pat: &Pat, ty: &Type) -> String {
    let Pat::Ctor(ctor, args) = pat else { return "_".to_string() };

    match ctor {
      Ctor::Bool(value) => value.to_string(),
      Ctor::Int(value) => value.to_string(),
      Ctor::Char(value) => format!("{value:?}"),
      Ctor::Str(value) => format!("{value:?}"),
      Ctor::Variant(index) => {
        let Some(kind) = ty.enum_kind() else { return "_".to_string() };
        let path = self.variant_path(VariantRef { owner: kind, index: *index });
        let def = self.typed.variants(ty).and_then(|variants| variants.get(*index as usize).cloned());
        let Some(def) = def else { return path };
        let shown: Vec<String> = args.iter().zip(def.fields.iter()).map(|(arg, (_, field_ty))| self.display(arg, field_ty)).collect();

        if def.fields.is_empty() {
          path
        } else if def.named {
          if shown.iter().all(|part| part == "_") {
            return format!("{path} {{ .. }}");
          }

          let parts: Vec<String> =
            def.fields.iter().zip(&shown).map(|((name, _), part)| format!("{}: {part}", name.clone().unwrap_or_default())).collect();

          format!("{path} {{ {} }}", parts.join(", "))
        } else {
          format!("{path}({})", shown.join(", "))
        }
      }
    }
  }
}

fn literal_ctor(expr: &Expr) -> Option<Ctor> {
  match &expr.kind {
    ExprKind::Int { value, .. } => i128::try_from(*value).ok().map(Ctor::Int),
    ExprKind::Unary { op: UnaryOp::Neg, operand } => match &operand.kind {
      ExprKind::Int { value, .. } => i128::try_from(*value).ok().map(|value| Ctor::Int(-value)),
      _ => None,
    },
    ExprKind::Bool(value) => Some(Ctor::Bool(*value)),
    ExprKind::Char(value) => Some(Ctor::Char(*value)),
    ExprKind::Str(value) => Some(Ctor::Str(value.clone())),
    _ => None,
  }
}
