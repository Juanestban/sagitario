//! Flow analysis of one function: definite assignment, `const` bindings assigned at most once,
//! reachability, and the binding warnings of docs/spec/03-memory.md. In a class's initializer
//! the fields of `self` are tracked like bindings: each must be assigned before it is read,
//! before `self` is used and before `new` returns (docs/spec/04-classes.md#construction).
//!
//! Loop bodies are analyzed twice: a silent first pass finds what may already be assigned when
//! the body repeats (at the end of the body or at a `continue`), and the second pass starts from
//! that state. So `loop { x = 1; break; }` assigns a `const` once, while `while c { x = 1; }`
//! may assign it twice.

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Pattern, PatternKind, Stmt, TemplatePart, UnaryOp};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_resolve::{LocalId, LocalInfo, LocalKind, Res, Resolution};
use sagitario_span::Span;

use crate::{BuiltinMethod, FieldDef, Receiver, Type, TypedModule};

/// The kind of type a function belongs to, for the rules about `self`.
pub(crate) enum Owner<'a> {
  /// A module function, or a function of a struct or enum: their `self` can be reassigned.
  Value,
  /// A function of a class; for its initializer, the class name and the fields `new` assigns.
  Class { initializer: Option<(&'a str, &'a [FieldDef])> },
}

pub(crate) fn check_function(
  decl: &FnDecl,
  locals: &[LocalInfo],
  owner: Owner,
  resolution: &Resolution,
  typed: &TypedModule,
  diagnostics: &mut Diagnostics,
) {
  let fields = match owner {
    Owner::Class { initializer: Some((_, fields)) } => fields.len(),
    _ => 0,
  };
  // The fields of an initializer's `self` follow the locals.
  let count = locals.len() + fields;
  let mut flow = Flow {
    locals,
    owner,
    resolution,
    typed,
    diagnostics,
    silent: 0,
    state: State { assigned: vec![false; count], maybe: vec![false; count], reachable: true },
    used: vec![false; locals.len()],
    reassigned: vec![false; locals.len()],
    reported: vec![false; count],
    self_reported: false,
    lambdas: Vec::new(),
    loops: Vec::new(),
  };

  for (index, local) in locals.iter().enumerate() {
    if matches!(local.kind, LocalKind::Param | LocalKind::SelfParam) {
      flow.define(LocalId(index as u32), true);
    }
  }

  flow.block(&decl.body);

  if flow.state.reachable {
    flow.report_missing_fields(decl.name.span, false);
  }

  flow.warn_bindings();
}

#[derive(Clone)]
struct State {
  /// Assigned on every path that reaches this point.
  assigned: Vec<bool>,
  /// Assigned on at least one path.
  maybe: Vec<bool>,
  reachable: bool,
}

/// What leaves or repeats a loop body.
#[derive(Default)]
struct LoopExits {
  /// Intersection of `assigned` at each `break`; `None` while there are none.
  break_assigned: Option<Vec<bool>>,
  /// Union of `maybe` at each `break`.
  break_maybe: Option<Vec<bool>>,
  /// Union of `maybe` at each `continue`.
  continue_maybe: Option<Vec<bool>>,
}

struct Flow<'a> {
  locals: &'a [LocalInfo],
  owner: Owner<'a>,
  resolution: &'a Resolution,
  typed: &'a TypedModule,
  diagnostics: &'a mut Diagnostics,
  /// While positive, diagnostics are not reported (first pass over a loop body).
  silent: usize,
  state: State,
  used: Vec<bool>,
  reassigned: Vec<bool>,
  /// Whether an unassigned use was already reported for each binding or field.
  reported: Vec<bool>,
  /// Whether a use of `self` before every field is assigned was already reported.
  self_reported: bool,
  /// The lambdas being analyzed, innermost last.
  lambdas: Vec<sagitario_resolve::LambdaInfo>,
  loops: Vec<LoopExits>,
}

