//! SIR (Sagitario Intermediate Representation) and lowering from the type-checked AST
//! (docs/architecture.md#sir).
//!
//! SIR is structured (`If`/`Loop` instead of a control-flow graph) because it is emitted as C.
//! Every intermediate value lives in a numbered local; fields and array elements are reached
//! through places. ARC is explicit: locals of a *managed* type (one that contains a `String`)
//! own one reference to each string inside, released at the end of their block, on
//! `break`/`continue`/`return`, or when overwritten. Temporaries are released at the end of the
//! statement or branch that created them, so the set of pending releases always matches the
//! values created on the current path.
//!
//! Enums, `Option` and `Result` values carry a tag (the variant's index) and the fields of their
//! variant; `match` and `?` (in `matching`) test the tag before reading those fields.
//!
//! Lowering only runs on programs without errors, so an inconsistency it finds is an internal
//! compiler error, reported as `LowerError`.

mod matching;
mod text;

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Module, Stmt as AstStmt, TemplatePart, UnaryOp};
use sagitario_resolve::{Builtin, FnId, Res, Resolution};
use sagitario_span::SourceFile;
use sagitario_types::{BuiltinMethod, FloatTy, IntTy, Receiver, Type, TypedModule};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
  /// Indexed by `FnId`.
  pub functions: Vec<Function>,
  pub layouts: Layouts,
  pub entry: FnId,
  /// Path of the source file, for `#line` directives.
  pub source_path: String,
}

/// The fields of the program's structs and enums.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layouts {
  /// Indexed by `StructId`.
  pub structs: Vec<StructLayout>,
  /// Indexed by `EnumId`.
  pub enums: Vec<EnumLayout>,
}

impl Layouts {
  /// Whether values of `ty` hold `String`s, so copies must be retained and released.
  pub fn is_managed(&self, ty: &Type) -> bool {
    match ty {
      Type::String | Type::StringBuilder => true,
      Type::Array(elem, _) => self.is_managed(elem),
      Type::Struct(inner) => self.structs.get(inner.id.0 as usize).is_some_and(|layout| layout.fields.iter().any(|field| self.is_managed(field))),
      Type::Enum(_) | Type::Option(_) | Type::Result(..) => self.variants(ty).iter().flatten().flatten().any(|field| self.is_managed(field)),
      _ => false,
    }
  }

  /// The field types of each variant of an enum, `Option` or `Result` type.
  pub fn variants(&self, ty: &Type) -> Option<Vec<Vec<Type>>> {
    match ty {
      Type::Enum(inner) => self.enums.get(inner.id.0 as usize).map(|layout| layout.variants.clone()),
      Type::Option(value) => Some(vec![Vec::new(), vec![value.as_ref().clone()]]),
      Type::Result(value, error) => Some(vec![vec![value.as_ref().clone()], vec![error.as_ref().clone()]]),
      _ => None,
    }
  }

