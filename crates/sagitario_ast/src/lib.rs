//! Abstract syntax tree. It mirrors the source closely; names and types are resolved by later
//! phases, which refer to nodes by `NodeId`.

use sagitario_span::{FileId, Span};

/// Identifies an expression or declaration so later phases can attach information to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct NodeId(pub u32);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ident {
  pub name: String,
  pub span: Span,
}

/// Visibility of module-level items (docs/spec/09-modules.md) and of struct members.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
  /// No modifier: visible in the declaring module only.
  Module,
  /// `pub`
  Public,
  /// `prv`: a struct member visible only inside its struct.
  Private,
}

/// The modules of a program (docs/spec/09-modules.md): the root module (the entry file) first,
/// then every module it reaches through paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
  pub modules: Vec<Module>,
}

impl Package {
  /// A program made of one file.
  pub fn single(module: Module) -> Package {
    Package { modules: vec![module] }
  }
}

/// One source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
  pub file: FileId,
  /// The logical path from the package root: empty for the root, `["net", "http"]` for
  /// `net/http.sag`.
  pub path: Vec<String>,
  pub uses: Vec<UseDecl>,
  pub items: Vec<Item>,
}

/// `use tree;`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseDecl {
  pub tree: UseTree,
  pub span: Span,
}

/// `a::b`, `a::b as c` or `a::{b, c::d}`; inside braces, `self` names the prefix itself.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UseTree {
  pub prefix: Vec<Ident>,
  pub kind: UseTreeKind,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UseTreeKind {
  /// Imports the last segment of the prefix, under `alias` if given.
  Simple { alias: Option<Ident> },
  /// `prefix::{trees}`
  Group(Vec<UseTree>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
  Fn(FnDecl),
  Struct(StructDecl),
  Enum(EnumDecl),
  Extern(ExternBlock),
}

/// `extern "C" { fn name(params) -> Ret; ... }`: C functions the program calls
/// (docs/spec/10-ffi.md).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternBlock {
  pub functions: Vec<ExternFn>,
  pub span: Span,
}

/// A C function declared in an `extern "C"` block; its name is its C symbol.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternFn {
  pub visibility: Visibility,
  pub name: Ident,
  pub params: Vec<Param>,
  pub return_type: Option<TypeExpr>,
  pub span: Span,
}

/// `@export("symbol")` on an `extern "C" fn`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Export {
  pub symbol: String,
  pub span: Span,
}

