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

/// Visibility of module-level items (docs/spec/09-modules.md).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Visibility {
  /// No modifier: visible in the declaring module only.
  Module,
  /// `pub(pkg)`
  Package,
  /// `pub`
  Public,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
  pub file: FileId,
  pub items: Vec<Item>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Item {
  Fn(FnDecl),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnDecl {
  pub id: NodeId,
  pub visibility: Visibility,
  pub name: Ident,
  pub params: Vec<Param>,
  pub return_type: Option<TypeExpr>,
  pub body: Block,
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
  Path(Path),
}

/// `a::b::c`
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
  /// `expr;`
  Expr(Expr),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Expr {
  pub id: NodeId,
  pub kind: ExprKind,
  pub span: Span,
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
  /// `()`
  Unit,
  Path(Path),
  Call {
    callee: Box<Expr>,
    args: Vec<Expr>,
  },
}