  pub fn struct_fields(&self, id: sagitario_resolve::StructId) -> Vec<Type> {
    self.structs.get(id.0 as usize).map(|layout| layout.fields.clone()).unwrap_or_default()
  }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnumLayout {
  pub name: String,
  /// The field types of each variant.
  pub variants: Vec<Vec<Type>>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructLayout {
  pub name: String,
  /// Field types in declaration order.
  pub fields: Vec<Type>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Function {
  pub name: String,
  /// The first locals are the parameters, starting with `self` in methods.
  pub param_count: usize,
  /// A `var self` method: local 0 refers to the caller's value instead of holding a copy.
  pub self_by_ref: bool,
  pub ret: Type,
  /// Type of every local: source bindings first (same numbering as `LocalId`), then
  /// temporaries.
  pub locals: Vec<Type>,
  pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Local(pub u32);

/// A local, or a field or element inside it.
#[derive(Debug, Clone, PartialEq)]
pub struct Place {
  pub local: Local,
  pub projections: Vec<Projection>,
  /// The type of the value the place holds.
  pub ty: Type,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Projection {
  /// The field at this position of a struct.
  Field(usize),
  /// A bounds-checked element of an array of length `len`.
  Index { index: Operand, len: u64 },
  /// The tag of an enum value: the index of its variant, as `u32`.
  Tag,
  /// A field of the given variant of an enum value; only valid when the tag matches.
  VariantField { variant: u32, field: usize },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
  /// The following statements come from this 1-based source line.
  Line(u32),
  Assign {
    dest: Local,
    value: Rvalue,
  },
  /// Stores into a field or element; like `Assign`, it does not release the previous value.
  AssignPlace {
    dest: Place,
    value: Rvalue,
  },
  /// Evaluates for side effects (a call returning `()`).
  Eval(Rvalue),
  /// Adds a reference to every `String` inside a managed value.
  Retain(Operand),
  /// Drops the references held by a managed local.
  Release(Local),
  /// Drops the references held by a managed field or element.
  ReleasePlace(Place),
  /// Makes a managed local hold an empty value without releasing its previous contents (a
  /// binding declared without a value).
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
  /// `for var in array`: `var` borrows each element of `array` (a copy taken before the loop).
  ForArray {
    var: Local,
    array: Local,
    len: u64,
    body: Vec<Stmt>,
  },
  /// `for var in text.chars()` (each `char`) or `text.bytes()` (each `u8`).
  ForText {
    var: Local,
    text: Local,
    bytes: bool,
    body: Vec<Stmt>,
  },
  Break,
  Continue,
  Return(Option<Operand>),
  /// Aborts the program with a message (exit code 101).
  Panic(PanicMessage),
}

#[derive(Debug, Clone, PartialEq)]
pub enum PanicMessage {
  Static(String),
  /// A `String` value.
  Value(Operand),
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
  /// A struct (fields in declaration order) or an array, of the destination's type. The
  /// operands are already retained for the new value.
  Aggregate(Vec<Operand>),
  /// An array of the destination's type with every element equal to the operand; each copy is
  /// retained.
  Repeat(Operand),
  /// An enum value of the destination's type with the given variant; the field operands are
  /// already retained for the new value.
  Variant {
    index: u32,
    fields: Vec<Operand>,
  },
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
  /// A runtime function; `located` ones also receive the source location (to report panics).
  Runtime {
    function: &'static str,
    located: bool,
  },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
  Local(Local),
  /// Reads a field or element.
  Place(Place),
  /// The address of a place: the receiver of a `var self` method.
  Ref(Place),
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

/// The type of enum tags.
const TAG_TYPE: Type = Type::Int(IntTy::U32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LowerError(pub String);

type LResult<T> = Result<T, LowerError>;

fn error<T>(message: impl Into<String>) -> LResult<T> {
  Err(LowerError(message.into()))
}

pub fn lower(module: &Module, resolution: &Resolution, typed: &TypedModule, source: &SourceFile) -> LResult<Program> {
  let entry = resolution.main.ok_or_else(|| LowerError("program without `main`".to_string()))?;
  let structs = typed
    .structs
    .iter()
    .map(|def| StructLayout { name: def.name.clone(), fields: def.fields.iter().map(|field| field.ty.clone()).collect() })
    .collect();
  let enums = typed
    .enums
    .iter()
    .map(|def| EnumLayout { name: def.name.clone(), variants: def.variants.iter().map(|variant| variant.field_types()).collect() })
    .collect();
  let layouts = Layouts { structs, enums };
  let mut functions = Vec::new();

  for (id, decl) in resolution.fn_decls(module) {
    functions.push(lower_function(id, decl, resolution, typed, &layouts, source)?);
  }

  Ok(Program { functions, layouts, entry, source_path: source.path().display().to_string() })
}

fn lower_function(
  id: FnId,
  decl: &FnDecl,
  resolution: &Resolution,
  typed: &TypedModule,
  layouts: &Layouts,
  source: &SourceFile,
) -> LResult<Function> {
  let sig = typed.sig(id).ok_or_else(|| LowerError(format!("no signature for `{}`", decl.name.name)))?;
  let locals = typed.local_types.get(id.0 as usize).cloned().ok_or_else(|| LowerError("missing local types".to_string()))?;
  let mut lowering = FnLowering {
    resolution,
    typed,
    layouts,
    source,
    ret: sig.ret.clone(),
    locals,
    buffers: vec![Vec::new()],
    scopes: Vec::new(),
    pending: Vec::new(),
    loops: Vec::new(),
  };

  if sig.ret == Type::Unit {
    lowering.block_into(&decl.body, None)?;
  } else {
    // The result is owned by the caller, so it is not released here.
    let result = lowering.temp(sig.ret.clone());

    lowering.block_into(&decl.body, Some(result))?;
    lowering.emit(Stmt::Return(Some(Operand::Local(result))));
  }

  let body = lowering.buffers.pop().unwrap_or_default();

  Ok(Function {
    name: decl.name.name.clone(),
    param_count: decl.params.len() + usize::from(sig.receiver.is_some()),
    self_by_ref: sig.receiver == Some(Receiver::Var),
    ret: sig.ret.clone(),
    locals: lowering.locals,
    body,
  })
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
  layouts: &'a Layouts,
  source: &'a SourceFile,
  /// The function's return type, for `?`.
  ret: Type,
  locals: Vec<Type>,
  /// Statement lists being built, innermost last.
  buffers: Vec<Vec<Stmt>>,
  /// Managed bindings declared in each open block, released when it closes.
  scopes: Vec<Vec<Local>>,
  /// Managed temporaries created on the current path and not yet released.
  pending: Vec<Local>,
  loops: Vec<LoopCx>,
}

/// Types without a runtime value: nothing needs to be stored.
fn is_valueless(ty: &Type) -> bool {
  matches!(ty, Type::Unit | Type::Never)
}

impl FnLowering<'_> {
  fn managed(&self, ty: &Type) -> bool {
    self.layouts.is_managed(ty)
  }

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
    if self.managed(&self.local_type(dest)) {
      self.emit(Stmt::Retain(value.clone()));
    }

    self.emit(Stmt::Assign { dest, value: Rvalue::Use(value) });
  }

  /// A copy of a borrowed managed value that lives until the end of the statement, so the
  /// original can change without invalidating it.
  fn owned_copy(&mut self, ty: Type, value: Operand) -> Operand {
    let borrowed = match &value {
      Operand::Local(local) => !self.pending.contains(local),
      Operand::Place(_) => true,
      Operand::Ref(_) | Operand::Const(_) => false,
    };

    if !borrowed || !self.managed(&ty) {
      return value;
    }

    let copy = self.temp(ty);

    self.store(copy, value);
    self.pending.push(copy);

    Operand::Local(copy)
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
        let managed = self.managed(&ty);

        match &stmt.init {
          Some(init) => {
            let value = self.expr(init)?;

            if !is_valueless(&self.type_of(init)?) {
              self.store(local, value);
            }
          }
          None if managed => self.emit(Stmt::Reset(local)),
          None => {}
        }

        if managed && let Some(scope) = self.scopes.last_mut() {
          scope.push(local);
        }
      }
      AstStmt::Assign { target, op, value, .. } if matches!(target.kind, ExprKind::Field { .. } | ExprKind::Index { .. }) => {
        let value = self.expr(value)?;
        let place = self.place(target)?;
        let ty = place.ty.clone();
        // `text += more` is `text = text + more`.
        let (op, value) = match op {
          Some(BinaryOp::Add) if ty == Type::String => (&None, self.binary(BinaryOp::Add, Type::String, Operand::Place(place.clone()), value)),
          _ => (op, value),
        };

        match op {
          None if self.managed(&ty) => {
            // Retain first: the new value may share strings with the old one (`p.a = p.a;`).
            self.emit(Stmt::Retain(value.clone()));
            self.emit(Stmt::ReleasePlace(place.clone()));
            self.emit(Stmt::AssignPlace { dest: place, value: Rvalue::Use(value) });
          }
          None => self.emit(Stmt::AssignPlace { dest: place, value: Rvalue::Use(value) }),
          Some(op) => {
            let lhs = Operand::Place(place.clone());

            self.emit(Stmt::AssignPlace { dest: place, value: Rvalue::Binary { op: *op, ty, lhs, rhs: value } });
          }
        }
      }
      AstStmt::Assign { target, op, value, .. } => {
        let Some(Res::Local(id)) = self.resolution.paths.get(&target.id).copied() else {
          return error("assignment to something other than a binding");
        };
        let local = Local(id.0);
        let ty = self.local_type(local);
        let value = self.expr(value)?;
        let (op, value) = match op {
          Some(BinaryOp::Add) if ty == Type::String => (&None, self.binary(BinaryOp::Add, Type::String, Operand::Local(local), value)),
          _ => (op, value),
        };

        match op {
          None if self.managed(&ty) => {
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

  /// The place a binding, field or element expression designates. Its base is lowered first;
  /// a base that is not a place (a call result, a literal) is stored in a temporary.
  fn place(&mut self, expr: &Expr) -> LResult<Place> {
    let ty = self.type_of(expr)?;

    match &expr.kind {
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        Some(Res::Local(id)) => Ok(Place { local: Local(id.0), projections: Vec::new(), ty }),
        other => error(format!("place of unsupported path {other:?}")),
      },
      ExprKind::Field { base, name } => {
        let Type::Struct(owner) = self.type_of(base)? else {
          return error("field of a value that is not a struct");
        };
        let index = self.typed.field_index(owner.id, &name.name).ok_or_else(|| LowerError(format!("unknown field `{}`", name.name)))?;
        let mut place = self.base_place(base)?;

        place.projections.push(Projection::Field(index));
        place.ty = ty;

        Ok(place)
      }
      ExprKind::Index { base, index } => {
        let Type::Array(_, len) = self.type_of(base)? else {
          return error("index into a value that is not an array");
        };
        let mut place = self.base_place(base)?;
        let index = self.expr(index)?;

        place.projections.push(Projection::Index { index, len });
        place.ty = ty;

        Ok(place)
      }
      _ => {
        let value = self.expr(expr)?;

        self.operand_place(value, ty)
      }
    }
  }

  fn base_place(&mut self, base: &Expr) -> LResult<Place> {
    match &base.kind {
      ExprKind::Path(_) | ExprKind::Field { .. } | ExprKind::Index { .. } => self.place(base),
      _ => {
        let ty = self.type_of(base)?;
        let value = self.expr(base)?;

        self.operand_place(value, ty)
      }
    }
  }

  fn operand_place(&mut self, value: Operand, ty: Type) -> LResult<Place> {
    match value {
      Operand::Local(local) => Ok(Place { local, projections: Vec::new(), ty }),
      Operand::Place(place) => Ok(place),
      value @ Operand::Const(_) => match self.new_value(ty.clone(), Rvalue::Use(value)) {
        Operand::Local(local) => Ok(Place { local, projections: Vec::new(), ty }),
        _ => error("constant without a local"),
      },
      Operand::Ref(_) => error("place of a reference"),
    }
  }

  /// Lowers an expression to an operand. Values of managed types that are newly created are
  /// pending temporaries; reading a binding, field or element of a managed type borrows it.
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
        Some(Res::Variant(variant)) => Ok(self.variant(ty, variant.index, Vec::new())),
        other => error(format!("unsupported path operand {other:?}")),
      },
      ExprKind::Try(inner) => self.try_expr(inner),
      ExprKind::New { .. } if ty == Type::StringBuilder => Ok(self.new_string_builder()),
      ExprKind::New { .. } => error("`new` of a type other than `StringBuilder`"),
      ExprKind::Match { scrutinee, arms } => self.match_expr(&ty, scrutinee, arms),
      ExprKind::Field { .. } | ExprKind::Index { .. } => {
        let place = self.place(expr)?;

        if self.managed(&ty) {
          return Ok(Operand::Place(place));
        }

        // Copied when read, so a later change to the place in the same expression is not seen.
        Ok(self.new_value(ty, Rvalue::Use(Operand::Place(place))))
      }
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
          Some(Res::Variant(variant)) => {
            let fields = self.variant_fields(&ty, variant.index);
            let mut operands = Vec::new();

            for (arg, field) in args.iter().zip(&fields) {
              operands.push(self.operand_as(arg, field)?);
            }

            return Ok(self.variant(ty, variant.index, operands));
          }
          other => return error(format!("unsupported callee {other:?}")),
        };
        let params = match callee {
          Callee::Fn(id) => self.typed.sig(id).map(|sig| sig.params.clone()),
          Callee::Builtin(builtin) => Some(sagitario_types::builtin_sig(builtin).params),
          Callee::Runtime { .. } => None,
        }
        .unwrap_or_default();
        let mut operands = Vec::new();

        for (arg, param) in args.iter().zip(&params) {
          operands.push(self.operand_as(arg, param)?);
        }

        Ok(self.call(callee, operands, ty))
      }
      ExprKind::MethodCall { receiver, args, .. } if self.typed.builtin_methods.contains_key(&expr.id) => {
        let method = self.typed.builtin_methods.get(&expr.id).cloned().ok_or_else(|| LowerError("missing built-in method".to_string()))?;

        match method {
          BuiltinMethod::Runtime { .. } | BuiltinMethod::Find | BuiltinMethod::ToDigit | BuiltinMethod::Parse(_) => {
            self.text_method(method, &ty, receiver, args)
          }
          _ => self.builtin_method(method, &ty, receiver, args),
        }
      }
      ExprKind::MethodCall { receiver, args, .. } => {
        let Some(function) = self.typed.method_calls.get(&expr.id).copied() else {
          // The built-in `len()` of arrays: the length is part of the type.
          let Type::Array(_, len) = self.type_of(receiver)? else {
            return error("method call without a target");
          };

          self.expr(receiver)?;

          return Ok(Operand::Const(Const::Int { value: u128::from(len), negative: false, ty: IntTy::Usize }));
        };
        let sig = self.typed.sig(function).cloned().ok_or_else(|| LowerError("method without a signature".to_string()))?;
        let by_ref = sig.receiver == Some(Receiver::Var);
        let mut operands = vec![if by_ref { Operand::Ref(self.base_place(receiver)?) } else { self.expr(receiver)? }];

        for (arg, param) in args.iter().zip(&sig.params) {
          let value = self.operand_as(arg, param)?;

          // The method may modify the receiver, so arguments must not borrow from it.
          operands.push(if by_ref { self.owned_copy(param.clone(), value) } else { value });
        }

        Ok(self.call(Callee::Fn(function), operands, ty))
      }
      ExprKind::StructLit { fields, .. } if matches!(ty, Type::Enum(_)) => {
        let Some(Res::Variant(variant)) = self.resolution.paths.get(&expr.id).copied() else {
          return error("variant literal without a variant");
        };
        let def = self.typed.variants(&ty).and_then(|variants| variants.get(variant.index as usize).cloned());
        let def = def.ok_or_else(|| LowerError("unknown variant".to_string()))?;
        let mut operands: Vec<Option<Operand>> = vec![None; def.fields.len()];

        // Field values run in source order and are stored in declaration order.
        for init in fields {
          let index = def.field_index(&init.name.name).ok_or_else(|| LowerError("unknown variant field".to_string()))?;
          let field_ty = def.fields.get(index).map_or(Type::Error, |(_, ty)| ty.clone());
          let value = self.operand_as(&init.value, &field_ty)?;

          if let Some(slot) = operands.get_mut(index) {
            *slot = Some(value);
          }
        }

        let operands: Option<Vec<Operand>> = operands.into_iter().collect();
        let operands = operands.ok_or_else(|| LowerError("variant literal with missing fields".to_string()))?;

        Ok(self.variant(ty, variant.index, operands))
      }
      ExprKind::StructLit { fields, .. } => {
        let Type::Struct(owner) = &ty else {
          return error("struct literal without a struct type");
        };
        let field_types = self.layouts.struct_fields(owner.id);
        let mut operands: Vec<Option<Operand>> = vec![None; field_types.len()];

        // Field values run in source order and are stored in declaration order.
        for init in fields {
          let index = self.typed.field_index(owner.id, &init.name.name).ok_or_else(|| LowerError("unknown field in literal".to_string()))?;
          let field_ty = field_types.get(index).cloned().unwrap_or(Type::Error);
          let value = self.operand_as(&init.value, &field_ty)?;

          if let Some(slot) = operands.get_mut(index) {
            *slot = Some(value);
          }
        }

        let operands: Option<Vec<Operand>> = operands.into_iter().collect();
        let operands = operands.ok_or_else(|| LowerError("struct literal with missing fields".to_string()))?;

        Ok(self.aggregate(ty, operands, &field_types))
      }
      ExprKind::ArrayLit(elems) => {
        let Type::Array(elem_ty, _) = &ty else {
          return error("array literal without an array type");
        };
        let mut operands = Vec::new();

        for elem in elems {
          operands.push(self.operand_as(elem, elem_ty)?);
        }

        let field_types = vec![elem_ty.as_ref().clone(); operands.len()];

        Ok(self.aggregate(ty.clone(), operands, &field_types))
      }
      ExprKind::ArrayRepeat { value, .. } => {
        let Type::Array(elem_ty, _) = &ty else {
          return error("array repeat without an array type");
        };
        let value = self.operand_as(value, elem_ty)?;

        Ok(self.new_value(ty, Rvalue::Repeat(value)))
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

        if matches!(operand_ty, Type::Enum(_)) {
          // Enums without data are equal when their variants are.
          let lhs = self.tag(lhs, &operand_ty)?;
          let rhs = self.tag(rhs, &operand_ty)?;

          return Ok(self.new_value(ty, Rvalue::Binary { op: *op, ty: TAG_TYPE, lhs, rhs }));
        }

        Ok(self.binary(*op, operand_ty, lhs, rhs))
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
        let var = self.binding(expr.id)?;

        match self.typed.builtin_methods.get(&iter.id) {
          Some(BuiltinMethod::Chars) => return self.for_text(var, iter, false, body).map(|()| UNIT),
          Some(BuiltinMethod::Bytes) => return self.for_text(var, iter, true, body).map(|()| UNIT),
          _ => {}
        }

        let iter_ty = self.type_of(iter)?;

        if let Type::Array(_, len) = iter_ty {
          // The loop runs over a copy, so changing the array inside the body does not change
          // the iteration (arrays are values).
          let value = self.expr(iter)?;
          let array = self.temp(iter_ty.clone());

          self.store(array, value);

          if self.managed(&iter_ty) {
            self.pending.push(array);
          }

          let loop_body = self.loop_buffer(None, |lowering| lowering.block_into(body, None))?;

          self.emit(Stmt::ForArray { var, array, len, body: loop_body });

          return Ok(UNIT);
        }

        let ExprKind::Range { start, end, inclusive } = &iter.kind else {
          return error("`for` over something other than a range or an array");
        };
        let Type::Int(int) = iter_ty else {
          return error("range without an integer type");
        };
        // Bounds are evaluated once, before the loop, into their own locals.
        let start = self.expr(start)?;
        let start = self.new_value(Type::Int(int), Rvalue::Use(start));
        let end = self.expr(end)?;
        let end = self.new_value(Type::Int(int), Rvalue::Use(end));
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

            if self.managed(&self.type_of(value)?) {
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

  /// Lowers an expression used where a value of type `ty` is stored. A diverging expression
  /// has no value; the unreachable use gets a zero-initialized local of the right C type.
  fn operand_as(&mut self, expr: &Expr, ty: &Type) -> LResult<Operand> {
    let value = self.expr(expr)?;

    if self.type_of(expr)? == Type::Never && !is_valueless(ty) {
      return Ok(Operand::Local(self.temp(ty.clone())));
    }

    Ok(value)
  }

  fn call(&mut self, callee: Callee, args: Vec<Operand>, ty: Type) -> Operand {
    let call = Rvalue::Call { callee, args };

    if is_valueless(&ty) {
      self.emit(Stmt::Eval(call));

      return UNIT;
    }

    self.new_value(ty, call)
  }

  /// A new struct or array whose parts (of the given types) are retained for it.
  fn aggregate(&mut self, ty: Type, operands: Vec<Operand>, part_types: &[Type]) -> Operand {
    for (operand, part_ty) in operands.iter().zip(part_types) {
      if self.managed(part_ty) {
        self.emit(Stmt::Retain(operand.clone()));
      }
    }

    self.new_value(ty, Rvalue::Aggregate(operands))
  }

  /// Stores a computed value in a new local; managed values become pending temporaries.
  fn new_value(&mut self, ty: Type, value: Rvalue) -> Operand {
    let local = self.temp(ty.clone());

    self.emit(Stmt::Assign { dest: local, value });

    if self.managed(&ty) {
      self.pending.push(local);
    }

    Operand::Local(local)
  }

  /// The operand for a value already stored (owned) in `dest` by branches or `break`s.
  fn owned_result(&mut self, dest: Option<Local>) -> Operand {
    let Some(dest) = dest else { return UNIT };

    if self.managed(&self.local_type(dest)) {
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
