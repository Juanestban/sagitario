//! SIR (Sagitario Intermediate Representation) and lowering from the type-checked AST
//! (docs/architecture.md#sir).
//!
//! SIR is structured (`If`/`Loop` instead of a control-flow graph) because it is emitted as C.
//! Every intermediate value lives in a numbered local. ARC is explicit: `String` locals own one
//! reference, released at the end of their block, on `break`/`continue`/`return`, or when
//! overwritten. Temporaries are released at the end of the statement or branch that created
//! them, so the set of pending releases always matches the values created on the current path.
//!
//! Lowering only runs on programs without errors, so an inconsistency it finds is an internal
//! compiler error, reported as `LowerError`.

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Module, Stmt as AstStmt, TemplatePart, UnaryOp};
use sagitario_resolve::{Builtin, FnId, Res, Resolution};
use sagitario_span::SourceFile;
use sagitario_types::{FloatTy, IntTy, Type, TypedModule};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
  /// Indexed by `FnId`.
  pub functions: Vec<Function>,
  pub entry: FnId,
  /// Path of the source file, for `#line` directives.
  pub source_path: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
  pub name: String,
  /// The first locals are the parameters.
  pub param_count: usize,
  pub ret: Type,
  /// Type of every local: source bindings first (same numbering as `LocalId`), then
  /// temporaries.
  pub locals: Vec<Type>,
  pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local(pub u32);

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
  /// The following statements come from this 1-based source line.
  Line(u32),
  Assign {
    dest: Local,
    value: Rvalue,
  },
  /// Evaluates for side effects (a call returning `()`).
  Eval(Rvalue),
  /// Adds a reference to a `String` value.
  Retain(Operand),
  /// Drops the reference held by a `String` local.
  Release(Local),
  /// Makes a `String` local hold the empty string without releasing its previous contents
  /// (a binding declared without a value).
  Reset(Local),
  If {
    cond: Operand,
    then_body: Vec<Stmt>,
    else_body: Vec<Stmt>,
  },
  /// Repeats forever; left with `Break` or `Return`.
  Loop {
    body: Vec<Stmt>,
  },
  /// `for var in start..end` (or `..=`); `start` and `end` are evaluated once, before the loop.
  ForRange {
    var: Local,
    ty: IntTy,
    start: Operand,
    end: Operand,
    inclusive: bool,
    body: Vec<Stmt>,
  },
  Break,
  Continue,
  Return(Option<Operand>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
  Use(Operand),
  /// `ty` is the operand type.
  Unary {
    op: UnaryOp,
    ty: Type,
    operand: Operand,
  },
  /// `ty` is the operand type; `&&` and `||` are lowered to `If`.
  Binary {
    op: BinaryOp,
    ty: Type,
    lhs: Operand,
    rhs: Operand,
  },
  Cast {
    from: Type,
    to: Type,
    operand: Operand,
  },
  Call {
    callee: Callee,
    args: Vec<Operand>,
  },
  /// Builds a new `String`.
  Template(Vec<TemplatePiece>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePiece {
  Text(String),
  Value(Type, Operand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callee {
  Fn(FnId),
  Builtin(Builtin),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
  Local(Local),
  Const(Const),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
  Unit,
  Bool(bool),
  Char(char),
  Int {
    value: u128,
    negative: bool,
    ty: IntTy,
  },
  /// Decimal text as written in the source (without separators), possibly with a leading `-`.
  Float(String, FloatTy),
  Str(String),
}

const UNIT: Operand = Operand::Const(Const::Unit);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError(pub String);

type LResult<T> = Result<T, LowerError>;

fn error<T>(message: impl Into<String>) -> LResult<T> {
  Err(LowerError(message.into()))
}

pub fn lower(module: &Module, resolution: &Resolution, typed: &TypedModule, source: &SourceFile) -> LResult<Program> {
  let entry = resolution.main.ok_or_else(|| LowerError("program without `main`".to_string()))?;
  let mut functions = Vec::new();

  for (id, decl) in resolution.fn_decls(module) {
    functions.push(lower_function(id, decl, resolution, typed, source)?);
  }

  Ok(Program { functions, entry, source_path: source.path().display().to_string() })
}

fn lower_function(id: FnId, decl: &FnDecl, resolution: &Resolution, typed: &TypedModule, source: &SourceFile) -> LResult<Function> {
  let sig = typed.sig(id).ok_or_else(|| LowerError(format!("no signature for `{}`", decl.name.name)))?;
  let locals = typed.local_types.get(id.0 as usize).cloned().ok_or_else(|| LowerError("missing local types".to_string()))?;
  let mut lowering =
    FnLowering { resolution, typed, source, locals, buffers: vec![Vec::new()], scopes: Vec::new(), pending: Vec::new(), loops: Vec::new() };

  if sig.ret == Type::Unit {
    lowering.block_into(&decl.body, None)?;
  } else {
    // The result is owned by the caller, so it is not released here.
    let result = lowering.temp(sig.ret.clone());

    lowering.block_into(&decl.body, Some(result))?;
    lowering.emit(Stmt::Return(Some(Operand::Local(result))));
  }

  let body = lowering.buffers.pop().unwrap_or_default();

  Ok(Function { name: decl.name.name.clone(), param_count: decl.params.len(), ret: sig.ret.clone(), locals: lowering.locals, body })
}

struct LoopCx {
  /// Number of open scopes when the loop started; deeper scopes are released on exit.
  scope_depth: usize,
  /// Number of pending temporaries when the loop started.
  pending_mark: usize,
  /// Where `break value` stores its value.
  result: Option<Local>,
}

struct FnLowering<'a> {
  resolution: &'a Resolution,
  typed: &'a TypedModule,
  source: &'a SourceFile,
  locals: Vec<Type>,
  /// Statement lists being built, innermost last.
  buffers: Vec<Vec<Stmt>>,
  /// `String` bindings declared in each open block, released when it closes.
  scopes: Vec<Vec<Local>>,
  /// `String` temporaries created on the current path and not yet released.
  pending: Vec<Local>,
  loops: Vec<LoopCx>,
}

fn is_arc(ty: &Type) -> bool {
  *ty == Type::String
}

/// Types without a runtime value: nothing needs to be stored.
fn is_valueless(ty: &Type) -> bool {
  matches!(ty, Type::Unit | Type::Never)
}

impl FnLowering<'_> {
  fn emit(&mut self, stmt: Stmt) {
    if let Some(buffer) = self.buffers.last_mut() {
      buffer.push(stmt);
    }
  }

  fn temp(&mut self, ty: Type) -> Local {
    self.locals.push(ty);

    Local(self.locals.len() as u32 - 1)
  }

  fn local_type(&self, local: Local) -> Type {
    self.locals.get(local.0 as usize).cloned().unwrap_or(Type::Error)
  }

  fn type_of(&self, expr: &Expr) -> LResult<Type> {
    self.typed.expr_type(expr).cloned().ok_or_else(|| LowerError(format!("expression without a type at byte {}", expr.span.start)))
  }

  fn binding(&self, id: sagitario_ast::NodeId) -> LResult<Local> {
    self.resolution.bindings.get(&id).map(|local| Local(local.0)).ok_or_else(|| LowerError("unresolved binding".to_string()))
  }

  fn line(&self, expr_start: u32) -> Stmt {
    Stmt::Line(self.source.line_col(expr_start).line)
  }

  /// Runs `f` with a new statement buffer and returns its statements. Temporaries created
  /// inside are released before the buffer closes.
  fn buffer(&mut self, f: impl FnOnce(&mut Self) -> LResult<()>) -> LResult<Vec<Stmt>> {
    let mark = self.pending.len();

    self.buffers.push(Vec::new());
    f(self)?;
    self.release_pending_from(mark);

    Ok(self.buffers.pop().unwrap_or_default())
  }

  fn release_pending_from(&mut self, mark: usize) {
    let released: Vec<Local> = self.pending.drain(mark.min(self.pending.len())..).collect();

    for local in released.into_iter().rev() {
      self.emit(Stmt::Release(local));
    }
  }

  /// Emits releases for scopes from `depth` and temporaries from `mark`, without forgetting
  /// them: used before `break`, `continue` and `return`, after which the path ends.
  fn release_for_exit(&mut self, depth: usize, mark: usize) {
    let mut releases: Vec<Local> = self.pending.get(mark..).unwrap_or_default().iter().rev().copied().collect();

    for scope in self.scopes.get(depth..).unwrap_or_default().iter().rev() {
      releases.extend(scope.iter().rev().copied());
    }

    for local in releases {
      self.emit(Stmt::Release(local));
    }
  }

  /// Stores an owned copy of `value` in `dest`, which holds nothing that needs releasing.
  fn store(&mut self, dest: Local, value: Operand) {
    if is_arc(&self.local_type(dest)) {
      self.emit(Stmt::Retain(value.clone()));
    }

    self.emit(Stmt::Assign { dest, value: Rvalue::Use(value) });
  }

  /// Lowers a block in a new scope. With `dest`, the tail's value is stored there.
  fn block_into(&mut self, block: &Block, dest: Option<Local>) -> LResult<()> {
    self.scopes.push(Vec::new());

    for stmt in &block.stmts {
      self.stmt(stmt)?;
    }

    if let Some(tail) = &block.tail {
      let start = tail.span.start;
      let line = self.line(start);

      self.emit(line);

      let mark = self.pending.len();
      let value = self.expr(tail)?;

      if let Some(dest) = dest
        && !is_valueless(&self.type_of(tail)?)
      {
        self.store(dest, value);
      }

      self.release_pending_from(mark);
    }

    let scope = self.scopes.pop().unwrap_or_default();

    for local in scope.into_iter().rev() {
      self.emit(Stmt::Release(local));
    }

    Ok(())
  }

  fn stmt(&mut self, stmt: &AstStmt) -> LResult<()> {
    let start = match stmt {
      AstStmt::Let(stmt) => stmt.span.start,
      AstStmt::Assign { span, .. } => span.start,
      AstStmt::Expr(expr) => expr.span.start,
    };
    let line = self.line(start);

    self.emit(line);

    let mark = self.pending.len();

    match stmt {
      AstStmt::Let(stmt) => {
        let local = self.binding(stmt.id)?;
        let ty = self.local_type(local);

        match &stmt.init {
          Some(init) => {
            let value = self.expr(init)?;

            if !is_valueless(&self.type_of(init)?) {
              self.store(local, value);
            }
          }
          None if is_arc(&ty) => self.emit(Stmt::Reset(local)),
          None => {}
        }

        if is_arc(&ty)
          && let Some(scope) = self.scopes.last_mut()
        {
          scope.push(local);
        }
      }
      AstStmt::Assign { target, op, value, .. } => {
        let Some(Res::Local(id)) = self.resolution.paths.get(&target.id).copied() else {
          return error("assignment to something other than a binding");
        };
        let local = Local(id.0);
        let ty = self.local_type(local);
        let value = self.expr(value)?;

        match op {
          None if is_arc(&ty) => {
            // Retain first: the new value may be the old one (`s = s;`).
            self.emit(Stmt::Retain(value.clone()));
            self.emit(Stmt::Release(local));
            self.emit(Stmt::Assign { dest: local, value: Rvalue::Use(value) });
          }
          None => self.emit(Stmt::Assign { dest: local, value: Rvalue::Use(value) }),
          Some(op) => {
            self.emit(Stmt::Assign { dest: local, value: Rvalue::Binary { op: *op, ty, lhs: Operand::Local(local), rhs: value } });
          }
        }
      }
      AstStmt::Expr(expr) => {
        self.expr(expr)?;
      }
    }

    self.release_pending_from(mark);

    Ok(())
  }

  /// Lowers an expression to an operand. `String` results that are new values are pending
  /// temporaries; reading a binding borrows it.
  fn expr(&mut self, expr: &Expr) -> LResult<Operand> {
    let ty = self.type_of(expr)?;

    match &expr.kind {
      ExprKind::Int { value, .. } => match ty {
        Type::Int(int) => Ok(Operand::Const(Const::Int { value: *value, negative: false, ty: int })),
        other => error(format!("integer literal typed as {other}")),
      },
      ExprKind::Float { text, .. } => match ty {
        Type::Float(float) => Ok(Operand::Const(Const::Float(text.clone(), float))),
        other => error(format!("float literal typed as {other}")),
      },
      ExprKind::Bool(value) => Ok(Operand::Const(Const::Bool(*value))),
      ExprKind::Char(value) => Ok(Operand::Const(Const::Char(*value))),
      ExprKind::Str(value) => Ok(Operand::Const(Const::Str(value.clone()))),
      ExprKind::Unit => Ok(UNIT),
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        Some(Res::Local(id)) => Ok(Operand::Local(Local(id.0))),
        other => error(format!("unsupported path operand {other:?}")),
      },
      ExprKind::Template(parts) => {
        let mut pieces = Vec::new();

        for part in parts {
          match part {
            TemplatePart::Text(text) => pieces.push(TemplatePiece::Text(text.clone())),
            TemplatePart::Expr(part) => {
              let part_ty = self.type_of(part)?;
              let value = self.expr(part)?;

              pieces.push(TemplatePiece::Value(part_ty, value));
            }
          }
        }

        Ok(self.new_value(ty, Rvalue::Template(pieces)))
      }
      ExprKind::Call { callee, args } => {
        let callee = match self.resolution.paths.get(&callee.id) {
          Some(Res::Fn(id)) => Callee::Fn(*id),
          Some(Res::Builtin(builtin)) => Callee::Builtin(*builtin),
          other => return error(format!("unsupported callee {other:?}")),
        };
        let mut operands = Vec::new();

        for arg in args {
          operands.push(self.expr(arg)?);
        }

        let call = Rvalue::Call { callee, args: operands };

        if is_valueless(&ty) {
          self.emit(Stmt::Eval(call));

          return Ok(UNIT);
        }

        Ok(self.new_value(ty, call))
      }
      ExprKind::Unary { op, operand } => {
        // A negated literal is a single constant, so `-128i8` never overflows.
        if *op == UnaryOp::Neg {
          match (&operand.kind, &ty) {
            (ExprKind::Int { value, .. }, Type::Int(int)) => {
              return Ok(Operand::Const(Const::Int { value: *value, negative: true, ty: *int }));
            }
            (ExprKind::Float { text, .. }, Type::Float(float)) => return Ok(Operand::Const(Const::Float(format!("-{text}"), *float))),
            _ => {}
          }
        }

        let operand_ty = self.type_of(operand)?;
        let value = self.expr(operand)?;

        Ok(self.new_value(ty, Rvalue::Unary { op: *op, ty: operand_ty, operand: value }))
      }
      ExprKind::Binary { op: op @ (BinaryOp::And | BinaryOp::Or), lhs, rhs } => {
        let result = self.temp(Type::Bool);
        let lhs = self.expr(lhs)?;
        let evaluate_rhs = |lowering: &mut Self| -> LResult<()> {
          let value = lowering.expr(rhs)?;

          lowering.emit(Stmt::Assign { dest: result, value: Rvalue::Use(value) });

          Ok(())
        };
        let short_circuit = |value: bool| {
          move |lowering: &mut Self| -> LResult<()> {
            lowering.emit(Stmt::Assign { dest: result, value: Rvalue::Use(Operand::Const(Const::Bool(value))) });

            Ok(())
          }
        };
        let (then_body, else_body) = if *op == BinaryOp::And {
          (self.buffer(evaluate_rhs)?, self.buffer(short_circuit(false))?)
        } else {
          (self.buffer(short_circuit(true))?, self.buffer(evaluate_rhs)?)
        };

        self.emit(Stmt::If { cond: lhs, then_body, else_body });

        Ok(Operand::Local(result))
      }
      ExprKind::Binary { op, lhs, rhs } => {
        let operand_ty = self.type_of(lhs)?;
        let lhs = self.expr(lhs)?;
        let rhs = self.expr(rhs)?;

        Ok(self.new_value(ty, Rvalue::Binary { op: *op, ty: operand_ty, lhs, rhs }))
      }
      ExprKind::Cast { expr: inner, .. } => {
        let from = self.type_of(inner)?;
        let value = self.expr(inner)?;

        Ok(self.new_value(ty.clone(), Rvalue::Cast { from, to: ty, operand: value }))
      }
      ExprKind::Block(block) => {
        let dest = (!is_valueless(&ty)).then(|| self.temp(ty.clone()));

        self.block_into(block, dest)?;

        Ok(self.owned_result(dest))
      }
      ExprKind::If { cond, then_block, else_branch } => {
        let cond = self.expr(cond)?;
        let dest = (!is_valueless(&ty)).then(|| self.temp(ty.clone()));
        let then_body = self.buffer(|lowering| lowering.block_into(then_block, dest))?;
        let else_body = match else_branch {
          Some(branch) => self.buffer(|lowering| {
            let value = lowering.expr(branch)?;

            if let Some(dest) = dest
              && !is_valueless(&lowering.type_of(branch)?)
            {
              lowering.store(dest, value);
            }

            Ok(())
          })?,
          None => Vec::new(),
        };

        self.emit(Stmt::If { cond, then_body, else_body });

        Ok(self.owned_result(dest))
      }
      ExprKind::While { cond, body } => {
        let loop_body = self.loop_buffer(None, |lowering| {
          let mark = lowering.pending.len();
          let cond = lowering.expr(cond)?;

          lowering.release_pending_from(mark);
          lowering.emit(Stmt::If { cond, then_body: Vec::new(), else_body: vec![Stmt::Break] });
          lowering.block_into(body, None)
        })?;

        self.emit(Stmt::Loop { body: loop_body });

        Ok(UNIT)
      }
      ExprKind::Loop { body } => {
        let dest = (!is_valueless(&ty)).then(|| self.temp(ty.clone()));
        let loop_body = self.loop_buffer(dest, |lowering| lowering.block_into(body, None))?;

        self.emit(Stmt::Loop { body: loop_body });

        Ok(self.owned_result(dest))
      }
      ExprKind::For { iter, body, .. } => {
        let ExprKind::Range { start, end, inclusive } = &iter.kind else {
          return error("`for` over something other than a range");
        };
        let Type::Int(int) = self.type_of(iter)? else {
          return error("range without an integer type");
        };
        // Bounds are evaluated once, before the loop, into their own locals.
        let start = self.expr(start)?;
        let start = self.new_value(Type::Int(int), Rvalue::Use(start));
        let end = self.expr(end)?;
        let end = self.new_value(Type::Int(int), Rvalue::Use(end));
        let var = self.binding(expr.id)?;
        let loop_body = self.loop_buffer(None, |lowering| lowering.block_into(body, None))?;

        self.emit(Stmt::ForRange { var, ty: int, start, end, inclusive: *inclusive, body: loop_body });

        Ok(UNIT)
      }
      ExprKind::Break { value } => {
        let Some((depth, mark, result)) = self.loops.last().map(|cx| (cx.scope_depth, cx.pending_mark, cx.result)) else {
          return error("`break` outside of a loop");
        };

        if let Some(value) = value {
          let value_ty = self.type_of(value)?;
          let operand = self.expr(value)?;

          if let Some(result) = result.filter(|_| !is_valueless(&value_ty)) {
            self.store(result, operand);
          }
        }

        self.release_for_exit(depth, mark);
        self.emit(Stmt::Break);

        Ok(UNIT)
      }
      ExprKind::Continue => {
        let Some((depth, mark)) = self.loops.last().map(|cx| (cx.scope_depth, cx.pending_mark)) else {
          return error("`continue` outside of a loop");
        };

        self.release_for_exit(depth, mark);
        self.emit(Stmt::Continue);

        Ok(UNIT)
      }
      ExprKind::Return { value } => {
        let operand = match value {
          Some(value) if !is_valueless(&self.type_of(value)?) => {
            let operand = self.expr(value)?;

            if is_arc(&self.type_of(value)?) {
              self.emit(Stmt::Retain(operand.clone()));
            }

            Some(operand)
          }
          Some(value) => {
            self.expr(value)?;

            None
          }
          None => None,
        };

        self.release_for_exit(0, 0);
        self.emit(Stmt::Return(operand));

        Ok(UNIT)
      }
      ExprKind::Range { .. } => error("range outside of `for`"),
    }
  }

  /// Stores a computed value in a new local; `String` values become pending temporaries.
  fn new_value(&mut self, ty: Type, value: Rvalue) -> Operand {
    let local = self.temp(ty.clone());

    self.emit(Stmt::Assign { dest: local, value });

    if is_arc(&ty) {
      self.pending.push(local);
    }

    Operand::Local(local)
  }

  /// The operand for a value already stored (owned) in `dest` by branches or `break`s.
  fn owned_result(&mut self, dest: Option<Local>) -> Operand {
    let Some(dest) = dest else { return UNIT };

    if is_arc(&self.local_type(dest)) {
      self.pending.push(dest);
    }

    Operand::Local(dest)
  }

  fn loop_buffer(&mut self, result: Option<Local>, f: impl FnOnce(&mut Self) -> LResult<()>) -> LResult<Vec<Stmt>> {
    self.loops.push(LoopCx { scope_depth: self.scopes.len(), pending_mark: self.pending.len(), result });

    let body = self.buffer(f);

    self.loops.pop();

    body
  }
}
