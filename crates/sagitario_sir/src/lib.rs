//! SIR (Sagitario Intermediate Representation) and lowering from the type-checked AST.
//! See docs/architecture.md#sir. Lowering runs only on programs without errors, so any
//! inconsistency it finds is an internal compiler error, reported as `LowerError`.

use sagitario_ast::{Expr, ExprKind, FnDecl, Module, Stmt};
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
  pub params: Vec<Type>,
  pub ret: Type,
  pub body: Vec<Inst>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Inst {
  /// The following instructions come from this 1-based source line.
  Line(u32),
  Call {
    callee: Callee,
    args: Vec<Operand>,
  },
  /// Evaluates an operand and discards its value.
  Eval(Operand),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Callee {
  Fn(FnId),
  Builtin(Builtin),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
  Param(u32),
  Const(Const),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Const {
  Unit,
  Bool(bool),
  Char(char),
  Int(u128, IntTy),
  /// Decimal text as written in the source, without separators.
  Float(String, FloatTy),
  Str(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError(pub String);

pub fn lower(module: &Module, resolution: &Resolution, typed: &TypedModule, source: &SourceFile) -> Result<Program, LowerError> {
  let entry = resolution.main.ok_or_else(|| LowerError("program without `main`".to_string()))?;
  let lowering = Lowering { resolution, typed, source };
  let functions = resolution.fn_decls(module).map(|(id, decl)| lowering.function(id, decl)).collect::<Result<_, _>>()?;

  Ok(Program { functions, entry, source_path: source.path().display().to_string() })
}

struct Lowering<'a> {
  resolution: &'a Resolution,
  typed: &'a TypedModule,
  source: &'a SourceFile,
}

impl Lowering<'_> {
  fn function(&self, id: FnId, decl: &FnDecl) -> Result<Function, LowerError> {
    let sig = self.typed.sig(id).ok_or_else(|| LowerError(format!("no signature for `{}`", decl.name.name)))?;
    let mut body = Vec::new();
    let exprs = decl.body.stmts.iter().map(|Stmt::Expr(expr)| expr).chain(decl.body.tail.as_deref());

    for expr in exprs {
      body.push(Inst::Line(self.source.line_col(expr.span.start).line));
      body.push(self.statement(expr)?);
    }

    Ok(Function { name: decl.name.name.clone(), params: sig.params.clone(), ret: sig.ret.clone(), body })
  }

  fn statement(&self, expr: &Expr) -> Result<Inst, LowerError> {
    match &expr.kind {
      ExprKind::Call { callee, args } => {
        let callee = match self.resolution.paths.get(&callee.id) {
          Some(Res::Fn(id)) => Callee::Fn(*id),
          Some(Res::Builtin(builtin)) => Callee::Builtin(*builtin),
          other => return Err(LowerError(format!("unsupported callee {other:?}"))),
        };
        let args = args.iter().map(|arg| self.operand(arg)).collect::<Result<_, _>>()?;

        Ok(Inst::Call { callee, args })
      }
      _ => Ok(Inst::Eval(self.operand(expr)?)),
    }
  }

  fn operand(&self, expr: &Expr) -> Result<Operand, LowerError> {
    let constant = match &expr.kind {
      ExprKind::Unit => Const::Unit,
      ExprKind::Bool(value) => Const::Bool(*value),
      ExprKind::Char(value) => Const::Char(*value),
      ExprKind::Str(value) => Const::Str(value.clone()),
      ExprKind::Int { value, .. } => match self.typed.expr_types.get(&expr.id) {
        Some(Type::Int(ty)) => Const::Int(*value, *ty),
        other => return Err(LowerError(format!("integer literal typed as {other:?}"))),
      },
      ExprKind::Float { text, .. } => match self.typed.expr_types.get(&expr.id) {
        Some(Type::Float(ty)) => Const::Float(text.clone(), *ty),
        other => return Err(LowerError(format!("float literal typed as {other:?}"))),
      },
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        Some(Res::Param(index)) => return Ok(Operand::Param(*index)),
        other => return Err(LowerError(format!("unsupported path operand {other:?}"))),
      },
      ExprKind::Call { .. } => return Err(LowerError("calls used as values are not supported yet".to_string())),
    };

    Ok(Operand::Const(constant))
  }
}