fn set(flags: &mut [bool], id: LocalId, value: bool) {
  if let Some(flag) = flags.get_mut(id.0 as usize) {
    *flag = value;
  }
}

fn get(flags: &[bool], id: LocalId) -> bool {
  flags.get(id.0 as usize).copied().unwrap_or(false)
}

fn and(a: &[bool], b: &[bool]) -> Vec<bool> {
  a.iter().zip(b).map(|(a, b)| *a && *b).collect()
}

fn or(a: &[bool], b: &[bool]) -> Vec<bool> {
  a.iter().zip(b).map(|(a, b)| *a || *b).collect()
}

fn or_into(target: &mut Option<Vec<bool>>, value: &[bool]) {
  *target = Some(match target.take() {
    Some(previous) => or(&previous, value),
    None => value.to_vec(),
  });
}

impl<'a> Flow<'a> {
  fn report(&mut self, diagnostic: Diagnostic) {
    if self.silent == 0 {
      self.diagnostics.push(diagnostic);
    }
  }

  fn define(&mut self, id: LocalId, assigned: bool) {
    set(&mut self.state.assigned, id, assigned);
    set(&mut self.state.maybe, id, assigned);
  }

  fn info(&self, id: LocalId) -> Option<&LocalInfo> {
    self.locals.get(id.0 as usize)
  }

  fn initializer(&self) -> Option<(&'a str, &'a [FieldDef])> {
    match self.owner {
      Owner::Class { initializer } => initializer,
      Owner::Value => None,
    }
  }

  fn is_self(&self, expr: &Expr) -> bool {
    match self.resolution.paths.get(&expr.id) {
      Some(Res::Local(id)) => self.info(*id).is_some_and(|info| info.kind == LocalKind::SelfParam),
      _ => false,
    }
  }

  /// `self.name` in an initializer: the slot that tracks the field, and the field.
  fn self_field(&self, expr: &Expr) -> Option<(LocalId, &'a FieldDef)> {
    let (_, fields) = self.initializer()?;
    let ExprKind::Field { base, name } = &expr.kind else { return None };

    if !self.is_self(base) {
      return None;
    }

    let index = fields.iter().position(|field| field.name == name.name)?;

    Some((LocalId((self.locals.len() + index) as u32), fields.get(index)?))
  }

  /// The fields of an initializer's `self` that may still be unassigned.
  fn missing_fields(&self) -> Vec<&'a FieldDef> {
    let Some((_, fields)) = self.initializer() else { return Vec::new() };

