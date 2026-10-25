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
  /// `prv`: a member visible only inside its struct, enum or class.
  Private,
}

/// The modules of a program (docs/spec/09-modules.md): the root module (the entry file) first,
/// then every module it reaches through paths.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Package {
  pub modules: Vec<Module>,
  /// Index in `modules` of the prelude written in Sagitario, whose public items every module
  /// sees; it is not part of the module tree.
  pub prelude: Option<usize>,
  /// Indices in `modules` of the standard library modules the program uses, such as `std::io`
  /// with the path `["std", "io"]`; they are not part of the module tree either.
  pub std: Vec<usize>,
  /// A library: it needs no `main`, and a function called `main` is an ordinary one.
  pub library: bool,
  /// The package being compiled first, then its dependencies (docs/nova.md#dependencies), which
  /// `Module::package` indexes. Empty for a program without dependencies.
  pub packages: Vec<PackageInfo>,
  /// The machine the program is compiled for.
  pub target: TargetInfo,
}

/// What the compiler needs to know about the machine a program runs on (docs/nova.md#targets).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TargetInfo {
  /// The width of pointers, `isize` and `usize`: 32 or 64.
  pub pointer_width: u32,
  /// Whether C's `long` has 32 bits: on Windows and on 32-bit targets.
  pub long_is_32: bool,
  /// Whether C's `char` is unsigned: on Linux and Android for ARM.
  pub char_is_unsigned: bool,
}

impl TargetInfo {
  /// The machine running the compiler.
  pub fn host() -> TargetInfo {
    let pointer_width = if cfg!(target_pointer_width = "32") { 32 } else { 64 };

    TargetInfo {
      pointer_width,
      long_is_32: cfg!(windows) || pointer_width == 32,
      char_is_unsigned: cfg!(all(any(target_os = "linux", target_os = "android"), any(target_arch = "aarch64", target_arch = "arm"))),
    }
  }
}

impl Default for TargetInfo {
  fn default() -> TargetInfo {
    TargetInfo::host()
  }
}

/// A package of a program: the one being compiled, or a dependency.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageInfo {
  /// The name its dependents use as a path root; unused for the package being compiled.
  pub name: String,
  /// Indices in `Package::packages` of the packages it depends on directly.
  pub dependencies: Vec<usize>,
}

impl Package {
  /// A program made of one file.
  pub fn single(module: Module) -> Package {
    Package { modules: vec![module], prelude: None, std: Vec::new(), library: false, packages: Vec::new(), target: TargetInfo::host() }
  }
}

/// One source file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Module {
  pub file: FileId,
  /// The logical path from the package root: empty for the root, `["net", "http"]` for
  /// `net/http.sag`.
  pub path: Vec<String>,
  /// Index in `Package::packages` of the package the file belongs to: 0 for the one being
  /// compiled (and for the prelude and the standard library).
  pub package: usize,
  /// The file's path on disk, for `include_str`; `None` for the prelude and the standard
  /// library, which the compiler embeds.
  pub location: Option<String>,
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
  Class(ClassDecl),
  Interface(InterfaceDecl),
  Extern(ExternBlock),
  Const(ConstDecl),
}

/// `const NAME: Type = value;` at module level: a constant computed by the compiler
/// (docs/spec/03-memory.md#module-level-constants).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConstDecl {
  pub visibility: Visibility,
  pub name: Ident,
  pub ty: TypeExpr,
  pub value: Expr,
  pub span: Span,
}

/// `interface Name<T> extends A, B { fields and methods }` (docs/spec/05-interfaces-generics.md).
/// Members are public; a method without body is required, one with a body is a default.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDecl {
  pub visibility: Visibility,
  pub name: Ident,
  pub generics: Vec<GenericParam>,
  pub parents: Vec<TypeExpr>,
  pub fields: Vec<FieldDecl>,
  pub functions: Vec<FnDecl>,
  pub span: Span,
}

/// `abstract class Name extends Parent { fields and functions }` (docs/spec/04-classes.md): a
/// reference type managed by ARC. A function named `new` without `self` is the initializer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDecl {
  pub is_abstract: bool,
  pub visibility: Visibility,
  pub name: Ident,
  pub generics: Vec<GenericParam>,
  /// The class after `extends`.
  pub parent: Option<TypeExpr>,
  /// The interfaces after `impl`.
  pub impls: Vec<TypeExpr>,
  pub fields: Vec<FieldDecl>,
  pub functions: Vec<FnDecl>,
  pub span: Span,
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
  pub generics: Vec<GenericParam>,
  /// The interfaces after `impl`.
  pub impls: Vec<TypeExpr>,
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
  pub generics: Vec<GenericParam>,
  /// The interfaces after `impl`.
  pub impls: Vec<TypeExpr>,
  pub fields: Vec<FieldDecl>,
  pub functions: Vec<FnDecl>,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDecl {
  pub visibility: Visibility,
  /// `const` (false) or `var` (true) before the name: required in classes, rejected in structs.
  pub mutable: Option<bool>,
  /// `weak var`: a class field that does not keep the instance it refers to alive.
  pub weak: bool,
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
  /// `abstract fn` in a class, or an interface method without a default: no body (the body is
  /// empty).
  pub is_abstract: bool,
  /// `override fn`: replaces a method of the parent class.
  pub is_override: bool,
  pub name: Ident,
  pub generics: Vec<GenericParam>,
  /// `self`, only in methods of structs, enums and classes.
  pub self_param: Option<SelfParam>,
  pub params: Vec<Param>,
  pub return_type: Option<TypeExpr>,
  pub body: Block,
  pub span: Span,
}

/// A type parameter `T: Bound1 + Bound2`, with the bounds of its `where` clause too
/// (docs/spec/05-interfaces-generics.md#generics).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericParam {
  pub name: Ident,
  pub bounds: Vec<TypeExpr>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SelfParam {
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
  /// `fn(A, B) -> R`; without `-> R` the result is `()`.
  Fn { params: Vec<TypeExpr>, ret: Option<Box<TypeExpr>> },
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
  /// Type arguments written after one of the segments in an expression: `max<i32>`,
  /// `Pair<i32>::new`, `new Box<i32>(...)`.
  pub type_args: Option<TypeArgs>,
  pub span: Span,
}

/// `<T, U>` after the segment at `segment` of an expression path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeArgs {
  pub segment: usize,
  pub args: Vec<TypeExpr>,
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
  /// `zod`, the parent class's view of `self`: only as the receiver of `zod.method(...)` and
  /// `zod.new(...)`.
  Zod,
  /// `(a, b: T) => body` or `x => body` (docs/spec/08-closures.md).
  Lambda {
    params: Vec<LambdaParam>,
    body: Box<Expr>,
  },
  /// `new Type(args)`: a class instance, or a `StringBuilder`.
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

/// A parameter of a lambda, with an optional type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LambdaParam {
  pub name: Ident,
  pub ty: Option<TypeExpr>,
  pub span: Span,
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