/// `enum Name { variants, then functions }`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDecl {
  pub visibility: Visibility,
  pub name: Ident,
  pub variants: Vec<VariantDecl>,
  pub functions: Vec<FnDecl>,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDecl {
  pub name: Ident,
  pub fields: VariantFields,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VariantFields {
  /// `Empty`
  Unit,
  /// `Circle(f64)`
  Tuple(Vec<TypeExpr>),
  /// `Rect { w: f64, h: f64 }`
  Named(Vec<VariantFieldDecl>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantFieldDecl {
  pub name: Ident,
  pub ty: TypeExpr,
  pub span: Span,
}

/// `struct Name { fields and functions }`; members keep their source order within each list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDecl {
  /// `@repr(C)`: C layout, usable across the FFI boundary.
  pub repr_c: bool,
  pub visibility: Visibility,
  pub name: Ident,
  pub fields: Vec<FieldDecl>,
  pub functions: Vec<FnDecl>,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
  pub visibility: Visibility,
  pub name: Ident,
  pub ty: TypeExpr,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDecl {
  pub id: NodeId,
  /// `extern "C" fn`: callable from C, exported under its name or `export`.
  pub extern_c: bool,
  pub export: Option<Export>,
  pub visibility: Visibility,
  pub name: Ident,
  /// `self` or `var self`, only in struct methods.
  pub self_param: Option<SelfParam>,
  pub params: Vec<Param>,
  pub return_type: Option<TypeExpr>,
  pub body: Block,
  pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfParam {
  pub mutable: bool,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Param {
  pub name: Ident,
  pub ty: TypeExpr,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeExpr {
  pub kind: TypeExprKind,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TypeExprKind {
  /// `()`
  Unit,
  /// A named type; `Self` is a path with the single segment `Self`.
  Path(Path),
  /// `Option<T>`, `Result<T, E>`
  Generic { path: Path, args: Vec<TypeExpr> },
  /// `[T; N]`
  Array { elem: Box<TypeExpr>, len: ArrayLen },
  /// `*const T` or `*mut T`
  Ptr { mutable: bool, pointee: Box<TypeExpr> },
}

/// The length of an array type or repeat expression: an integer literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayLen {
  pub value: u128,
  pub suffix: Option<String>,
  pub span: Span,
}

/// `a::b::c`; the keywords `pkg`, `self`, `super` and `Self` are segments with their own names.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
  /// Identifies the path so later phases can find what a type path names.
  pub id: NodeId,
  pub segments: Vec<Ident>,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
  pub stmts: Vec<Stmt>,
  /// The final expression without `;`, which is the value of the block.
  pub tail: Option<Box<Expr>>,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
  /// `const name: Type = init;` or `var ...`
  Let(LetStmt),
  /// `target = value;`, or `target op= value;` when `op` is set.
  Assign { target: Expr, op: Option<BinaryOp>, value: Expr, span: Span },
  /// `expr;`, or a block-like expression (`if`, `while`, ...) used as a statement.
  Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LetStmt {
  pub id: NodeId,
  /// `var` (true) or `const` (false).
  pub mutable: bool,
  pub name: Ident,
  pub ty: Option<TypeExpr>,
  pub init: Option<Expr>,
  pub span: Span,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BinaryOp {
  Add,
  Sub,
  Mul,
  Div,
  Rem,
  Eq,
  Ne,
  Lt,
  Le,
  Gt,
  Ge,
  And,
  Or,
}

impl BinaryOp {
  pub fn symbol(self) -> &'static str {
    match self {
      BinaryOp::Add => "+",
      BinaryOp::Sub => "-",
      BinaryOp::Mul => "*",
      BinaryOp::Div => "/",
      BinaryOp::Rem => "%",
      BinaryOp::Eq => "==",
      BinaryOp::Ne => "!=",
      BinaryOp::Lt => "<",
      BinaryOp::Le => "<=",
      BinaryOp::Gt => ">",
      BinaryOp::Ge => ">=",
      BinaryOp::And => "&&",
      BinaryOp::Or => "||",
    }
  }

  pub fn is_arithmetic(self) -> bool {
    matches!(self, BinaryOp::Add | BinaryOp::Sub | BinaryOp::Mul | BinaryOp::Div | BinaryOp::Rem)
  }

  pub fn is_comparison(self) -> bool {
    matches!(self, BinaryOp::Eq | BinaryOp::Ne | BinaryOp::Lt | BinaryOp::Le | BinaryOp::Gt | BinaryOp::Ge)
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UnaryOp {
  Neg,
  Not,
  /// `*pointer`, only inside `unsafe`.
  Deref,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
  pub id: NodeId,
  pub kind: ExprKind,
  pub span: Span,
}

impl Expr {
  /// Block-like expressions can be statements without a trailing `;`.
  pub fn is_block_like(&self) -> bool {
    matches!(
      self.kind,
      ExprKind::If { .. }
        | ExprKind::While { .. }
        | ExprKind::Loop { .. }
        | ExprKind::For { .. }
        | ExprKind::Match { .. }
        | ExprKind::Block(_)
        | ExprKind::Unsafe(_)
    )
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ExprKind {
  Int {
    value: u128,
    suffix: Option<String>,
  },
  Float {
    text: String,
    suffix: Option<String>,
  },
  Bool(bool),
  Char(char),
  Str(String),
  Template(Vec<TemplatePart>),
  /// `()`
  Unit,
  Path(Path),
  Call {
    callee: Box<Expr>,
    args: Vec<Expr>,
  },
  /// `receiver.method(args)`
  MethodCall {
    receiver: Box<Expr>,
    method: Ident,
    args: Vec<Expr>,
  },
  /// `base.name`
  Field {
    base: Box<Expr>,
    name: Ident,
  },
  /// `base[index]`
  Index {
    base: Box<Expr>,
    index: Box<Expr>,
  },
  /// `Name { field: value, ... }`; the shorthand `{ x }` is stored as `{ x: x }`.
  StructLit {
    path: Path,
    fields: Vec<FieldInit>,
  },
  /// `[a, b, c]`
  ArrayLit(Vec<Expr>),
  /// `[value; N]`
  ArrayRepeat {
    value: Box<Expr>,
    len: ArrayLen,
  },
  Unary {
    op: UnaryOp,
    operand: Box<Expr>,
  },
  Binary {
    op: BinaryOp,
    lhs: Box<Expr>,
    rhs: Box<Expr>,
  },
  Cast {
    expr: Box<Expr>,
    ty: TypeExpr,
  },
  /// `start..end` or `start..=end`
  Range {
    start: Box<Expr>,
    end: Box<Expr>,
    inclusive: bool,
  },
  /// `expr?`
  Try(Box<Expr>),
  /// `new Type(args)`; in F1 only `new StringBuilder()`.
  New {
    path: Path,
    args: Vec<Expr>,
  },
  Match {
    scrutinee: Box<Expr>,
    arms: Vec<MatchArm>,
  },
  Block(Block),
  /// `unsafe { ... }`
  Unsafe(Block),
  If {
    cond: Box<Expr>,
    then_block: Block,
    /// A `Block` expression or another `If` (`else if`).
    else_branch: Option<Box<Expr>>,
  },
  While {
    cond: Box<Expr>,
    body: Block,
  },
  Loop {
    body: Block,
  },
  /// `for var in iter { body }`; the loop variable binding is identified by the expression's id.
  For {
    var: Ident,
    iter: Box<Expr>,
    body: Block,
  },
  Break {
    value: Option<Box<Expr>>,
  },
  Continue,
  Return {
    value: Option<Box<Expr>>,
  },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
  pub pattern: Pattern,
  /// `if condition` after the pattern.
  pub guard: Option<Expr>,
  pub body: Expr,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pattern {
  pub id: NodeId,
  pub kind: PatternKind,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PatternKind {
  /// `_`
  Wildcard,
  /// A single name: a new binding, or a prelude variant without data (`None`).
  Ident(Ident),
  /// An integer (possibly negated), character, boolean or string literal.
  Literal(Box<Expr>),
  /// A variant without data: `Shape::Empty`.
  Path(Path),
  /// `Shape::Circle(r)`, `Some(x)`
  Tuple { path: Path, elems: Vec<Pattern> },
  /// `Shape::Rect { w, h: 0.0, .. }`; `rest` is set by `..`.
  Named { path: Path, fields: Vec<FieldPattern>, rest: bool },
  /// `a | b`
  Or(Vec<Pattern>),
}

/// `name: pattern`; the shorthand `name` is stored with an `Ident` pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldPattern {
  pub name: Ident,
  pub pattern: Pattern,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldInit {
  pub name: Ident,
  pub value: Expr,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TemplatePart {
  Text(String),
  Expr(Expr),
}