    fields
      .iter()
      .enumerate()
      .filter(|(index, _)| !get(&self.state.assigned, LocalId((self.locals.len() + index) as u32)))
      .map(|(_, field)| field)
      .collect()
  }

  /// Reports fields an initializer has not assigned when it returns, at a `return` or at the
  /// end of its body.
  fn report_missing_fields(&mut self, span: Span, at_return: bool) {
    let missing = self.missing_fields();

    if missing.is_empty() {
      return;
    }

    let names: Vec<String> = missing.iter().map(|field| format!("`{}`", field.name)).collect();
    let fields = format!("field{} {}", if names.len() == 1 { "" } else { "s" }, names.join(", "));
    let message =
      if at_return { format!("`new` returns before assigning the {fields}") } else { format!("`new` does not assign the {fields} on every path") };
    let mut diagnostic = Diagnostic::error(codes::UNINITIALIZED_FIELD, span, message);

    for field in missing {
      diagnostic = diagnostic.with_label(field.span, "declared here");
    }

    self.report(diagnostic.with_help("assign every field of `self` in `new`"));
  }

  /// Reading `self.name` in an initializer.
  fn read_field(&mut self, slot: LocalId, field: &FieldDef, span: Span) {
    if self.silent > 0 || !self.state.reachable || get(&self.state.assigned, slot) || get(&self.reported, slot) {
      return;
    }

    set(&mut self.reported, slot, true);

    let message = if get(&self.state.maybe, slot) {
      format!("the field `{}` may be read before being assigned", field.name)
    } else {
      format!("the field `{}` is read before being assigned", field.name)
    };

    self.report(Diagnostic::error(codes::UNINITIALIZED_FIELD, span, message).with_label(field.span, "declared here"));
  }

  /// `self.name = value` in an initializer; a `const` field is assigned exactly once.
  fn assign_field(&mut self, target: &Expr, compound: bool) {
    let Some((slot, field)) = self.self_field(target) else { return };

    if !field.mutable && (compound || get(&self.state.maybe, slot)) {
      self.report(
        Diagnostic::error(codes::ASSIGN_TO_CONST, target.span, format!("cannot assign twice to the `const` field `{}`", field.name))
          .with_label(field.span, "declared `const` here")
          .with_help(format!("declare it `var {}` to assign it more than once", field.name)),
      );
    }

    set(&mut self.state.assigned, slot, true);
    set(&mut self.state.maybe, slot, true);
  }

  /// Using `self` in an initializer requires every field to be assigned.
  fn use_self(&mut self, span: Span) {
    if self.self_reported || self.silent > 0 || !self.state.reachable {
      return;
    }

    let missing: Vec<String> = self.missing_fields().iter().map(|field| format!("`{}`", field.name)).collect();

    if missing.is_empty() {
      return;
    }

    self.self_reported = true;
    self.report(
      Diagnostic::error(codes::UNINITIALIZED_FIELD, span, "`self` is used before every field is assigned")
        .with_note(format!("still unassigned here: {}", missing.join(", ")))
        .with_help("assign every field before calling methods on `self` or passing it along"),
    );
  }

  /// Joins the states of two paths that meet.
  fn merge(&mut self, other: State) {
    let current = std::mem::replace(&mut self.state, other.clone());
    let maybe = or(&current.maybe, &other.maybe);

    self.state = match (current.reachable, other.reachable) {
      (false, _) => State { maybe, ..other },
      (_, false) => State { maybe, ..current },
      _ => State { assigned: and(&current.assigned, &other.assigned), maybe, reachable: true },
    };
  }

  fn block(&mut self, block: &Block) {
    let mut warned = false;

    for stmt in &block.stmts {
      let span = match stmt {
        Stmt::Let(stmt) => stmt.span,
        Stmt::Assign { span, .. } => *span,
        Stmt::Expr(expr) => expr.span,
      };

      self.warn_unreachable(span, &mut warned);
      self.stmt(stmt);
    }

    if let Some(tail) = &block.tail {
      self.warn_unreachable(tail.span, &mut warned);
      self.expr(tail);
    }
  }

  fn warn_unreachable(&mut self, span: Span, warned: &mut bool) {
    if !self.state.reachable && !*warned {
      self.report(Diagnostic::warning(codes::UNREACHABLE_CODE, span, "unreachable code"));
      *warned = true;
    }
  }

  fn stmt(&mut self, stmt: &Stmt) {
    match stmt {
      Stmt::Let(stmt) => {
        if let Some(init) = &stmt.init {
          self.expr(init);
        }

        if let Some(id) = self.resolution.bindings.get(&stmt.id).copied() {
          self.define(id, stmt.init.is_some());
        }
      }
      Stmt::Assign { target, op, value, .. } if self.self_field(target).is_some() => {
        if op.is_some() {
          // A compound assignment reads the field first.
          self.expr(target);
        }

        self.expr(value);
        self.assign_field(target, op.is_some());
      }
      Stmt::Assign { target, value, .. }
        if matches!(target.kind, ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::Unary { op: UnaryOp::Deref, .. }) =>
      {
        // Modifying a part of a binding reads the binding and counts as reassigning it.
        self.expr(target);
        self.expr(value);
        self.mutate(target);
      }
      Stmt::Assign { target, op, value, .. } => {
        let local = match self.resolution.paths.get(&target.id) {
          Some(Res::Local(id)) => Some(*id),
          _ => None,
        };

        if op.is_some() {
          // A compound assignment reads the binding first.
          self.expr(target);
        }

        self.expr(value);

        if let Some(id) = local {
          self.assign(id, target.span, op.is_some());
        }
      }
      Stmt::Expr(expr) => self.expr(expr),
    }
  }

  /// Marks the binding a place belongs to as modified, for the unneeded-`var` warning. The
  /// fields of a class instance belong to the instance, not to the binding referring to it.
  fn mutate(&mut self, place: &Expr) {
    match &place.kind {
      ExprKind::Path(_) => {
        if let Some(Res::Local(id)) = self.resolution.paths.get(&place.id).copied() {
          set(&mut self.reassigned, id, true);
        }
      }
      ExprKind::Field { base, .. } | ExprKind::Index { base, .. } if !matches!(self.typed.expr_types.get(&base.id), Some(Type::Class(_))) => {
        self.mutate(base);
      }
      _ => {}
    }
  }

  fn assign(&mut self, id: LocalId, span: Span, compound: bool) {
    // Assigning a capture inside a lambda is reported by the type checker.
    if self.lambdas.last().is_some_and(|lambda| !lambda.owns(id)) {
      return;
    }

    let Some(info) = self.info(id).cloned() else { return };
    let again = compound || get(&self.state.maybe, id);
    let error = match info.kind {
      LocalKind::Var => {
        if again {
          set(&mut self.reassigned, id, true);
        }

        None
      }
      LocalKind::Const if again => Some(format!("cannot assign twice to the `const` binding `{}`", info.name)),
      LocalKind::Const => None,
      LocalKind::SelfParam if matches!(self.owner, Owner::Class { .. }) => Some("cannot assign to `self` in a class".to_string()),
      // A struct or enum method that replaces `self` modifies the caller's value.
      LocalKind::SelfParam => None,
      LocalKind::Param => Some(format!("cannot assign to the parameter `{}`", info.name)),
      LocalKind::ForVar => Some(format!("cannot assign to the loop variable `{}`", info.name)),
      LocalKind::Pattern => Some(format!("cannot assign to the pattern binding `{}`", info.name)),
    };

    if let Some(message) = error {
      let declared = match info.kind {
        LocalKind::Param => "parameter declared here",
        LocalKind::ForVar => "loop variable declared here",
        LocalKind::Pattern => "bound by the pattern here",
        LocalKind::SelfParam => "`self` is the instance itself",
        _ => "declared with `const` here",
      };
      let help = if info.kind == LocalKind::SelfParam {
        "assign the fields of `self` instead".to_string()
      } else {
        format!("declare `{}` with `var` to reassign it", info.name)
      };

      self.report(Diagnostic::error(codes::ASSIGN_TO_CONST, span, message).with_label(info.span, declared).with_help(help));
    }

    set(&mut self.state.assigned, id, true);
    set(&mut self.state.maybe, id, true);
  }

  fn read(&mut self, id: LocalId, span: Span) {
    set(&mut self.used, id, true);

    if self.silent > 0 || !self.state.reachable || get(&self.state.assigned, id) || get(&self.reported, id) {
      return;
    }

    set(&mut self.reported, id, true);

    let Some(info) = self.info(id).cloned() else { return };
    let message = if get(&self.state.maybe, id) {
      format!("`{}` may be used before being assigned", info.name)
    } else {
      format!("`{}` is used before being assigned", info.name)
    };

    self.report(
      Diagnostic::error(codes::UNASSIGNED_BINDING, span, message)
        .with_label(info.span, "declared here without a value")
        .with_help("assign it on every path before this use"),
    );
  }

  fn expr(&mut self, expr: &Expr) {
    match &expr.kind {
      ExprKind::Path(_) => {
        if let Some(Res::Local(id)) = self.resolution.paths.get(&expr.id).copied() {
          self.read(id, expr.span);

          if self.initializer().is_some() && self.is_self(expr) {
            self.use_self(expr.span);
          }
        }
      }
      ExprKind::Call { callee, args } => {
        self.expr(callee);
        self.exprs(args);

        // A call that never returns (`panic`) ends the path.
        if self.typed.expr_types.get(&expr.id) == Some(&crate::Type::Never) {
          self.state.reachable = false;
        }
      }
      ExprKind::New { args, .. } => self.exprs(args),
      ExprKind::MethodCall { receiver, method, args } => {
        // `zod.new(...)` runs before the fields of `self` are assigned.
        if !(receiver.kind == ExprKind::Zod && method.name == "new") {
          self.expr(receiver);
        }

        self.exprs(args);

        let function = self.typed.method_calls.get(&expr.id).copied();

        let as_mut_ptr = matches!(self.typed.builtin_methods.get(&expr.id), Some(BuiltinMethod::Runtime { function: "SAG_ARRAY_MUT_PTR", .. }));

        if as_mut_ptr || function.and_then(|function| self.typed.sig(function)).is_some_and(|sig| sig.receiver == Some(Receiver::Var)) {
          self.mutate(receiver);
        }
      }
      ExprKind::Lambda { body, .. } => {
        let Some(info) = self.resolution.lambdas.get(&expr.id).cloned() else { return };

        // The captures are copied when the lambda is created.
        for capture in &info.captures {
          self.read(*capture, expr.span);
        }

        // The body runs later, maybe several times: it is analyzed apart and changes nothing
        // here.
        let saved_state = self.state.clone();
        let saved_loops = std::mem::take(&mut self.loops);

        for param in &info.params {
          self.define(*param, true);
        }

        self.state.reachable = true;
        self.lambdas.push(info);
        self.expr(body);
        self.lambdas.pop();
        self.state = saved_state;
        self.loops = saved_loops;
      }
      // `zod.method(...)` uses `self`.
      ExprKind::Zod => {
        if self.initializer().is_some() {
          self.use_self(expr.span);
        }
      }
      ExprKind::Field { base, .. } => match self.self_field(expr) {
        Some((slot, field)) => self.read_field(slot, field, expr.span),
        None => self.expr(base),
      },
      ExprKind::Index { base, index } => {
        self.expr(base);
        self.expr(index);
      }
      ExprKind::StructLit { fields, .. } => {
        for field in fields {
          self.expr(&field.value);
        }
      }
      ExprKind::ArrayLit(elems) => self.exprs(elems),
      ExprKind::ArrayRepeat { value, .. } => self.expr(value),
      ExprKind::Try(inner) => self.expr(inner),
      ExprKind::Match { scrutinee, arms } => {
        self.expr(scrutinee);

        // Exactly one arm runs; each starts from the state after the scrutinee.
        let before = self.state.clone();
        let mut after: Option<State> = None;

        for arm in arms {
          self.state = before.clone();

          let mut bound = Vec::new();

          self.pattern_bindings(&arm.pattern, &mut bound);

          for local in bound {
            self.define(local, true);
          }

          if let Some(guard) = &arm.guard {
            self.expr(guard);
          }

          self.expr(&arm.body);

          if let Some(previous) = after.take() {
            self.merge(previous);
          }

          after = Some(self.state.clone());
        }

        self.state = after.unwrap_or(State { reachable: false, ..before });
      }
      ExprKind::Template(parts) => {
        for part in parts {
          if let TemplatePart::Expr(part) = part {
            self.expr(part);
          }
        }
      }
      ExprKind::Unary { operand, .. } => self.expr(operand),
      ExprKind::Cast { expr, .. } => self.expr(expr),
      ExprKind::Binary { op, lhs, rhs } => {
        self.expr(lhs);

        if matches!(op, BinaryOp::And | BinaryOp::Or) {
          // The right operand may not run.
          let skipped = self.state.clone();

          self.expr(rhs);
          self.merge(skipped);
        } else {
          self.expr(rhs);
        }
      }
      ExprKind::Range { start, end, .. } => {
        self.expr(start);
        self.expr(end);
      }
      ExprKind::Block(block) | ExprKind::Unsafe(block) => self.block(block),
      ExprKind::If { cond, then_block, else_branch } => {
        self.expr(cond);

        let before = self.state.clone();

        self.block(then_block);

        let after_then = std::mem::replace(&mut self.state, before);

        if let Some(branch) = else_branch {
          self.expr(branch);
        }

        self.merge(after_then);
      }
      ExprKind::While { cond, body } => {
        self.expr(cond);
        self.repeating_body(body, None, false);
      }
      ExprKind::For { iter, body, .. } => {
        self.expr(iter);

        let var = self.resolution.bindings.get(&expr.id).copied();

        self.repeating_body(body, var, false);
      }
      ExprKind::Loop { body } => self.repeating_body(body, None, true),
      ExprKind::Break { value } => {
        if let Some(value) = value {
          self.expr(value);
        }

        if self.state.reachable
          && let Some(exits) = self.loops.last_mut()
        {
          exits.break_assigned = Some(match exits.break_assigned.take() {
            Some(previous) => and(&previous, &self.state.assigned),
            None => self.state.assigned.clone(),
          });
          or_into(&mut exits.break_maybe, &self.state.maybe);
        }

        self.state.reachable = false;
      }
      ExprKind::Continue => {
        if self.state.reachable
          && let Some(exits) = self.loops.last_mut()
        {
          or_into(&mut exits.continue_maybe, &self.state.maybe);
        }

        self.state.reachable = false;
      }
      ExprKind::Return { value } => {
        if let Some(value) = value {
          self.expr(value);
        }

        // A `return` inside a lambda leaves the lambda, not the initializer.
        if self.state.reachable && self.lambdas.is_empty() {
          self.report_missing_fields(expr.span, true);
        }

        self.state.reachable = false;
      }
      ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Str(_) | ExprKind::Unit => {}
    }
  }

  /// The locals a pattern binds.
  fn pattern_bindings(&self, pattern: &Pattern, out: &mut Vec<LocalId>) {
    match &pattern.kind {
      PatternKind::Ident(_) => {
        if let Some(local) = self.resolution.bindings.get(&pattern.id) {
          out.push(*local);
        }
      }
      PatternKind::Tuple { elems: patterns, .. } | PatternKind::Or(patterns) => {
        patterns.iter().for_each(|pattern| self.pattern_bindings(pattern, out));
      }
      PatternKind::Named { fields, .. } => fields.iter().for_each(|field| self.pattern_bindings(&field.pattern, out)),
      PatternKind::Wildcard | PatternKind::Literal(_) | PatternKind::Path(_) => {}
    }
  }

  fn exprs(&mut self, exprs: &[Expr]) {
    for expr in exprs {
      self.expr(expr);
    }
  }

  /// Analyzes one run of a loop body starting from `start`, returning the state at its end and
  /// what left or repeated it.
  fn body_pass(&mut self, start: State, body: &Block, var: Option<LocalId>) -> (State, LoopExits) {
    self.state = start;

    if let Some(var) = var {
      // The loop variable is a fresh binding on each iteration.
      self.define(var, true);
    }

    self.loops.push(LoopExits::default());
    self.block(body);

    let exits = self.loops.pop().unwrap_or_default();

    (self.state.clone(), exits)
  }

  /// A loop body that may repeat. `while` and `for` bodies may also not run at all; a `loop`
  /// body runs at least once and is only left through `break`.
  fn repeating_body(&mut self, body: &Block, var: Option<LocalId>, is_loop: bool) {
    let before = self.state.clone();

    self.silent += 1;

    let (end, exits) = self.body_pass(before.clone(), body, var);

    self.silent -= 1;

    let mut repeat_maybe = exits.continue_maybe.unwrap_or_else(|| vec![false; before.maybe.len()]);

    if end.reachable {
      repeat_maybe = or(&repeat_maybe, &end.maybe);
    }

    let start = State { maybe: or(&before.maybe, &repeat_maybe), ..before.clone() };
    let (end, exits) = self.body_pass(start, body, var);
    let mut maybe = or(&before.maybe, &end.maybe);

    for extra in [&exits.break_maybe, &exits.continue_maybe].into_iter().flatten() {
      maybe = or(&maybe, extra);
    }

    self.state = if is_loop {
      match exits.break_assigned {
        Some(assigned) => State { assigned, maybe, reachable: before.reachable },
        None => State { assigned: before.assigned, maybe, reachable: false },
      }
    } else {
      State { assigned: before.assigned, maybe, reachable: before.reachable }
    };
  }

  fn warn_bindings(&mut self) {
    for (index, local) in self.locals.iter().enumerate() {
      if local.kind == LocalKind::SelfParam {
        continue;
      }

      let used = self.used.get(index).copied().unwrap_or(true);
      let reassigned = self.reassigned.get(index).copied().unwrap_or(true);

      if !used && !local.name.starts_with('_') {
        self.diagnostics.push(
          Diagnostic::warning(codes::UNUSED_BINDING, local.span, format!("unused binding `{}`", local.name))
            .with_help(format!("prefix it with an underscore to silence this warning: `_{}`", local.name)),
        );
      }

      if local.kind == LocalKind::Var && !reassigned {
        self.diagnostics.push(Diagnostic::warning(
          codes::UNNEEDED_VAR,
          local.span,
          format!("`{}` is declared with `var` but never reassigned; use `const`", local.name),
        ));
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use sagitario_diagnostics::Diagnostics;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn codes_of(body: &str) -> Vec<&'static str> {
    codes_with("", body)
  }

  /// Like `codes_of`, with `items` declared before `main`.
  fn codes_with(items: &str, body: &str) -> Vec<&'static str> {
    let text = format!("{items}\npub fn main() {{ {body} }}");
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.clone());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, &text, &mut diagnostics);
    let package = sagitario_ast::Package::single(sagitario_parser::parse_module(file, tokens, &mut diagnostics));
    let resolution = sagitario_resolve::resolve(&package, &mut diagnostics);

    crate::check(&package, &resolution, &mut diagnostics);

    diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect()
  }

  #[test]
  fn accepts_definitely_assigned_bindings() {
    assert_eq!(codes_of("const x: i32; if true { x = 1; } else { x = 2; } println(`${x}`);"), Vec::<&str>::new());
    assert_eq!(codes_of("const x: i32; if true { x = 1; } else { return; } println(`${x}`);"), Vec::<&str>::new());
    assert_eq!(codes_of("const x: i32; loop { x = 1; break; } println(`${x}`);"), Vec::<&str>::new());
    assert_eq!(codes_of("var x = 0; while x < 3 { x += 1; } println(`${x}`);"), Vec::<&str>::new());
    assert_eq!(codes_of("for i in 0..3 { const y: i32; y = i; println(`${y}`); }"), Vec::<&str>::new());
  }

  #[test]
  fn rejects_possibly_unassigned_reads() {
    assert_eq!(codes_of("const x: i32; println(`${x}`);"), vec!["E0309"]);
    assert_eq!(codes_of("const x: i32; if true { x = 1; } println(`${x}`);"), vec!["E0309"]);
    assert_eq!(codes_of("var x: i32; while false { x = 1; } println(`${x}`);"), vec!["E0309"]);
    assert_eq!(codes_of("var x: i32; x += 1;"), vec!["E0309"]);
  }

  #[test]
  fn rejects_reassigning_consts_params_and_loop_variables() {
    assert_eq!(codes_of("const x = 1; x = 2; println(`${x}`);"), vec!["E0308"]);
    assert_eq!(codes_of("const x: i32; while true { x = 1; } println(`${x}`);"), vec!["E0308", "E0309"]);
    assert_eq!(codes_of("const x: i32; loop { if true { x = 1; continue; } break; }"), vec!["W0001", "E0308"]);
    assert_eq!(codes_of("for i in 0..3 { i = 1; }"), vec!["W0001", "E0308"]);
    assert_eq!(codes_of("const x = 1; x += 1;"), vec!["E0308"]);
  }

  #[test]
  fn warns_about_bindings_and_unreachable_code() {
    assert_eq!(codes_of("const x = 1;"), vec!["W0001"]);
    assert_eq!(codes_of("const _x = 1;"), Vec::<&str>::new());
    assert_eq!(codes_of("var x = 1; println(`${x}`);"), vec!["W0002"]);
    assert_eq!(codes_of("var x: i32; x = 1; println(`${x}`);"), vec!["W0002"]);
    assert_eq!(codes_of("return; println(\"never\");"), vec!["W0003"]);
  }

  #[test]
  fn modifying_parts_of_a_var_counts_as_reassigning_it() {
    let point = "struct P { pub x: i32; pub fn bump(self) { self.x += 1; } pub fn get(self) -> i32 { self.x } }";

    assert_eq!(codes_with(point, "var p = P { x: 1 }; p.x = 2;"), Vec::<&str>::new());
    assert_eq!(codes_with(point, "var p = P { x: 1 }; p.bump();"), Vec::<&str>::new());
    assert_eq!(codes_with(point, "var a = [1, 2]; a[0] += 1;"), Vec::<&str>::new());
    assert_eq!(codes_with(point, "var p = P { x: 1 }; println(`${p.get()}`);"), vec!["W0002"]);
    assert_eq!(codes_with(point, "var p: P; p.x = 1;"), vec!["E0309"]);
    assert_eq!(codes_with("struct Q { pub fn f(self) { self = Q {}; } }", ""), Vec::<&str>::new());
    assert_eq!(codes_with("class C { pub fn f(self) { self = new C(); } }", ""), vec!["E0308"]);
  }

  #[test]
  fn initializers_assign_every_field() {
    let class =
      |init: &str| codes_with(&format!("class C {{ pub const a: i32; pub var b: i32; pub fn new() {{ {init} }} pub fn m(self) {{}} }}"), "");

    assert_eq!(class("self.a = 1; self.b = self.a; self.b += 1; self.m();"), Vec::<&str>::new());
    assert_eq!(class("self.a = 1;"), vec!["E0325"]);
    assert_eq!(class("self.b = self.a; self.a = 1;"), vec!["E0325"]);
    assert_eq!(class("self.a = 1; self.m(); self.b = 2;"), vec!["E0325"]);
    assert_eq!(class("self.a = 1; if true { return; } self.b = 2;"), vec!["E0325"]);
    assert_eq!(class("self.a = 1; self.b = 2; self.a = 3;"), vec!["E0308"]);
    assert_eq!(class("if true { self.a = 1; } else { self.a = 2; } self.b = 0;"), Vec::<&str>::new());
  }

  #[test]
  fn panic_ends_the_path() {
    assert_eq!(codes_of("panic(\"stop\"); println(\"never\");"), vec!["W0003"]);
    assert_eq!(codes_of("const x: i32; if true { x = 1; } else { panic(\"no\"); } println(`${x}`);"), Vec::<&str>::new());
  }

  #[test]
  fn match_arms_are_alternative_paths() {
    assert_eq!(codes_of("const x: i32; match Some(1) { Some(n) => { x = n; } None => { x = 0; } } println(`${x}`);"), Vec::<&str>::new());
    assert_eq!(codes_of("const x: i32; match Some(1) { Some(n) => { x = n; } None => {} } println(`${x}`);"), vec!["E0309"]);
    assert_eq!(codes_of("match Some(1) { Some(n) => {} None => {} }"), vec!["W0001"]);
    assert_eq!(codes_of("match Some(1) { Some(n) => { n = 2; } None => {} }"), vec!["W0001", "E0308"]);
    assert_eq!(codes_of("const x = match 1 { _ => return }; println(`${x}`);"), vec!["W0003"]);
  }
}
