//! Flow analysis of one function: definite assignment, `const` bindings assigned at most once,
//! reachability, and the binding warnings of docs/spec/03-memory.md.
//!
//! Loop bodies are analyzed twice: a silent first pass finds what may already be assigned when
//! the body repeats (at the end of the body or at a `continue`), and the second pass starts from
//! that state. So `loop { x = 1; break; }` assigns a `const` once, while `while c { x = 1; }`
//! may assign it twice.

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Stmt, TemplatePart};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_resolve::{LocalId, LocalInfo, LocalKind, Res, Resolution};
use sagitario_span::Span;

pub(crate) fn check_function(decl: &FnDecl, locals: &[LocalInfo], resolution: &Resolution, diagnostics: &mut Diagnostics) {
  let count = locals.len();
  let mut flow = Flow {
    locals,
    resolution,
    diagnostics,
    silent: 0,
    state: State { assigned: vec![false; count], maybe: vec![false; count], reachable: true },
    used: vec![false; count],
    reassigned: vec![false; count],
    reported: vec![false; count],
    loops: Vec::new(),
  };

  for (index, local) in locals.iter().enumerate() {
    if local.kind == LocalKind::Param {
      flow.define(LocalId(index as u32), true);
    }
  }

  flow.block(&decl.body);
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
  resolution: &'a Resolution,
  diagnostics: &'a mut Diagnostics,
  /// While positive, diagnostics are not reported (first pass over a loop body).
  silent: usize,
  state: State,
  used: Vec<bool>,
  reassigned: Vec<bool>,
  /// Whether an unassigned use was already reported for each binding.
  reported: Vec<bool>,
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

impl Flow<'_> {
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

  fn assign(&mut self, id: LocalId, span: Span, compound: bool) {
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
      LocalKind::Param => Some(format!("cannot assign to the parameter `{}`", info.name)),
      LocalKind::ForVar => Some(format!("cannot assign to the loop variable `{}`", info.name)),
    };

    if let Some(message) = error {
      self.report(Diagnostic::error(codes::ASSIGN_TO_CONST, span, message).with_note(format!("declare `{}` with `var` to reassign it", info.name)));
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

    let name = self.info(id).map_or("?", |info| info.name.as_str()).to_string();
    let message = if get(&self.state.maybe, id) {
      format!("`{name}` may be used before being assigned")
    } else {
      format!("`{name}` is used before being assigned")
    };

    self.report(Diagnostic::error(codes::UNASSIGNED_BINDING, span, message));
  }

  fn expr(&mut self, expr: &Expr) {
    match &expr.kind {
      ExprKind::Path(_) => {
        if let Some(Res::Local(id)) = self.resolution.paths.get(&expr.id).copied() {
          self.read(id, expr.span);
        }
      }
      ExprKind::Call { callee, args } => {
        self.expr(callee);

        for arg in args {
          self.expr(arg);
        }
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
      ExprKind::Block(block) => self.block(block),
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

        self.state.reachable = false;
      }
      ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Str(_) | ExprKind::Unit => {}
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
      let used = self.used.get(index).copied().unwrap_or(true);
      let reassigned = self.reassigned.get(index).copied().unwrap_or(true);

      if !used && !local.name.starts_with('_') {
        self.diagnostics.push(
          Diagnostic::warning(codes::UNUSED_BINDING, local.span, format!("unused binding `{}`", local.name))
            .with_note(format!("prefix it with an underscore to silence this warning: `_{}`", local.name)),
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
    let text = format!("pub fn main() {{ {body} }}");
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.clone());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, &text, &mut diagnostics);
    let module = sagitario_parser::parse_module(file, tokens, &mut diagnostics);
    let resolution = sagitario_resolve::resolve(&module, &mut diagnostics);

    crate::check(&module, &resolution, &mut diagnostics);

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
    assert_eq!(codes_of("const x: i32; loop { if true { x = 1; continue; } break; }"), vec!["E0308", "W0001"]);
    assert_eq!(codes_of("for i in 0..3 { i = 1; }"), vec!["E0308", "W0001"]);
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
}
