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
  /// `pub(pkg)`
  Package,
  /// `pub`
  Public,
  /// `prv`: a struct member visible only inside its struct.
  Private,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
  pub file: FileId,
  pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
  Fn(FnDecl),
  Struct(StructDecl),
}

/// `struct Name { fields and functions }`; members keep their source order within each list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDecl {
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
  /// `[T; N]`
  Array { elem: Box<TypeExpr>, len: ArrayLen },
}

/// The length of an array type or repeat expression: an integer literal.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArrayLen {
  pub value: u128,
  pub suffix: Option<String>,
  pub span: Span,
}

/// `a::b::c`; `self` and `Self` are segments named `self` and `Self`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Path {
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
    matches!(self.kind, ExprKind::If { .. } | ExprKind::While { .. } | ExprKind::Loop { .. } | ExprKind::For { .. } | ExprKind::Block(_))
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
  Block(Block),
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
