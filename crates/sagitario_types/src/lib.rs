//! Type checking with local inference (docs/spec/02-types.md, 06-control-flow.md). Records the
//! type of every expression and local binding for lowering; `patterns` checks `match` (with the
//! exhaustiveness algorithm in `exhaustive`), and `flow` then checks definite assignment and
//! reachability.

mod exhaustive;
mod ffi;
mod flow;
mod patterns;
mod text;

use std::collections::HashMap;
use std::fmt;

use sagitario_ast::{
  ArrayLen, BinaryOp, Block, Expr, ExprKind, FieldInit, FnDecl, Ident, NodeId, Package, Path, Stmt, TemplatePart, TypeExpr, TypeExprKind, UnaryOp,
  Visibility,
};
use sagitario_diagnostics::{Code, Diagnostic, Diagnostics, codes};
use sagitario_resolve::{
  Builtin, CType, EnumId, EnumKind, ExternId, FnId, LocalId, LocalKind, Res, Resolution, StructId, TypeItem, TypeRes, VariantRef,
};
use sagitario_span::Span;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IntTy {
  I8,
  I16,
  I32,
  I64,
  Isize,
  U8,
  U16,
  U32,
  U64,
  Usize,
}

impl IntTy {
  const ALL: [IntTy; 10] = [IntTy::I8, IntTy::I16, IntTy::I32, IntTy::I64, IntTy::Isize, IntTy::U8, IntTy::U16, IntTy::U32, IntTy::U64, IntTy::Usize];

  pub fn name(self) -> &'static str {
    match self {
      IntTy::I8 => "i8",
      IntTy::I16 => "i16",
      IntTy::I32 => "i32",
      IntTy::I64 => "i64",
      IntTy::Isize => "isize",
      IntTy::U8 => "u8",
      IntTy::U16 => "u16",
      IntTy::U32 => "u32",
      IntTy::U64 => "u64",
      IntTy::Usize => "usize",
    }
  }

  fn from_name(name: &str) -> Option<IntTy> {
    IntTy::ALL.into_iter().find(|ty| ty.name() == name)
  }

  pub fn is_signed(self) -> bool {
    matches!(self, IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64 | IntTy::Isize)
  }

  /// The largest value of the type. `isize`/`usize` assume 64-bit targets, the only ones the MVP
  /// supports.
  pub fn max(self) -> u128 {
    match self {
      IntTy::I8 => i8::MAX as u128,
      IntTy::I16 => i16::MAX as u128,
      IntTy::I32 => i32::MAX as u128,
      IntTy::I64 | IntTy::Isize => i64::MAX as u128,
      IntTy::U8 => u8::MAX as u128,
      IntTy::U16 => u16::MAX as u128,
      IntTy::U32 => u32::MAX as u128,
      IntTy::U64 | IntTy::Usize => u64::MAX as u128,
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FloatTy {
  F32,
  F64,
}

impl FloatTy {
  pub fn name(self) -> &'static str {
    match self {
      FloatTy::F32 => "f32",
      FloatTy::F64 => "f64",
    }
  }

  fn from_name(name: &str) -> Option<FloatTy> {
    [FloatTy::F32, FloatTy::F64].into_iter().find(|ty| ty.name() == name)
  }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Type {
  Unit,
  Bool,
  Char,
  Int(IntTy),
  Float(FloatTy),
  String,
  /// The prelude's `StringBuilder`, a reference type (a class from F2).
  StringBuilder,
  Struct(StructRef),
  Enum(EnumRef),
  /// `[T; N]`
  Array(Box<Type>, u64),
  /// The prelude's `Option<T>`.
  Option(Box<Type>),
  /// The prelude's `Result<T, E>`.
  Result(Box<Type>, Box<Type>),
  /// `*const T` or `*mut T` (docs/spec/10-ffi.md).
  Ptr {
    mutable: bool,
    pointee: Box<Type>,
  },
  /// `std::ffi::c_void`, only behind a pointer.
  CVoid,
  /// `std::ffi::CString`: an ARC reference to a NUL-terminated copy of a string.
  CString,
  /// The type of expressions that never produce a value (`return`, `break`, ...). Compatible
  /// with every type; programs cannot write it.
  Never,
  /// The type of an expression that already produced an error; compatible with everything so
  /// one mistake is reported once.
  Error,
}

/// A struct type; the name is kept for messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct StructRef {
  pub id: StructId,
  pub name: String,
}

/// An enum type of the program; the name is kept for messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EnumRef {
  pub id: EnumId,
  pub name: String,
}

impl Type {
  fn is_numeric(&self) -> bool {
    matches!(self, Type::Int(_) | Type::Float(_))
  }

  /// Whether a value of type `self` can be used where `expected` is required.
  fn fits(&self, expected: &Type) -> bool {
    // A `*mut T` can be used as a `*const T`.
    let const_pointer =
      matches!((self, expected), (Type::Ptr { mutable: true, pointee }, Type::Ptr { mutable: false, pointee: expected }) if pointee == expected);

    self == expected || const_pointer || matches!(self, Type::Never | Type::Error) || *expected == Type::Error
  }

  /// The enum whose variants make up values of this type.
  pub fn enum_kind(&self) -> Option<EnumKind> {
    match self {
      Type::Enum(ty) => Some(EnumKind::User(ty.id)),
      Type::Option(_) => Some(EnumKind::Option),
      Type::Result(..) => Some(EnumKind::Result),
      _ => None,
    }
  }
}

impl fmt::Display for Type {
  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    match self {
      Type::Unit => f.write_str("()"),
      Type::Bool => f.write_str("bool"),
      Type::Char => f.write_str("char"),
      Type::Int(ty) => f.write_str(ty.name()),
      Type::Float(ty) => f.write_str(ty.name()),
      Type::String => f.write_str("String"),
      Type::Struct(ty) => f.write_str(&ty.name),
      Type::Enum(ty) => f.write_str(&ty.name),
      Type::Array(elem, len) => write!(f, "[{elem}; {len}]"),
      Type::Option(value) => write!(f, "Option<{value}>"),
      Type::Result(value, error) => write!(f, "Result<{value}, {error}>"),
      Type::StringBuilder => f.write_str("StringBuilder"),
      Type::Ptr { mutable, pointee } => write!(f, "*{} {pointee}", if *mutable { "mut" } else { "const" }),
      Type::CVoid => f.write_str("c_void"),
      Type::CString => f.write_str("CString"),
      Type::Never => f.write_str("never"),
      Type::Error => f.write_str("{error}"),
    }
  }
}

/// How a method receives `self`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Receiver {
  /// `self`: a read-only copy.
  Value,
  /// `var self`: the caller's value, which the method may modify.
  Var,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FnSig {
  /// `None` for functions without `self` (module functions and static functions).
  pub receiver: Option<Receiver>,
  /// Without `self`.
  pub params: Vec<Type>,
  pub ret: Type,
}

pub fn builtin_sig(builtin: Builtin) -> FnSig {
  match builtin {
    Builtin::Print | Builtin::Println | Builtin::Eprintln => FnSig { receiver: None, params: vec![Type::String], ret: Type::Unit },
    Builtin::Panic => FnSig { receiver: None, params: vec![Type::String], ret: Type::Never },
    Builtin::CStringNew => FnSig { receiver: None, params: vec![Type::String], ret: Type::Result(Box::new(Type::CString), Box::new(Type::String)) },
    Builtin::StringFromC => FnSig { receiver: None, params: vec![ffi::c_string_ptr()], ret: Type::String },
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
  pub name: String,
  pub ty: Type,
  pub public: bool,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
  pub name: String,
  /// In declaration order.
  pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EnumDef {
  pub name: String,
  pub variants: Vec<VariantDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VariantDef {
  pub name: String,
  /// Whether the fields are named (`Rect { w, h }`) rather than positional (`Circle(r)`).
  pub named: bool,
  /// In declaration order; named fields have a name.
  pub fields: Vec<(Option<String>, Type)>,
}

impl VariantDef {
  fn new(name: &str, fields: Vec<Type>) -> VariantDef {
    VariantDef { name: name.to_string(), named: false, fields: fields.into_iter().map(|ty| (None, ty)).collect() }
  }

  pub fn field_types(&self) -> Vec<Type> {
    self.fields.iter().map(|(_, ty)| ty.clone()).collect()
  }

  pub fn field_index(&self, name: &str) -> Option<usize> {
    self.fields.iter().position(|(field, _)| field.as_deref() == Some(name))
  }
}

/// A method of `Option`, `Result`, `String`, `char` or `StringBuilder`, implemented by the
/// compiler and the runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BuiltinMethod {
  IsSome,
  IsNone,
  IsOk,
  IsErr,
  Unwrap,
  Expect,
  UnwrapOr,
  UnwrapErr,
  /// A runtime function taking the receiver and the arguments in order; `located` functions
  /// also receive the source location, to report panics.
  Runtime {
    function: &'static str,
    located: bool,
  },
  /// `String::find`: an `Option<usize>`.
  Find,
  /// `char::to_digit`: an `Option<u32>`.
  ToDigit,
  /// `String::parse_<type>`: a `Result<type, String>`.
  Parse(Type),
  /// `String::chars` as the iterable of `for`.
  Chars,
  /// `String::bytes` as the iterable of `for`.
  Bytes,
}

#[derive(Debug, Default)]
pub struct TypedModule {
  /// Signature of each function, indexed by `FnId`.
  pub sigs: Vec<FnSig>,
  /// Signature of each C function of the `extern` blocks, indexed by `ExternId`.
  pub extern_sigs: Vec<FnSig>,
  /// Indexed by `StructId`.
  pub structs: Vec<StructDef>,
  /// Indexed by `EnumId`.
  pub enums: Vec<EnumDef>,
  /// Calls to methods of `Option` and `Result`, by node id.
  pub builtin_methods: HashMap<NodeId, BuiltinMethod>,
  /// Type of each local binding, indexed by `FnId` then `LocalId`.
  pub local_types: Vec<Vec<Type>>,
  pub expr_types: HashMap<NodeId, Type>,
  /// The function called by each method call expression; absent for built-in methods such as
  /// the `len()` of arrays.
  pub method_calls: HashMap<NodeId, FnId>,
}

impl TypedModule {
  pub fn sig(&self, id: FnId) -> Option<&FnSig> {
    self.sigs.get(id.0 as usize)
  }

  pub fn extern_sig(&self, id: ExternId) -> Option<&FnSig> {
    self.extern_sigs.get(id.0 as usize)
  }

  pub fn struct_def(&self, id: StructId) -> Option<&StructDef> {
    self.structs.get(id.0 as usize)
  }

  /// Position of a field in its struct's declaration.
  pub fn field_index(&self, id: StructId, name: &str) -> Option<usize> {
    self.struct_def(id)?.fields.iter().position(|field| field.name == name)
  }

  /// The variants of an enum, `Option` or `Result` type, with their field types.
  pub fn variants(&self, ty: &Type) -> Option<Vec<VariantDef>> {
    match ty {
      Type::Enum(ty) => self.enums.get(ty.id.0 as usize).map(|def| def.variants.clone()),
      Type::Option(value) => Some(vec![VariantDef::new("None", Vec::new()), VariantDef::new("Some", vec![value.as_ref().clone()])]),
      Type::Result(value, error) => {
        Some(vec![VariantDef::new("Ok", vec![value.as_ref().clone()]), VariantDef::new("Err", vec![error.as_ref().clone()])])
      }
      _ => None,
    }
  }

  pub fn local_type(&self, function: FnId, local: LocalId) -> Option<&Type> {
    self.local_types.get(function.0 as usize)?.get(local.0 as usize)
  }

  pub fn expr_type(&self, expr: &Expr) -> Option<&Type> {
    self.expr_types.get(&expr.id)
  }
}

pub fn check(package: &Package, resolution: &Resolution, diagnostics: &mut Diagnostics) -> TypedModule {
  let mut checker = Checker {
    package,
    resolution,
    diagnostics,
    typed: TypedModule::default(),
    locals: Vec::new(),
    ret: Type::Unit,
    ret_span: None,
    function: FnId(0),
    owner: None,
    loops: Vec::new(),
    unsafe_blocks: Vec::new(),
    pointees: Vec::new(),
  };

  checker.struct_defs();
  checker.enum_defs();
  checker.check_recursive_types();
  checker.check_repr_c_structs();
  checker.extern_sigs();

  let decls = resolution.fn_decls(package);

  for (id, decl) in &decls {
    checker.owner = resolution.owner(*id);

    let sig = checker.signature(decl);

    if decl.extern_c {
      checker.check_exported(decl, &sig);
    }

    checker.typed.sigs.push(sig);
  }

  checker.check_symbols();

  if let Some((id, main)) = decls.iter().find(|(id, _)| Some(*id) == resolution.main) {
    checker.check_main(*id, main);
  }

  for (id, decl) in &decls {
    checker.function(*id, decl);
  }

  checker.check_pointees();

  let typed = checker.typed;

  for (id, decl) in &decls {
    flow::check_function(decl, resolution.locals(*id), resolution, &typed, diagnostics);
  }

  typed
}

/// What a place expression (a binding, or a field or element of one) is used for.
#[derive(Clone, Copy)]
enum PlaceUse<'a> {
  Assign,
  /// The receiver of the named `var self` method.
  Receiver(&'a str),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum LoopKind {
  Loop,
  While,
  For,
}

struct LoopCx {
  kind: LoopKind,
  /// The expected type of the `loop` expression, used to type `break` values.
  expected: Option<Type>,
  break_ty: Option<Type>,
  has_break: bool,
}

struct Checker<'a> {
  package: &'a Package,
  resolution: &'a Resolution,
  diagnostics: &'a mut Diagnostics,
  typed: TypedModule,
  /// Types of the current function's locals; `None` until a declaration without type or value
  /// is first assigned.
  locals: Vec<Option<Type>>,
  ret: Type,
  /// The current function's return type annotation, for "expected because of" labels.
  ret_span: Option<Span>,
  function: FnId,
  /// The type whose function is being checked, for `Self` and `prv` members.
  owner: Option<TypeItem>,
  loops: Vec<LoopCx>,
  /// The enclosing `unsafe` blocks, innermost last, with whether each one was needed.
  unsafe_blocks: Vec<bool>,
  /// Pointee types, checked once every type is known.
  pointees: Vec<(Span, Type)>,
}

impl Checker<'_> {
  fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
    self.diagnostics.push(Diagnostic::error(code, span, message));
  }

  fn struct_ref(&self, id: StructId) -> Type {
    let name = self.resolution.struct_decl(self.package, id).map_or_else(String::new, |decl| decl.name.name.clone());

    Type::Struct(StructRef { id, name })
  }

  fn type_ref(&self, item: TypeItem) -> Type {
    match item {
      TypeItem::Struct(id) => self.struct_ref(id),
      TypeItem::Enum(id) => Type::Enum(EnumRef { id, name: self.resolution.type_name(self.package, item) }),
    }
  }

  /// Resolves the field types of every enum variant.
  fn enum_defs(&mut self) {
    for index in 0..self.resolution.enums.len() {
      let id = EnumId(index as u32);
      let Some(decl) = self.resolution.enum_decl(self.package, id) else { continue };

      self.owner = Some(TypeItem::Enum(id));

      let mut variants = Vec::new();

      for variant in &decl.variants {
        let (named, fields) = match &variant.fields {
          sagitario_ast::VariantFields::Unit => (false, Vec::new()),
          sagitario_ast::VariantFields::Tuple(types) => (false, types.iter().map(|ty| (None, self.type_expr(ty))).collect()),
          sagitario_ast::VariantFields::Named(fields) => {
            (true, fields.iter().map(|field| (Some(field.name.name.clone()), self.type_expr(&field.ty))).collect())
          }
        };

        variants.push(VariantDef { name: variant.name.name.clone(), named, fields });
      }

      self.typed.enums.push(EnumDef { name: decl.name.name.clone(), variants });
    }

    self.owner = None;
  }

  /// Resolves the field types of every struct.
  fn struct_defs(&mut self) {
    for index in 0..self.resolution.structs.len() {
      let id = StructId(index as u32);
      let Some(decl) = self.resolution.struct_decl(self.package, id) else { continue };

      self.owner = Some(TypeItem::Struct(id));

      let fields = decl
        .fields
        .iter()
        .map(|field| FieldDef {
          name: field.name.name.clone(),
          ty: self.type_expr(&field.ty),
          public: field.visibility != Visibility::Private,
          span: field.span,
        })
        .collect();

      self.typed.structs.push(StructDef { name: decl.name.name.clone(), fields });
    }

    self.owner = None;
  }

  /// A struct or enum cannot contain itself by value, directly or through other structs, enums,
  /// arrays, `Option` or `Result`.
  fn check_recursive_types(&mut self) {
    for index in 0..self.typed.structs.len() {
      let id = StructId(index as u32);
      let fields = self.typed.struct_def(id).map(|def| def.fields.clone()).unwrap_or_default();
      let recursive = fields.iter().find(|field| self.contains_type(&field.ty, TypeItem::Struct(id), &mut Vec::new()));

      if let (Some(field), Some(decl)) = (recursive, self.resolution.struct_decl(self.package, id)) {
        let (name, span) = (decl.name.name.clone(), decl.name.span);

        self.recursive_type_error("struct", &name, span, field.span);
      }
    }

    for index in 0..self.typed.enums.len() {
      let id = EnumId(index as u32);
      let Some(decl) = self.resolution.enum_decl(self.package, id) else { continue };
      let types = self.typed.enums.get(index).map(|def| def.variants.iter().map(VariantDef::field_types).collect::<Vec<_>>()).unwrap_or_default();
      let recursive = types.iter().position(|fields| fields.iter().any(|ty| self.contains_type(ty, TypeItem::Enum(id), &mut Vec::new())));

      if let Some(variant) = recursive.and_then(|index| decl.variants.get(index)) {
        self.recursive_type_error("enum", &decl.name.name, decl.name.span, variant.span);
      }
    }
  }

  fn recursive_type_error(&mut self, kind: &str, name: &str, span: Span, part: Span) {
    self.diagnostics.push(
      Diagnostic::error(codes::RECURSIVE_TYPE, span, format!("the {kind} `{name}` contains itself, so its size would be infinite"))
        .with_label(part, format!("this contains a `{name}`"))
        .with_help(format!("{kind}s are stored inline; keep the recursive value in a `class` (F2), which is a reference")),
    );
  }

  fn contains_type(&self, ty: &Type, target: TypeItem, visited: &mut Vec<TypeItem>) -> bool {
    let item = match ty {
      Type::Array(elem, _) | Type::Option(elem) => return self.contains_type(elem, target, visited),
      Type::Result(value, error) => return self.contains_type(value, target, visited) || self.contains_type(error, target, visited),
      Type::Struct(inner) => TypeItem::Struct(inner.id),
      Type::Enum(inner) => TypeItem::Enum(inner.id),
      _ => return false,
    };

    if item == target {
      return true;
    }

    if visited.contains(&item) {
      return false;
    }

    visited.push(item);

    let parts: Vec<Type> = match item {
      TypeItem::Struct(id) => self.typed.struct_def(id).map(|def| def.fields.iter().map(|field| field.ty.clone()).collect()).unwrap_or_default(),
      TypeItem::Enum(_) => self.typed.variants(ty).unwrap_or_default().iter().flat_map(VariantDef::field_types).collect(),
    };

    parts.iter().any(|part| self.contains_type(part, target, visited))
  }

  fn signature(&mut self, decl: &FnDecl) -> FnSig {
    let receiver = decl.self_param.map(|param| if param.mutable { Receiver::Var } else { Receiver::Value });
    let params = decl.params.iter().map(|param| self.type_expr(&param.ty)).collect();
    let ret = decl.return_type.as_ref().map_or(Type::Unit, |ty| self.type_expr(ty));

    FnSig { receiver, params, ret }
  }

  /// `fn main()` or `fn main() -> Result<(), String>`, with any visibility (docs/spec/07-errors.md).
  fn check_main(&mut self, id: FnId, main: &FnDecl) {
    let ret = self.typed.sig(id).map_or(Type::Unit, |sig| sig.ret.clone());
    let valid_ret = matches!(&ret, Type::Unit | Type::Error) || ret == Type::Result(Box::new(Type::Unit), Box::new(Type::String));

    if !main.params.is_empty() || !valid_ret {
      let mut diagnostic =
        Diagnostic::error(codes::INVALID_MAIN, main.name.span, "`main` must be declared as `fn main()` or `fn main() -> Result<(), String>`");

      if let Type::Result(..) = ret {
        diagnostic = diagnostic.with_note("in F1 the error type of `main` must be `String`");
      }

      self.diagnostics.push(diagnostic);
    }
  }

  fn function(&mut self, id: FnId, decl: &FnDecl) {
    let Some(sig) = self.typed.sig(id).cloned() else { return };
    let local_count = self.resolution.locals(id).len();

    self.function = id;
    self.owner = self.resolution.owner(id);
    self.ret = sig.ret.clone();
    self.ret_span = decl.return_type.as_ref().map(|ty| ty.span);
    self.locals = vec![None; local_count];

    // `self` is the first local of a method, followed by the parameters.
    let self_ty = self.owner.filter(|_| sig.receiver.is_some()).map(|owner| self.type_ref(owner));
    let param_types = self_ty.into_iter().chain(sig.params.iter().cloned());

    for (index, ty) in param_types.enumerate() {
      if let Some(slot) = self.locals.get_mut(index) {
        *slot = Some(ty);
      }
    }

    let body_ty = self.block(&decl.body, Some(&sig.ret));

    if !body_ty.fits(&sig.ret) {
      let span = decl.body.tail.as_ref().map_or(decl.body.span, |tail| tail.span);

      let mut diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, span, format!("expected `{}`, found `{body_ty}`", sig.ret))
        .with_note(format!("the function `{}` must return a value of type `{}` on every path", decl.name.name, sig.ret));

      if let Some(ret_span) = self.ret_span {
        diagnostic = diagnostic.with_label(ret_span, "expected because of the return type");
      }

      self.diagnostics.push(diagnostic);
    }

    let locals = std::mem::take(&mut self.locals);
    let infos = self.resolution.locals(id);
    let mut types = Vec::with_capacity(locals.len());

    for (index, ty) in locals.into_iter().enumerate() {
      let ty = match ty {
        Some(ty) => ty,
        None => {
          if let Some(info) = infos.get(index) {
            self.error(codes::UNKNOWN_TYPE, info.span, format!("cannot infer the type of `{}`; add a type annotation", info.name));
          }

          Type::Error
        }
      };

      types.push(ty);
    }

    self.typed.local_types.push(types);
  }

  fn type_expr(&mut self, ty: &TypeExpr) -> Type {
    self.type_expr_at(ty, false)
  }

  /// A type, possibly behind a pointer, the only place `c_void` can appear.
  fn type_expr_at(&mut self, ty: &TypeExpr, behind_pointer: bool) -> Type {
    match &ty.kind {
      TypeExprKind::Ptr { mutable, pointee } => self.pointer_type(*mutable, pointee),
      TypeExprKind::Unit => Type::Unit,
      TypeExprKind::Array { elem, len } => {
        let elem = self.type_expr(elem);

        match self.array_len(len) {
          Some(len) => Type::Array(Box::new(elem), len),
          None => Type::Error,
        }
      }
      TypeExprKind::Generic { path, args } => {
        let name = path.segments.last().map_or("", |segment| segment.name.as_str());
        let arity = match name {
          _ if self.resolution.type_paths.get(&path.id) == Some(&TypeRes::Error) => return Type::Error,
          _ if path.segments.len() != 1 || !matches!(self.resolution.type_paths.get(&path.id), Some(TypeRes::Builtin) | None) => None,
          "Option" => Some(1),
          "Result" => Some(2),
          _ => None,
        };
        let Some(arity) = arity else {
          self.error(codes::NOT_IMPLEMENTED, ty.span, "generic types other than `Option` and `Result` are not implemented yet (F2)");

          return Type::Error;
        };

        if args.len() != arity {
          self.error(
            codes::WRONG_TYPE_ARGUMENTS,
            ty.span,
            format!("`{name}` takes {arity} type argument{} but {} were supplied", plural(arity), args.len()),
          );

          return Type::Error;
        }

        let mut types: Vec<Type> = args.iter().map(|arg| self.type_expr(arg)).collect();
        let first = Box::new(if types.is_empty() { Type::Error } else { types.remove(0) });

        match types.pop() {
          Some(error) => Type::Result(first, Box::new(error)),
          None => Type::Option(first),
        }
      }
      TypeExprKind::Path(path) => {
        // The resolver reports unknown types and resolves program types and module paths.
        match self.resolution.type_paths.get(&path.id) {
          Some(TypeRes::Item(item)) => return self.type_ref(*item),
          Some(TypeRes::CType(CType::Void)) if !behind_pointer => return self.bare_c_void(path.span),
          Some(TypeRes::CType(c)) => return ffi::c_type(*c),
          Some(TypeRes::CString) => return Type::CString,
          Some(TypeRes::Error) => return Type::Error,
          _ => {}
        }

        let [segment] = path.segments.as_slice() else {
          self.error(codes::UNRESOLVED_NAME, path.span, "cannot find this type");

          return Type::Error;
        };
        let name = segment.name.as_str();

        if let Some(int) = IntTy::from_name(name) {
          return Type::Int(int);
        }

        if let Some(float) = FloatTy::from_name(name) {
          return Type::Float(float);
        }

        if name == "Self" {
          return match self.owner {
            Some(owner) => self.type_ref(owner),
            None => {
              self.error(codes::UNRESOLVED_NAME, segment.span, "`Self` is only available inside a struct or enum");

              Type::Error
            }
          };
        }

        match name {
          "bool" => Type::Bool,
          "char" => Type::Char,
          "String" => Type::String,
          "StringBuilder" => Type::StringBuilder,
          "Option" | "Result" => {
            let usage = if name == "Option" { "Option<T>" } else { "Result<T, E>" };

            self.error(codes::WRONG_TYPE_ARGUMENTS, segment.span, format!("`{name}` needs type arguments: `{usage}`"));

            Type::Error
          }
          _ => {
            let primitives =
              IntTy::ALL.iter().map(|ty| ty.name()).chain(["f32", "f64", "bool", "char", "String", "StringBuilder", "Option", "Result"]);
            let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("cannot find type `{name}`"));

            if let Some(similar) = sagitario_diagnostics::closest_match(name, primitives) {
              diagnostic = diagnostic.with_help(format!("a type with a similar name exists: `{similar}`"));
            }

            self.diagnostics.push(diagnostic);

            Type::Error
          }
        }
      }
    }
  }

  /// The length of an array type or repeat expression: a `usize` literal.
  fn array_len(&mut self, len: &ArrayLen) -> Option<u64> {
    if let Some(suffix) = len.suffix.as_deref().filter(|suffix| *suffix != "usize") {
      self.error(codes::MISMATCHED_TYPES, len.span, format!("expected `usize`, found `{suffix}`: array lengths are `usize`"));

      return None;
    }

    match u64::try_from(len.value) {
      Ok(value) => Some(value),
      Err(_) => {
        self.error(codes::INTEGER_OUT_OF_RANGE, len.span, format!("the array length `{}` does not fit in `usize`", len.value));

        None
      }
    }
  }

  fn local(&self, expr: &Expr) -> Option<LocalId> {
    match self.resolution.paths.get(&expr.id) {
      Some(Res::Local(id)) => Some(*id),
      _ => None,
    }
  }

  fn set_local(&mut self, id: LocalId, ty: Type) {
    if let Some(slot) = self.locals.get_mut(id.0 as usize) {
      *slot = Some(ty);
    }
  }

  /// Checks a block; its type is the tail's type, `never` if it diverges, or `()`.
  fn block(&mut self, block: &Block, expected: Option<&Type>) -> Type {
    let mut diverges = false;

    for stmt in &block.stmts {
      if self.stmt(stmt) == Type::Never {
        diverges = true;
      }
    }

    match &block.tail {
      Some(tail) => {
        let ty = self.expr(tail, expected);

        if diverges { Type::Never } else { ty }
      }
      None if diverges => Type::Never,
      None => Type::Unit,
    }
  }

  /// Checks a statement and returns `never` if it diverges.
  fn stmt(&mut self, stmt: &Stmt) -> Type {
    match stmt {
      Stmt::Let(stmt) => {
        let declared = stmt.ty.as_ref().map(|ty| self.type_expr(ty));
        let Some(id) = self.resolution.bindings.get(&stmt.id).copied() else { return Type::Error };
        let init_ty = stmt.init.as_ref().map(|init| match &declared {
          Some(declared) => {
            let because = stmt.ty.as_ref().map(|ty| (ty.span, "expected because of this type"));

            self.expect_because(init, declared, because);

            declared.clone()
          }
          None => self.expr(init, None),
        });
        let local_ty = match (&declared, &init_ty) {
          (Some(declared), _) => Some(declared.clone()),
          (None, Some(Type::Never)) => Some(Type::Error),
          (None, Some(ty)) => Some(ty.clone()),
          (None, None) => None,
        };

        if let Some(ty) = local_ty {
          self.set_local(id, ty);
        }

        if init_ty == Some(Type::Never) { Type::Never } else { Type::Unit }
      }
      Stmt::Assign { target, op, value, .. } => self.assign(target, *op, value),
      Stmt::Expr(expr) => self.expr(expr, None),
    }
  }

  fn assign(&mut self, target: &Expr, op: Option<BinaryOp>, value: &Expr) -> Type {
    if matches!(target.kind, ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::Unary { op: UnaryOp::Deref, .. }) {
      let target_ty = self.expr(target, None);

      self.check_mutable_place(target, PlaceUse::Assign);

      if let Some(op) = op {
        self.check_compound_operand(target, op, &target_ty);
      }

      self.expect(value, &target_ty);

      return if self.typed.expr_types.get(&value.id) == Some(&Type::Never) { Type::Never } else { Type::Unit };
    }

    let target_local = match &target.kind {
      ExprKind::Path(_) => self.local(target),
      _ => None,
    };
    let Some(id) = target_local else {
      if matches!(target.kind, ExprKind::Path(_)) && !self.resolution.paths.contains_key(&target.id) {
        // Unresolved name: already reported.
      } else {
        self.error(codes::INVALID_ASSIGNMENT_TARGET, target.span, "only bindings, fields and array elements can be assigned");
      }

      return self.expr(value, None);
    };
    let current = self.locals.get(id.0 as usize).cloned().flatten();

    self.typed.expr_types.insert(target.id, current.clone().unwrap_or(Type::Error));

    let value_ty = match (&current, op) {
      (Some(ty), None) => {
        self.expect(value, ty);

        ty.clone()
      }
      (Some(ty), Some(op)) => {
        self.check_compound_operand(target, op, ty);
        self.expect(value, ty);

        ty.clone()
      }
      (None, None) => {
        let ty = self.expr(value, None);

        if ty != Type::Never {
          self.set_local(id, ty.clone());
        }

        ty
      }
      (None, Some(_)) => {
        self.expr(value, None);

        Type::Error
      }
    };

    if value_ty == Type::Never { Type::Never } else { Type::Unit }
  }

  fn check_compound_operand(&mut self, target: &Expr, op: BinaryOp, ty: &Type) {
    let concatenation = *ty == Type::String && op == BinaryOp::Add;

    if !concatenation && !ty.is_numeric() && *ty != Type::Error {
      self.error(codes::INVALID_OPERAND, target.span, format!("cannot apply `{}=` to a value of type `{ty}`", op.symbol()));
    }
  }

  /// The binding a place expression (`p`, `p.x`, `a[i].y`) belongs to; `None` for temporaries.
  fn place_root(&self, expr: &Expr) -> Option<LocalId> {
    match &expr.kind {
      ExprKind::Path(_) => self.local(expr),
      ExprKind::Field { base, .. } | ExprKind::Index { base, .. } => self.place_root(base),
      _ => None,
    }
  }

  /// Fields and elements can only be modified through `var` bindings and `var self`.
  fn check_mutable_place(&mut self, place: &Expr, place_use: PlaceUse) {
    let code = match place_use {
      PlaceUse::Assign => codes::IMMUTABLE_PLACE,
      PlaceUse::Receiver(_) => codes::IMMUTABLE_RECEIVER,
    };

    if let Some(pointer) = self.deref_pointer(place) {
      self.check_pointer_write(place, pointer);

      return;
    }

    let Some(root) = self.place_root(place) else {
      if matches!(place.kind, ExprKind::Path(_)) {
        // Not a binding (an unresolved name or a function): already reported or rejected.
        return;
      }

      let (code, message) = match place_use {
        PlaceUse::Assign => (codes::INVALID_ASSIGNMENT_TARGET, "cannot assign to a part of a temporary value".to_string()),
        PlaceUse::Receiver(method) => (code, format!("cannot call the `var self` method `{method}` on a temporary value")),
      };

      self.diagnostics.push(Diagnostic::error(code, place.span, message).with_help("store the value in a `var` binding first"));

      return;
    };
    let Some(info) = self.resolution.locals(self.function).get(root.0 as usize).cloned() else { return };
    let name = &info.name;
    let action = match place_use {
      PlaceUse::Assign => format!("cannot modify `{name}`"),
      PlaceUse::Receiver(method) => format!("cannot call the `var self` method `{method}` on `{name}`"),
    };
    let (reason, label, help) = match info.kind {
      LocalKind::Var | LocalKind::VarSelf => return,
      LocalKind::Const => ("it is not declared with `var`", "declared with `const` here", format!("declare `{name}` with `var` to modify it")),
      LocalKind::Param => ("parameters are immutable", "parameter declared here", format!("copy it into a `var` binding: `var {name} = {name};`")),
      LocalKind::SelfParam => ("`self` is immutable in this method", "declared as `self` here", "declare the method with `var self`".to_string()),
      LocalKind::ForVar => {
        ("loop variables are immutable", "loop variable declared here", format!("copy it into a `var` binding: `var {name} = {name};`"))
      }
      LocalKind::Pattern => {
        ("pattern bindings are immutable", "bound by the pattern here", format!("copy it into a `var` binding: `var {name} = {name};`"))
      }
    };

    self.diagnostics.push(Diagnostic::error(code, place.span, format!("{action}: {reason}")).with_label(info.span, label).with_help(help));
  }

  fn expect(&mut self, expr: &Expr, expected: &Type) {
    self.expect_because(expr, expected, None);
  }

  /// Like `expect`, pointing at what imposes the expected type when it is a mismatch.
  fn expect_because(&mut self, expr: &Expr, expected: &Type, because: Option<(Span, &str)>) {
    let actual = self.expr(expr, Some(expected));

    if actual.fits(expected) {
      return;
    }

    let mut diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, expr.span, format!("expected `{expected}`, found `{actual}`"));

    if let Some((span, message)) = because {
      diagnostic = diagnostic.with_label(span, message);
    }

    self.diagnostics.push(diagnostic);
  }

  fn mismatch(&mut self, span: Span, expected: &Type, actual: &Type) {
    self.error(codes::MISMATCHED_TYPES, span, format!("expected `{expected}`, found `{actual}`"));
  }

  /// Checks an expression, using `expected` to give unsuffixed literals their type.
  fn expr(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
    let ty = self.expr_kind(expr, expected);

    self.typed.expr_types.insert(expr.id, ty.clone());

    ty
  }

  fn expr_kind(&mut self, expr: &Expr, expected: Option<&Type>) -> Type {
    match &expr.kind {
      ExprKind::Int { value, suffix } => self.int_literal(expr.span, *value, false, suffix.as_deref(), expected),
      ExprKind::Float { suffix, .. } => float_literal_type(suffix.as_deref(), expected),
      ExprKind::Bool(_) => Type::Bool,
      ExprKind::Char(_) => Type::Char,
      ExprKind::Str(_) => Type::String,
      ExprKind::Unit => Type::Unit,
      ExprKind::Template(parts) => {
        for part in parts {
          if let TemplatePart::Expr(part) = part {
            let ty = self.expr(part, None);

            if !matches!(ty, Type::Int(_) | Type::Float(_) | Type::Bool | Type::Char | Type::String | Type::Never | Type::Error) {
              self.diagnostics.push(
                Diagnostic::error(codes::NOT_INTERPOLABLE, part.span, format!("a value of type `{ty}` cannot be interpolated"))
                  .with_note("integers, floats, `bool`, `char` and `String` can be interpolated; interpolate the fields instead"),
              );
            }
          }
        }

        Type::String
      }
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        // An unassigned binding without a known type is reported by the flow analysis.
        Some(Res::Local(id)) => self.locals.get(id.0 as usize).cloned().flatten().unwrap_or(Type::Error),
        Some(Res::Fn(_) | Res::Extern(_) | Res::Builtin(_)) => {
          self.error(codes::NOT_IMPLEMENTED, expr.span, "functions used as values are not implemented yet");

          Type::Error
        }
        Some(Res::Variant(variant)) => self.variant_value(expr.span, *variant, None, expected),
        Some(Res::Struct(_)) | None => Type::Error,
      },
      ExprKind::Call { callee, args } => match self.resolution.paths.get(&callee.id).copied() {
        Some(Res::Variant(variant)) if matches!(callee.kind, ExprKind::Path(_)) => {
          let ty = self.variant_value(expr.span, variant, Some(args), expected);

          self.typed.expr_types.insert(callee.id, ty.clone());

          ty
        }
        _ => self.call(expr, callee, args),
      },
      ExprKind::Try(inner) => self.try_expr(inner),
      ExprKind::New { path, args } => self.new_expr(path, args),
      ExprKind::Match { scrutinee, arms } => self.match_expr(scrutinee, arms, expected),
      ExprKind::MethodCall { receiver, method, args } => self.method_call(expr, receiver, method, args),
      ExprKind::Field { base, name } => self.field(base, name),
      ExprKind::Index { base, index } => self.index(base, index),
      ExprKind::StructLit { path, fields } => self.struct_literal(expr, path, fields),
      ExprKind::ArrayLit(elems) => self.array_literal(expr.span, elems, expected),
      ExprKind::ArrayRepeat { value, len } => {
        let elem = match expected {
          Some(Type::Array(elem, _)) => Some(elem.as_ref().clone()),
          _ => None,
        };
        let value_ty = self.expr(value, elem.as_ref());

        match self.array_len(len) {
          Some(len) => Type::Array(Box::new(value_ty), len),
          None => Type::Error,
        }
      }
      ExprKind::Unary { op, operand } => self.unary(expr, *op, operand, expected),
      ExprKind::Binary { op, lhs, rhs } => self.binary(*op, lhs, rhs, expected),
      ExprKind::Cast { expr: inner, ty } => self.cast(inner, ty),
      ExprKind::Range { .. } => {
        self.error(codes::NOT_IMPLEMENTED, expr.span, "ranges outside of `for` loops are not implemented yet");

        Type::Error
      }
      ExprKind::Block(block) => self.block(block, expected),
      ExprKind::Unsafe(block) => self.unsafe_block(expr, block, expected),
      ExprKind::If { cond, then_block, else_branch } => self.if_expr(cond, then_block, else_branch.as_deref(), expected),
      ExprKind::While { cond, body } => {
        self.expect(cond, &Type::Bool);
        self.loop_body(LoopKind::While, None, body);

        Type::Unit
      }
      ExprKind::Loop { body } => {
        let cx = self.loop_body(LoopKind::Loop, expected.cloned(), body);

        if cx.has_break { cx.break_ty.unwrap_or(Type::Unit) } else { Type::Never }
      }
      ExprKind::For { iter, body, .. } => {
        let var_ty = self.for_iter(iter);

        if let Some(id) = self.resolution.bindings.get(&expr.id).copied() {
          self.set_local(id, var_ty);
        }

        self.loop_body(LoopKind::For, None, body);

        Type::Unit
      }
      ExprKind::Break { value } => {
        self.break_expr(expr.span, value.as_deref());

        Type::Never
      }
      ExprKind::Continue => {
        if self.loops.is_empty() {
          self.error(codes::BREAK_OUTSIDE_LOOP, expr.span, "`continue` outside of a loop");
        }

        Type::Never
      }
      ExprKind::Return { value } => {
        let ret = self.ret.clone();

        match value {
          Some(value) => {
            let because = self.ret_span.map(|span| (span, "expected because of the return type"));

            self.expect_because(value, &ret, because);
          }
          None if !Type::Unit.fits(&ret) => {
            self.error(codes::MISMATCHED_TYPES, expr.span, format!("expected `{ret}`, found `()`: `return` needs a value"));
          }
          None => {}
        }

        Type::Never
      }
    }
  }

  fn int_literal(&mut self, span: Span, value: u128, negative: bool, suffix: Option<&str>, expected: Option<&Type>) -> Type {
    let int = match (suffix.and_then(IntTy::from_name), expected) {
      (Some(int), _) => int,
      (None, Some(Type::Int(int))) => *int,
      (None, _) => IntTy::I32,
    };

    if negative && !int.is_signed() {
      self.error(codes::INVALID_OPERAND, span, format!("cannot negate a value of the unsigned type `{}`", int.name()));

      return Type::Int(int);
    }

    // The most negative value has one more unit of magnitude than the maximum.
    let limit = if negative { int.max() + 1 } else { int.max() };

    if value > limit {
      let shown = if negative { format!("-{value}") } else { value.to_string() };

      self.diagnostics.push(
        Diagnostic::error(codes::INTEGER_OUT_OF_RANGE, span, format!("the literal `{shown}` does not fit in `{}`", int.name())).with_note(format!(
          "the maximum value of `{}` is `{}`",
          int.name(),
          int.max()
        )),
      );
    }

    Type::Int(int)
  }

  fn unary(&mut self, expr: &Expr, op: UnaryOp, operand: &Expr, expected: Option<&Type>) -> Type {
    if op == UnaryOp::Neg {
      // `-128i8` is a single literal value, so its range is checked including the sign.
      if let ExprKind::Int { value, suffix } = &operand.kind {
        let ty = self.int_literal(expr.span, *value, true, suffix.as_deref(), expected);

        self.typed.expr_types.insert(operand.id, ty.clone());

        return ty;
      }
    }

    match op {
      UnaryOp::Deref => self.deref(expr, operand),
      UnaryOp::Not => {
        self.expect(operand, &Type::Bool);

        Type::Bool
      }
      UnaryOp::Neg => {
        let ty = self.expr(operand, expected.filter(|ty| ty.is_numeric()));

        match &ty {
          Type::Int(int) if !int.is_signed() => {
            self.error(codes::INVALID_OPERAND, expr.span, format!("cannot negate a value of the unsigned type `{ty}`"));

            Type::Error
          }
          Type::Int(_) | Type::Float(_) | Type::Error | Type::Never => ty,
          _ => {
            self.error(codes::INVALID_OPERAND, expr.span, format!("cannot negate a value of type `{ty}`"));

            Type::Error
          }
        }
      }
    }
  }

  /// Checks two operands that must share a type. An unsuffixed literal on the left takes its
  /// type from the right operand, so both `x + 1` and `1 + x` work for any integer `x`.
  fn same_type_operands(&mut self, lhs: &Expr, rhs: &Expr, hint: Option<&Type>) -> Type {
    let (lhs_ty, rhs_ty, mismatch_span) = if is_untyped_literal(lhs) && !is_untyped_literal(rhs) {
      let rhs_ty = self.expr(rhs, hint);
      let lhs_ty = self.expr(lhs, Some(&rhs_ty));

      (lhs_ty, rhs_ty, lhs.span)
    } else {
      let lhs_ty = self.expr(lhs, hint);
      let rhs_ty = self.expr(rhs, Some(&lhs_ty));

      (lhs_ty, rhs_ty, rhs.span)
    };

    match (&lhs_ty, &rhs_ty) {
      (Type::Error, _) | (_, Type::Error) => Type::Error,
      (Type::Never, other) | (other, Type::Never) => other.clone(),
      _ if lhs_ty == rhs_ty => lhs_ty,
      _ => {
        let (expected, found) = if mismatch_span == rhs.span { (&lhs_ty, &rhs_ty) } else { (&rhs_ty, &lhs_ty) };

        self.mismatch(mismatch_span, expected, found);

        Type::Error
      }
    }
  }

  fn binary(&mut self, op: BinaryOp, lhs: &Expr, rhs: &Expr, expected: Option<&Type>) -> Type {
    if matches!(op, BinaryOp::And | BinaryOp::Or) {
      self.expect(lhs, &Type::Bool);
      self.expect(rhs, &Type::Bool);

      return Type::Bool;
    }

    let hint = if op.is_arithmetic() { expected.filter(|ty| ty.is_numeric()) } else { None };
    let ty = self.same_type_operands(lhs, rhs, hint);
    let span = lhs.span.to(rhs.span);

    if op.is_arithmetic() {
      return match ty {
        Type::Int(_) | Type::Float(_) | Type::Error | Type::Never => ty,
        Type::String if op == BinaryOp::Add => Type::String,
        other => {
          self.error(codes::INVALID_OPERAND, span, format!("cannot apply `{}` to values of type `{other}`", op.symbol()));

          Type::Error
        }
      };
    }

    let supported = match &ty {
      Type::Int(_) | Type::Float(_) | Type::Char | Type::Error | Type::Never => true,
      Type::Bool => matches!(op, BinaryOp::Eq | BinaryOp::Ne),
      // Enums without data compare by variant (docs/spec/02-types.md#equality-f1f2).
      Type::Enum(_) => {
        let fieldless = self.typed.variants(&ty).is_some_and(|variants| variants.iter().all(|variant| variant.fields.is_empty()));

        if fieldless && matches!(op, BinaryOp::Eq | BinaryOp::Ne) {
          true
        } else {
          let message = format!("cannot compare values of type `{ty}` with `{}`", op.symbol());

          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_OPERAND, span, message)
              .with_note("only enums whose variants carry no data support `==` and `!=`; use `match` instead"),
          );

          return Type::Bool;
        }
      }
      // Byte-wise lexicographic order.
      Type::String => true,
      // Pointers compare by address.
      Type::Ptr { .. } => matches!(op, BinaryOp::Eq | BinaryOp::Ne),
      _ => false,
    };

    if !supported {
      self.error(codes::INVALID_OPERAND, span, format!("cannot compare values of type `{ty}` with `{}`", op.symbol()));
    }

    Type::Bool
  }

  fn cast(&mut self, inner: &Expr, ty: &TypeExpr) -> Type {
    let to = self.type_expr(ty);
    // `0 as *const T` is the null pointer: the literal is a `usize`.
    let hint = matches!(to, Type::Ptr { .. }).then_some(Type::Int(IntTy::Usize));
    let from = self.expr(inner, hint.as_ref());
    let valid = match (&from, &to) {
      (Type::Error | Type::Never, _) | (_, Type::Error) => true,
      (from, to) if from == to => true,
      (Type::Ptr { .. }, Type::Ptr { .. } | Type::Int(IntTy::Usize)) | (Type::Int(IntTy::Usize), Type::Ptr { .. }) => true,
      (Type::Int(_) | Type::Float(_), Type::Int(_) | Type::Float(_)) => true,
      (Type::Bool | Type::Char, Type::Int(_)) => true,
      (Type::Int(IntTy::U8), Type::Char) => true,
      _ => false,
    };

    if !valid {
      let span = inner.span.to(ty.span);

      self.error(codes::INVALID_CAST, span, format!("cannot convert a value of type `{from}` to `{to}` with `as`"));
    }

    to
  }

  fn if_expr(&mut self, cond: &Expr, then_block: &Block, else_branch: Option<&Expr>, expected: Option<&Type>) -> Type {
    self.expect(cond, &Type::Bool);

    let then_ty = self.block(then_block, expected);
    let Some(else_branch) = else_branch else {
      if !then_ty.fits(&Type::Unit) {
        let span = then_block.tail.as_ref().map_or(then_block.span, |tail| tail.span);

        self.diagnostics.push(
          Diagnostic::error(codes::MISMATCHED_TYPES, span, format!("expected `()`, found `{then_ty}`"))
            .with_note("an `if` without `else` has type `()`"),
        );
      }

      return Type::Unit;
    };
    let else_expected = if then_ty == Type::Never { expected.cloned() } else { Some(then_ty.clone()) };
    let else_ty = self.expr(else_branch, else_expected.as_ref());

    match (&then_ty, &else_ty) {
      (Type::Never, other) | (other, Type::Never) => other.clone(),
      (Type::Error, _) | (_, Type::Error) => Type::Error,
      _ if then_ty == else_ty => then_ty,
      _ => {
        let mut diagnostic = Diagnostic::error(codes::MISMATCHED_TYPES, else_branch.span, format!("expected `{then_ty}`, found `{else_ty}`"))
          .with_note("both branches of an `if` must have the same type");

        if let Some(tail) = &then_block.tail {
          diagnostic = diagnostic.with_label(tail.span, format!("this branch has type `{then_ty}`"));
        }

        self.diagnostics.push(diagnostic);

        Type::Error
      }
    }
  }

  fn loop_body(&mut self, kind: LoopKind, expected: Option<Type>, body: &Block) -> LoopCx {
    self.loops.push(LoopCx { kind, expected, break_ty: None, has_break: false });

    let body_ty = self.block(body, Some(&Type::Unit));

    if !body_ty.fits(&Type::Unit) {
      let span = body.tail.as_ref().map_or(body.span, |tail| tail.span);

      self.mismatch(span, &Type::Unit, &body_ty);
    }

    self.loops.pop().unwrap_or(LoopCx { kind, expected: None, break_ty: None, has_break: false })
  }

  fn for_iter(&mut self, iter: &Expr) -> Type {
    if let Some(elem) = self.text_iterable(iter) {
      return elem;
    }

    let ExprKind::Range { start, end, .. } = &iter.kind else {
      return match self.expr(iter, None) {
        Type::Array(elem, _) => *elem,
        Type::Error | Type::Never => Type::Error,
        other => {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_OPERAND, iter.span, format!("cannot iterate over a value of type `{other}`"))
              .with_note("`for` iterates over ranges and arrays"),
          );

          Type::Error
        }
      };
    };
    let ty = self.same_type_operands(start, end, None);
    let ty = match ty {
      Type::Int(_) | Type::Error => ty,
      other => {
        self.error(codes::INVALID_OPERAND, iter.span, format!("range bounds must be integers, found `{other}`"));

        Type::Error
      }
    };

    self.typed.expr_types.insert(iter.id, ty.clone());

    ty
  }

  fn break_expr(&mut self, span: Span, value: Option<&Expr>) {
    let Some(kind) = self.loops.last().map(|cx| cx.kind) else {
      self.error(codes::BREAK_OUTSIDE_LOOP, span, "`break` outside of a loop");

      if let Some(value) = value {
        self.expr(value, None);
      }

      return;
    };

    if kind != LoopKind::Loop {
      if let Some(value) = value {
        self.error(codes::BREAK_WITH_VALUE, value.span, "`break` with a value is only allowed inside `loop`");
        self.expr(value, None);
      }

      return;
    }

    let (known, expected) = self.loops.last().map(|cx| (cx.break_ty.clone(), cx.expected.clone())).unwrap_or((None, None));
    let ty = match (value, &known) {
      (Some(value), Some(known)) => {
        self.expect(value, known);

        known.clone()
      }
      (Some(value), None) => self.expr(value, expected.as_ref()),
      (None, Some(known)) => {
        if !Type::Unit.fits(known) {
          self.mismatch(span, known, &Type::Unit);
        }

        known.clone()
      }
      (None, None) => Type::Unit,
    };

    if let Some(cx) = self.loops.last_mut() {
      cx.has_break = true;

      if cx.break_ty.is_none() && ty != Type::Never {
        cx.break_ty = Some(ty);
      }
    }
  }

  fn call(&mut self, expr: &Expr, callee: &Expr, args: &[Expr]) -> Type {
    let sig = match (&callee.kind, self.resolution.paths.get(&callee.id).copied()) {
      (ExprKind::Path(_), Some(Res::Fn(id))) => self.typed.sig(id).cloned(),
      (ExprKind::Path(_), Some(Res::Extern(id))) => self.extern_call_sig(callee, id),
      (ExprKind::Path(_), Some(Res::Builtin(builtin))) => {
        if builtin == Builtin::StringFromC {
          self.require_unsafe(callee.span, "reading a C string");
        }

        Some(builtin_sig(builtin))
      }
      (ExprKind::Path(_), None) => None,
      _ => {
        self.error(codes::NOT_IMPLEMENTED, callee.span, "calling values other than functions is not implemented yet");

        None
      }
    };
    let Some(sig) = sig else {
      self.unchecked_args(args);

      return Type::Error;
    };

    self.args(expr.span, "function", &sig.params, args);

    sig.ret
  }

  fn unchecked_args(&mut self, args: &[Expr]) {
    for arg in args {
      self.expr(arg, None);
    }
  }

  fn args(&mut self, span: Span, what: &str, params: &[Type], args: &[Expr]) {
    if args.len() != params.len() {
      self.error(
        codes::WRONG_ARGUMENT_COUNT,
        span,
        format!("this {what} takes {} argument{} but {} were supplied", params.len(), plural(params.len()), args.len()),
      );
    }

    for (index, arg) in args.iter().enumerate() {
      match params.get(index) {
        Some(param) => self.expect(arg, param),
        None => {
          self.expr(arg, None);
        }
      }
    }
  }

  fn method_call(&mut self, expr: &Expr, receiver: &Expr, method: &Ident, args: &[Expr]) -> Type {
    let receiver_ty = self.expr(receiver, None);
    let name = method.name.as_str();

    if let Some(ty) = self.ffi_method(expr, receiver, &receiver_ty, method, args) {
      return ty;
    }

    let owner = match &receiver_ty {
      Type::Struct(ty) => TypeItem::Struct(ty.id),
      Type::Enum(ty) => TypeItem::Enum(ty.id),
      Type::Option(_) | Type::Result(..) => return self.builtin_method(expr, &receiver_ty, method, args),
      Type::Array(..) if name == "len" => {
        self.args(expr.span, "method", &[], args);

        return Type::Int(IntTy::Usize);
      }
      Type::Error | Type::Never => {
        self.unchecked_args(args);

        return Type::Error;
      }
      Type::String | Type::Char | Type::StringBuilder => return self.text_method(expr, &receiver_ty, method, args),
      _ => {
        let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{receiver_ty}`"));

        if matches!(receiver_ty, Type::Array(..)) {
          diagnostic = diagnostic.with_help("arrays have the methods `len()`, `as_ptr()` and `as_mut_ptr()`");
        }

        self.diagnostics.push(diagnostic);
        self.unchecked_args(args);

        return Type::Error;
      }
    };
    let found = self.resolution.type_function(self.package, owner, name);
    let (Some(function), Some(decl)) = (found, found.and_then(|function| self.resolution.fn_decl(self.package, function))) else {
      let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{receiver_ty}`"));
      let functions = match owner {
        TypeItem::Struct(id) => self.resolution.struct_decl(self.package, id).map(|decl| &decl.functions),
        TypeItem::Enum(id) => self.resolution.enum_decl(self.package, id).map(|decl| &decl.functions),
      };
      let methods: Vec<String> =
        functions.into_iter().flatten().filter(|function| function.self_param.is_some()).map(|function| function.name.name.clone()).collect();

      if let Some(similar) = sagitario_diagnostics::closest_match(name, methods.iter().map(String::as_str)) {
        diagnostic = diagnostic.with_help(format!("a method with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);
      self.unchecked_args(args);

      return Type::Error;
    };
    let Some(sig) = self.typed.sig(function).cloned() else { return Type::Error };

    match sig.receiver {
      None => {
        self.diagnostics.push(
          Diagnostic::error(codes::WRONG_CALL_FORM, method.span, format!("`{name}` is a static function of `{receiver_ty}`, not a method"))
            .with_label(decl.name.span, "declared without a `self` parameter here")
            .with_help(format!("call it as `{receiver_ty}::{name}(...)`")),
        );
      }
      Some(receiver_kind) => {
        if decl.visibility == Visibility::Private && self.owner != Some(owner) {
          self.diagnostics.push(
            Diagnostic::error(codes::PRIVATE_MEMBER, method.span, format!("the method `{name}` of `{receiver_ty}` is private"))
              .with_label(decl.name.span, "declared `prv` here"),
          );
        }

        if receiver_kind == Receiver::Var {
          self.check_mutable_place(receiver, PlaceUse::Receiver(name));
        }

        self.typed.method_calls.insert(expr.id, function);
      }
    }

    self.args(expr.span, "method", &sig.params, args);

    sig.ret
  }

  fn field(&mut self, base: &Expr, name: &Ident) -> Type {
    let base_ty = self.expr(base, None);
    let field_name = name.name.as_str();
    let Type::Struct(owner) = &base_ty else {
      if !matches!(base_ty, Type::Error | Type::Never) {
        self.error(codes::UNKNOWN_MEMBER, name.span, format!("no field `{field_name}` on type `{base_ty}`"));
      }

      return Type::Error;
    };
    let Some(def) = self.typed.struct_def(owner.id).cloned() else { return Type::Error };
    let Some(field) = def.fields.iter().find(|field| field.name == field_name) else {
      let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, name.span, format!("no field `{field_name}` on type `{base_ty}`"));

      if self.resolution.type_function(self.package, TypeItem::Struct(owner.id), field_name).is_some() {
        diagnostic = diagnostic.with_help(format!("`{field_name}` is a function; call it with `{field_name}(...)`"));
      } else if let Some(similar) = sagitario_diagnostics::closest_match(field_name, def.fields.iter().map(|field| field.name.as_str())) {
        diagnostic = diagnostic.with_help(format!("a field with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);

      return Type::Error;
    };

    if !field.public && self.owner != Some(TypeItem::Struct(owner.id)) {
      self.diagnostics.push(
        Diagnostic::error(codes::PRIVATE_MEMBER, name.span, format!("the field `{field_name}` of `{base_ty}` is private"))
          .with_label(field.span, "declared `prv` here"),
      );
    }

    field.ty.clone()
  }

  fn index(&mut self, base: &Expr, index: &Expr) -> Type {
    let base_ty = self.expr(base, None);

    self.expect(index, &Type::Int(IntTy::Usize));

    match base_ty {
      Type::Array(elem, _) => *elem,
      Type::Error | Type::Never => Type::Error,
      Type::String => {
        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_OPERAND, base.span, "a `String` cannot be indexed")
            .with_note("iterate over `chars()` or `bytes()` instead"),
        );

        Type::Error
      }
      other => {
        self.error(codes::INVALID_OPERAND, base.span, format!("cannot index a value of type `{other}`"));

        Type::Error
      }
    }
  }

  fn struct_literal(&mut self, expr: &Expr, path: &Path, fields: &[FieldInit]) -> Type {
    let id = match self.resolution.paths.get(&expr.id).copied() {
      Some(Res::Struct(id)) => id,
      Some(Res::Variant(variant)) => return self.named_variant_literal(path, variant, fields),
      _ => {
        for field in fields {
          self.expr(&field.value, None);
        }

        return Type::Error;
      }
    };
    let ty = self.struct_ref(id);
    let def = self.typed.struct_def(id).cloned().unwrap_or(StructDef { name: String::new(), fields: Vec::new() });
    let mut seen: Vec<(&str, Span)> = Vec::new();

    for init in fields {
      let name = init.name.name.as_str();

      if let Some((_, previous)) = seen.iter().find(|(seen, _)| *seen == name) {
        self.diagnostics.push(
          Diagnostic::error(codes::DUPLICATE_DEFINITION, init.name.span, format!("the field `{name}` is initialized more than once"))
            .with_label(*previous, "first initialized here"),
        );
        self.expr(&init.value, None);

        continue;
      }

      seen.push((name, init.name.span));

      match def.fields.iter().find(|field| field.name == name) {
        Some(field) => self.expect(&init.value, &field.ty),
        None => {
          let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, init.name.span, format!("the struct `{ty}` has no field `{name}`"));

          if let Some(similar) = sagitario_diagnostics::closest_match(name, def.fields.iter().map(|field| field.name.as_str())) {
            diagnostic = diagnostic.with_help(format!("a field with a similar name exists: `{similar}`"));
          }

          self.diagnostics.push(diagnostic);
          self.expr(&init.value, None);
        }
      }
    }

    let missing: Vec<String> =
      def.fields.iter().filter(|field| !seen.iter().any(|(name, _)| *name == field.name)).map(|field| format!("`{}`", field.name)).collect();

    if !missing.is_empty() {
      self.diagnostics.push(
        Diagnostic::error(
          codes::MISSING_FIELDS,
          path.span,
          format!("missing field{} {} in the literal of `{ty}`", plural(missing.len()), missing.join(", ")),
        )
        .with_note("struct literals must initialize every field"),
      );
    }

    if self.owner != Some(TypeItem::Struct(id))
      && let Some(field) = def.fields.iter().find(|field| !field.public)
    {
      self.diagnostics.push(
        Diagnostic::error(
          codes::PRIVATE_MEMBER,
          path.span,
          format!("cannot build `{ty}` with a literal here: the field `{}` is private", field.name),
        )
        .with_label(field.span, "declared `prv` here")
        .with_help(format!("use a static function of `{ty}` that builds it, such as `{ty}::new(...)`")),
      );
    }

    ty
  }

  /// The display path of a variant, such as `Shape::Circle` or `Some`.
  fn variant_path(&self, variant: VariantRef) -> String {
    match variant.owner {
      EnumKind::User(id) => {
        let enum_name = self.resolution.type_name(self.package, TypeItem::Enum(id));
        let variant_name = self.typed.enums.get(id.0 as usize).and_then(|def| def.variants.get(variant.index as usize)).map(|def| def.name.clone());

        format!("{enum_name}::{}", variant_name.unwrap_or_default())
      }
      kind => kind.prelude_variants().get(variant.index as usize).copied().unwrap_or_default().to_string(),
    }
  }

  /// The type of `variant` in a context expecting `expected`. Prelude variants take their type
  /// arguments from it: `None` and the other half of `Ok`/`Err` cannot be inferred otherwise.
  fn variant_type(&mut self, span: Span, variant: VariantRef, expected: Option<&Type>) -> Type {
    match (variant.owner, expected) {
      (EnumKind::User(id), _) => self.type_ref(TypeItem::Enum(id)),
      (EnumKind::Option, Some(ty @ Type::Option(_))) | (EnumKind::Result, Some(ty @ Type::Result(..))) => ty.clone(),
      (EnumKind::Option, _) => {
        self.error(codes::UNKNOWN_TYPE, span, "cannot infer the type of `None`; add a type annotation such as `Option<i32>`");

        Type::Error
      }
      (EnumKind::Result, _) => {
        let missing = if variant.index == 0 { "error" } else { "value" };
        let path = self.variant_path(variant);

        self.error(
          codes::UNKNOWN_TYPE,
          span,
          format!("cannot infer the {missing} type of `{path}(...)`; add a type annotation such as `Result<i32, String>`"),
        );

        Type::Error
      }
    }
  }

  /// A variant used as a value: `Shape::Empty`, `None` (no `args`) or `Shape::Circle(r)`,
  /// `Some(x)` (with `args`).
  fn variant_value(&mut self, span: Span, variant: VariantRef, args: Option<&[Expr]>, expected: Option<&Type>) -> Type {
    let path = self.variant_path(variant);

    // `Some(x)` without an expected `Option`: the type comes from `x`.
    if variant.owner == EnumKind::Option && variant.index == 1 && !matches!(expected, Some(Type::Option(_))) {
      return match args {
        Some([arg]) => match self.expr(arg, None) {
          Type::Never | Type::Error => Type::Error,
          value => Type::Option(Box::new(value)),
        },
        Some(args) => {
          self.args(span, "variant", &[Type::Error], args);

          Type::Error
        }
        None => {
          self.error(codes::WRONG_CALL_FORM, span, "`Some` carries data; write `Some(...)`");

          Type::Error
        }
      };
    }

    let owner_ty = self.variant_type(span, variant, expected);
    let def = self.typed.variants(&owner_ty).and_then(|variants| variants.get(variant.index as usize).cloned());
    let Some(def) = def else {
      self.unchecked_args(args.unwrap_or_default());

      return Type::Error;
    };
    let form_error = match (args, def.fields.is_empty(), def.named) {
      (Some(_), true, _) => Some(format!("`{path}` carries no data; write `{path}` without parentheses")),
      (None, false, false) => Some(format!("`{path}` carries data; write `{path}(...)`")),
      (_, false, true) => Some(format!("`{path}` has named fields; write `{path} {{ ... }}`")),
      _ => None,
    };

    if let Some(message) = form_error {
      self.error(codes::WRONG_CALL_FORM, span, message);
      self.unchecked_args(args.unwrap_or_default());

      return owner_ty;
    }

    let Some(args) = args else { return owner_ty };

    self.args(span, "variant", &def.field_types(), args);

    owner_ty
  }

  /// `Shape::Rect { w: 1.0, h: 2.0 }`
  fn named_variant_literal(&mut self, path: &Path, variant: VariantRef, fields: &[FieldInit]) -> Type {
    let name = self.variant_path(variant);
    let ty = match variant.owner {
      EnumKind::User(id) => self.type_ref(TypeItem::Enum(id)),
      EnumKind::Option | EnumKind::Result => Type::Error,
    };
    let def = self.typed.variants(&ty).and_then(|variants| variants.get(variant.index as usize).cloned());
    let Some(def) = def.filter(|def| def.named) else {
      self.error(codes::WRONG_CALL_FORM, path.span, format!("`{name}` has no named fields"));

      for field in fields {
        self.expr(&field.value, None);
      }

      return Type::Error;
    };
    let mut seen: Vec<(&str, Span)> = Vec::new();

    for init in fields {
      let field_name = init.name.name.as_str();

      if let Some((_, previous)) = seen.iter().find(|(seen, _)| *seen == field_name) {
        self.diagnostics.push(
          Diagnostic::error(codes::DUPLICATE_DEFINITION, init.name.span, format!("the field `{field_name}` is initialized more than once"))
            .with_label(*previous, "first initialized here"),
        );
        self.expr(&init.value, None);

        continue;
      }

      seen.push((field_name, init.name.span));

      match def.field_index(field_name).and_then(|index| def.fields.get(index)) {
        Some((_, field_ty)) => self.expect(&init.value, &field_ty.clone()),
        None => {
          self.error(codes::UNKNOWN_MEMBER, init.name.span, format!("the variant `{name}` has no field `{field_name}`"));
          self.expr(&init.value, None);
        }
      }
    }

    let missing: Vec<String> = def
      .fields
      .iter()
      .filter_map(|(field, _)| field.as_ref())
      .filter(|field| !seen.iter().any(|(name, _)| name == field))
      .map(|field| format!("`{field}`"))
      .collect();

    if !missing.is_empty() {
      self.error(
        codes::MISSING_FIELDS,
        path.span,
        format!("missing field{} {} in the literal of `{name}`", plural(missing.len()), missing.join(", ")),
      );
    }

    ty
  }

  /// A method of `Option` or `Result` (docs/spec/07-errors.md).
  fn builtin_method(&mut self, expr: &Expr, receiver_ty: &Type, method: &Ident, args: &[Expr]) -> Type {
    let found = match (receiver_ty, method.name.as_str()) {
      (Type::Option(_), "is_some") => Some((BuiltinMethod::IsSome, Vec::new(), Type::Bool)),
      (Type::Option(_), "is_none") => Some((BuiltinMethod::IsNone, Vec::new(), Type::Bool)),
      (Type::Result(..), "is_ok") => Some((BuiltinMethod::IsOk, Vec::new(), Type::Bool)),
      (Type::Result(..), "is_err") => Some((BuiltinMethod::IsErr, Vec::new(), Type::Bool)),
      (Type::Option(value) | Type::Result(value, _), "unwrap") => Some((BuiltinMethod::Unwrap, Vec::new(), value.as_ref().clone())),
      (Type::Option(value) | Type::Result(value, _), "expect") => Some((BuiltinMethod::Expect, vec![Type::String], value.as_ref().clone())),
      (Type::Option(value) | Type::Result(value, _), "unwrap_or") => {
        Some((BuiltinMethod::UnwrapOr, vec![value.as_ref().clone()], value.as_ref().clone()))
      }
      (Type::Result(_, error), "unwrap_err") => Some((BuiltinMethod::UnwrapErr, Vec::new(), error.as_ref().clone())),
      _ => None,
    };
    let Some((builtin, params, ret)) = found else {
      let available = if matches!(receiver_ty, Type::Option(_)) {
        "`is_some`, `is_none`, `unwrap`, `expect` and `unwrap_or`"
      } else {
        "`is_ok`, `is_err`, `unwrap`, `expect`, `unwrap_or` and `unwrap_err`"
      };

      self.diagnostics.push(
        Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{}` on type `{receiver_ty}`", method.name))
          .with_note(format!("in F1 its methods are {available}; methods taking closures arrive in F2")),
      );
      self.unchecked_args(args);

      return Type::Error;
    };

    self.args(expr.span, "method", &params, args);
    self.typed.builtin_methods.insert(expr.id, builtin);

    ret
  }

  /// `value?` (docs/spec/07-errors.md#the--operator).
  fn try_expr(&mut self, inner: &Expr) -> Type {
    let ty = self.expr(inner, None);
    let ret = self.ret.clone();
    let fails = |checker: &mut Self, message: String, help: Option<String>| {
      let mut diagnostic = Diagnostic::error(codes::INVALID_TRY, inner.span, message);

      if let Some(span) = checker.ret_span {
        diagnostic = diagnostic.with_label(span, format!("this function returns `{ret}`"));
      }

      if let Some(help) = help {
        diagnostic = diagnostic.with_help(help);
      }

      checker.diagnostics.push(diagnostic);
    };

    match (&ty, &ret) {
      (Type::Error | Type::Never, _) => Type::Error,
      (Type::Option(value), Type::Option(_) | Type::Error) => value.as_ref().clone(),
      (Type::Option(value), _) => {
        fails(self, "`?` on an `Option` can only be used in a function that returns `Option`".to_string(), None);

        value.as_ref().clone()
      }
      (Type::Result(value, error), Type::Result(_, expected)) => {
        if !error.fits(expected) {
          fails(
            self,
            format!("the error type `{error}` does not match the function's error type `{expected}`"),
            Some("convert the error explicitly with a `match`; `?` never converts errors (`map_err` arrives with closures in F2)".to_string()),
          );
        }

        value.as_ref().clone()
      }
      (Type::Result(value, _), Type::Error) => value.as_ref().clone(),
      (Type::Result(value, _), _) => {
        let help = (Some(self.function) == self.resolution.main).then(|| "declare `main` as `pub fn main() -> Result<(), String>`".to_string());

        fails(self, "`?` on a `Result` can only be used in a function that returns `Result`".to_string(), help);

        value.as_ref().clone()
      }
      (other, _) => {
        self.error(codes::INVALID_TRY, inner.span, format!("`?` applies to `Option` and `Result` values, found `{other}`"));

        Type::Error
      }
    }
  }

  fn array_literal(&mut self, span: Span, elems: &[Expr], expected: Option<&Type>) -> Type {
    let expected_elem = match expected {
      Some(Type::Array(elem, _)) => Some(elem.as_ref().clone()),
      _ => None,
    };
    let Some(first) = elems.first() else {
      return match expected_elem {
        Some(elem) => Type::Array(Box::new(elem), 0),
        None => {
          self.error(codes::UNKNOWN_TYPE, span, "cannot infer the element type of an empty array; add a type annotation");

          Type::Error
        }
      };
    };
    let mut elem_ty = self.expr(first, expected_elem.as_ref());

    for elem in elems.iter().skip(1) {
      if elem_ty == Type::Never {
        elem_ty = self.expr(elem, expected_elem.as_ref());
      } else {
        let because = Some((first.span, "the first element sets the element type"));

        self.expect_because(elem, &elem_ty.clone(), because);
      }
    }

    Type::Array(Box::new(elem_ty), elems.len() as u64)
  }
}

fn float_literal_type(suffix: Option<&str>, expected: Option<&Type>) -> Type {
  match (suffix.and_then(FloatTy::from_name), expected) {
    (Some(ty), _) => Type::Float(ty),
    (None, Some(Type::Float(ty))) => Type::Float(*ty),
    (None, _) => Type::Float(FloatTy::F64),
  }
}

/// An unsuffixed numeric literal, possibly negated, whose type comes from context.
fn is_untyped_literal(expr: &Expr) -> bool {
  match &expr.kind {
    ExprKind::Int { suffix: None, .. } | ExprKind::Float { suffix: None, .. } => true,
    ExprKind::Unary { op: UnaryOp::Neg, operand } => is_untyped_literal(operand),
    _ => false,
  }
}

fn plural(count: usize) -> &'static str {
  if count == 1 { "" } else { "s" }
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_diagnostics::Severity;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn diagnostics(text: &str) -> Vec<&'static str> {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.to_string());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
    let package = Package::single(sagitario_parser::parse_module(file, tokens, &mut diagnostics));
    let resolution = sagitario_resolve::resolve(&package, &mut diagnostics);

    assert!(!diagnostics.has_errors(), "front end failed: {:?}", diagnostics.take());

    check(&package, &resolution, &mut diagnostics);

    // Warnings are covered by the `flow` tests.
    diagnostics.take().iter().filter(|diagnostic| diagnostic.severity == Severity::Error).map(|diagnostic| diagnostic.code.0).collect()
  }

  /// Wraps statements in `pub fn main()`.
  fn in_main(body: &str) -> Vec<&'static str> {
    diagnostics(&format!("pub fn main() {{ {body} }}"))
  }

  #[test]
  fn accepts_well_typed_programs() {
    let program = "
      fn add(a: i64, b: i64) -> i64 { a + b }
      fn sign(x: i32) -> i32 { if x < 0 { -1 } else if x == 0 { 0 } else { 1 } }
      fn first_square_above(limit: u32) -> u32 {
        var n: u32 = 0;
        loop {
          n += 1;
          if n * n > limit { break n; }
        }
      }
      pub fn main() {
        const total = add(1, 2) + 3;
        const ratio: f32 = 1.5 * 2.0;
        const byte = 300 as u8;
        const message = `${total} ${ratio} ${byte} ${sign(-5)} ${first_square_above(50)} ${'x'} ${true}`;
        println(message);
        for i in 0..=10 { if i % 2 == 0 { continue; } }
        var count: u8 = 0;
        while count < 10u8 { count += 1; }
      }";

    assert_eq!(diagnostics(program), Vec::<&str>::new());
  }

  #[test]
  fn infers_literals_from_the_other_operand() {
    assert!(in_main("const x: u8 = 1; const y = 1 + x; const z: u8 = y;").is_empty());
    assert!(in_main("const x = -128i8; const y: i8 = -128;").is_empty());
  }

  #[test]
  fn rejects_mismatched_types() {
    assert_eq!(in_main("const x: i64 = 1i32;"), vec!["E0300"]);
    assert_eq!(in_main("const x = 1 + 2.0;"), vec!["E0300"]);
    assert_eq!(in_main("const x = if true { 1 } else { \"a\" };"), vec!["E0300"]);
    assert_eq!(in_main("if true { 1 }"), vec!["E0300"]);
    assert_eq!(diagnostics("fn f() -> i32 { }\npub fn main() {}"), vec!["E0300"]);
    assert_eq!(diagnostics("fn f() -> i32 { return; }\npub fn main() {}"), vec!["E0300"]);
  }

  #[test]
  fn rejects_invalid_operands() {
    assert_eq!(in_main("const x = -1u32;"), vec!["E0310"]);
    assert_eq!(in_main("const x: u32 = 1; const y = -x;"), vec!["E0310"]);
    assert_eq!(in_main("const x = !1;"), vec!["E0300"]);
    assert_eq!(in_main("const x = true + false;"), vec!["E0310"]);
    assert_eq!(in_main("const x = true < false;"), vec!["E0310"]);
    assert_eq!(in_main("for i in 0.0..1.0 {}"), vec!["E0310"]);
  }

  #[test]
  fn checks_casts() {
    assert!(in_main("const a = 1.9 as i32; const b = true as u8; const c = 'a' as u32; const d = 65u8 as char;").is_empty());
    assert_eq!(in_main("const a = 1 as bool;"), vec!["E0305"]);
    assert_eq!(in_main("const a = 66u16 as char;"), vec!["E0305"]);
  }

  #[test]
  fn checks_loops_and_breaks() {
    assert_eq!(in_main("break;"), vec!["E0303"]);
    assert_eq!(in_main("continue;"), vec!["E0303"]);
    assert_eq!(in_main("while true { break 1; }"), vec!["E0304"]);
    assert_eq!(in_main("const x = loop { if true { break 1; } break \"a\"; };"), vec!["E0300"]);
    assert!(in_main("const x: u8 = loop { break 255; };").is_empty());
  }

  #[test]
  fn checks_interpolation_and_ranges() {
    assert_eq!(in_main("const x = `${()}`;"), vec!["E0306"]);
    assert_eq!(in_main("const r = 0..10;"), vec!["E0900"]);
  }

  #[test]
  fn checks_literal_ranges() {
    assert_eq!(in_main("const x: u8 = 256;"), vec!["E0302"]);
    assert_eq!(in_main("const x: i8 = -129;"), vec!["E0302"]);
  }

  #[test]
  fn infers_deferred_bindings_from_their_first_assignment() {
    assert!(in_main("var x; x = 5u8; const y: u8 = x;").is_empty());
    assert_eq!(in_main("var x;"), vec!["E0311"]);
  }

  #[test]
  fn rejects_invalid_assignment_targets() {
    assert_eq!(in_main("println = 1;"), vec!["E0307"]);
  }

  #[test]
  fn validates_main_signature() {
    assert_eq!(diagnostics("fn main() {}"), Vec::<&str>::new());
    assert_eq!(diagnostics("pub fn main() -> Result<(), String> { Ok(()) }"), Vec::<&str>::new());
    assert_eq!(diagnostics("pub fn main(x: i32) {}"), vec!["E0204"]);
    assert_eq!(diagnostics("fn main() -> i32 { 0 }"), vec!["E0204"]);
  }

  const POINT: &str = "
    struct Point {
      pub x: i32;
      prv tag: u8;
      pub fn new(x: i32) -> Self { Self { x, tag: 0 } }
      pub fn get(self) -> i32 { self.x + self.tag as i32 }
      pub fn set(var self, x: i32) { self.x = x; }
    }";

  fn with_point(body: &str) -> Vec<&'static str> {
    diagnostics(&format!("{POINT}\npub fn main() {{ {body} }}"))
  }

  #[test]
  fn checks_struct_members() {
    assert!(with_point("var p = Point::new(1); p.set(p.get() + 1); p.x += 1; const a = [p; 2]; const _ = a[1].x;").is_empty());
    assert_eq!(with_point("const _ = Point::new(1).tag;"), vec!["E0205"]);
    assert_eq!(with_point("const _ = Point::new(1).get(2);"), vec!["E0301"]);
    assert_eq!(with_point("const _: u8 = Point::new(1).x;"), vec!["E0300"]);
    assert_eq!(with_point("const _ = Point::new(\"a\");"), vec!["E0300"]);
  }

  #[test]
  fn infers_array_elements_from_the_expected_type() {
    assert!(in_main("const a: [u8; 2] = [1, 255]; const b: [i64; 3] = [0; 3]; const _ = a[0] + 1; const _ = b;").is_empty());
    assert_eq!(in_main("const _: [u8; 2] = [1, 256];"), vec!["E0302"]);
    assert_eq!(in_main("const _: [u8; 2] = [7; 2]; const _ = [1; 99999999999999999999999];"), vec!["E0302"]);
  }

  #[test]
  fn resolves_self_and_struct_types_in_signatures() {
    let program = "
      struct Pair { pub a: [Point; 2]; pub fn swap(self) -> Self { Self { a: [self.a[1], self.a[0]] } } }
      struct Point { pub x: i32; }
      fn first(p: Pair) -> Point { p.a[0] }
      pub fn main() { const _ = first(Pair { a: [Point { x: 1 }; 2] }.swap()); }";

    assert_eq!(diagnostics(program), Vec::<&str>::new());
    assert_eq!(diagnostics("fn f() -> Self { }\npub fn main() {}"), vec!["E0200"]);
  }

  #[test]
  fn infers_prelude_variants_from_context() {
    assert!(
      in_main(
        "const a = Some(1); const b: Option<u8> = None; const c: Result<u8, String> = Ok(1); const _ = [a, Some(2)]; const _ = b; const _ = c;"
      )
      .is_empty()
    );
    assert!(in_main("const a: Option<u8> = Some(255); const _ = if true { a } else { None };").is_empty());
    assert_eq!(in_main("const _ = if true { None } else { Some(1) };"), vec!["E0311"]);
    assert_eq!(in_main("const _: Option<u8> = Some(256);"), vec!["E0302"]);
    assert!(diagnostics("fn f() -> Result<i32, String> { Err(\"e\") }\npub fn main() { const _ = f(); }").is_empty());
  }

  #[test]
  fn checks_strings_and_text_methods() {
    assert!(
      in_main("var s = \"a\" + \"b\"; s += \"c\"; const _: bool = s < \"z\"; const _: usize = s.len(); const _: Option<usize> = s.find(\"b\");")
        .is_empty()
    );
    assert!(
      in_main("const _: Result<u8, String> = \"1\".parse_u8(); const _: char = 'a'.to_ascii_uppercase(); for _c in \"x\".chars() {}").is_empty()
    );
    assert_eq!(in_main("const _ = match \"a\" { \"a\" => 1, \"b\" => 2 };"), vec!["E0319"]);
    assert!(in_main("const _ = match \"a\" { \"a\" => 1, _ => 2 };").is_empty());
    assert!(in_main("const b = new StringBuilder(); b.append(\"x\"); const _: String = b.to_string();").is_empty());
    assert_eq!(in_main("const _: i32 = panic(\"no\");"), Vec::<&str>::new());
  }

  #[test]
  fn checks_match_exhaustiveness() {
    let enums = "enum E { A(Option<bool>), B { n: i32 }, C }\n";
    let with_enum = |body: &str| diagnostics(&format!("{enums}pub fn main() {{ const e = E::C; {body} }}"));

    assert!(
      with_enum("match e { E::A(Some(true)) => {} E::A(Some(false)) | E::A(None) => {} E::B { n: 0 } => {} E::B { .. } => {} E::C => {} }")
        .is_empty()
    );
    assert_eq!(with_enum("match e { E::A(Some(true)) => {} E::B { .. } | E::C => {} }"), vec!["E0319"]);
    assert_eq!(with_enum("match e { E::A(_) => {} E::B { n } if n > 0 => {} E::C => {} }"), vec!["E0319"]);
    assert_eq!(with_enum("const _: i32 = match e { E::A(_) => 1, E::B { n } => n, E::C => true };"), vec!["E0300"]);
    assert!(with_enum("const _ = match true { true => 1, false => 2 };").is_empty());
  }
}
