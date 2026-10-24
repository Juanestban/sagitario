//! Type checking with local inference (docs/spec/02-types.md, 06-control-flow.md). Records the
//! type of every expression and local binding for lowering; `patterns` checks `match` (with the
//! exhaustiveness algorithm in `exhaustive`), `classes` checks classes and which methods modify
//! `self`, `inheritance` the class hierarchy, `interfaces` interfaces and their implementations,
//! and `flow` then checks definite assignment and reachability.

mod classes;
mod closures;
mod exhaustive;
mod ffi;
mod flow;
mod generic;
mod inheritance;
mod interfaces;
mod patterns;
mod text;

use std::collections::{HashMap, HashSet};
use std::fmt;

use sagitario_ast::{
  ArrayLen, BinaryOp, Block, Expr, ExprKind, FieldInit, FnDecl, Ident, NodeId, Package, Path, Stmt, TemplatePart, TypeExpr, TypeExprKind, UnaryOp,
  Visibility,
};
use sagitario_diagnostics::{Code, Diagnostic, Diagnostics, codes};
use sagitario_resolve::{
  Builtin, CType, ClassId, EnumId, EnumKind, ExternId, FnId, InterfaceId, LocalId, LocalKind, ParamOwner, Res, Resolution, StructId, TypeItem,
  TypeRes, VariantRef,
};

pub use generic::Subst;
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
  /// A reference to an instance of a class, managed by ARC.
  Class(ClassRef),
  /// A value of any type implementing an interface: a reference with its methods
  /// (docs/spec/05-interfaces-generics.md#interfaces-as-types).
  Interface(InterfaceRef),
  /// A type parameter of a generic declaration, or the `Self` of an interface: replaced by a
  /// concrete type in each instance (docs/spec/05-interfaces-generics.md#generics).
  Param(ParamRef),
  /// `fn(A, B) -> R`: a function or closure (docs/spec/08-closures.md).
  Function {
    params: Vec<Type>,
    ret: Box<Type>,
  },
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

/// A struct type. The name is kept for messages and is not part of the type's identity.
#[derive(Debug, Clone)]
pub struct StructRef {
  pub id: StructId,
  pub name: String,
  /// The type arguments of a generic type, in the order of its parameters.
  pub args: Vec<Type>,
}

impl PartialEq for StructRef {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && self.args == other.args
  }
}

impl Eq for StructRef {}

impl std::hash::Hash for StructRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.id.hash(state);
    self.args.hash(state);
  }
}

/// A class type. The name is kept for messages and is not part of the type's identity.
#[derive(Debug, Clone)]
pub struct ClassRef {
  pub id: ClassId,
  pub name: String,
  /// The type arguments of a generic type, in the order of its parameters.
  pub args: Vec<Type>,
}

impl PartialEq for ClassRef {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && self.args == other.args
  }
}

impl Eq for ClassRef {}

impl std::hash::Hash for ClassRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.id.hash(state);
    self.args.hash(state);
  }
}

/// An interface type. The name is kept for messages and is not part of the type's identity.
#[derive(Debug, Clone)]
pub struct InterfaceRef {
  pub id: InterfaceId,
  pub name: String,
  /// The type arguments of a generic type, in the order of its parameters.
  pub args: Vec<Type>,
}

impl PartialEq for InterfaceRef {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && self.args == other.args
  }
}

impl Eq for InterfaceRef {}

impl std::hash::Hash for InterfaceRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.id.hash(state);
    self.args.hash(state);
  }
}

/// An enum type of the program. The name is kept for messages and is not part of the type's identity.
#[derive(Debug, Clone)]
pub struct EnumRef {
  pub id: EnumId,
  pub name: String,
  /// The type arguments of a generic type, in the order of its parameters.
  pub args: Vec<Type>,
}

impl PartialEq for EnumRef {
  fn eq(&self, other: &Self) -> bool {
    self.id == other.id && self.args == other.args
  }
}

impl Eq for EnumRef {}

impl std::hash::Hash for EnumRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.id.hash(state);
    self.args.hash(state);
  }
}

/// The position of `Self` among the parameters of an interface.
pub const SELF_PARAM: u32 = u32::MAX;

/// A type parameter: the declaration it belongs to and its position there (`SELF_PARAM` for the
/// `Self` of an interface). The name is kept for messages.
#[derive(Debug, Clone)]
pub struct ParamRef {
  pub owner: ParamOwner,
  pub index: u32,
  pub name: String,
}

impl PartialEq for ParamRef {
  fn eq(&self, other: &Self) -> bool {
    self.owner == other.owner && self.index == other.index
  }
}

impl Eq for ParamRef {}

impl std::hash::Hash for ParamRef {
  fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
    self.owner.hash(state);
    self.index.hash(state);
  }
}

/// Writes `Name` or `Name<A, B>`.
fn write_generic(f: &mut fmt::Formatter<'_>, name: &str, args: &[Type]) -> fmt::Result {
  f.write_str(name)?;

  if args.is_empty() {
    return Ok(());
  }

  let args: Vec<String> = args.iter().map(Type::to_string).collect();

  write!(f, "<{}>", args.join(", "))
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
      Type::Struct(ty) => write_generic(f, &ty.name, &ty.args),
      Type::Enum(ty) => write_generic(f, &ty.name, &ty.args),
      Type::Class(ty) => write_generic(f, &ty.name, &ty.args),
      Type::Interface(ty) => write_generic(f, &ty.name, &ty.args),
      Type::Param(param) => f.write_str(&param.name),
      Type::Function { params, ret } => {
        let params: Vec<String> = params.iter().map(Type::to_string).collect();

        write!(f, "fn({})", params.join(", "))?;

        if **ret != Type::Unit { write!(f, " -> {ret}") } else { Ok(()) }
      }
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
  /// A copy of a struct or enum value, or a class reference.
  Value,
  /// The caller's struct or enum value: the method modifies `self` (inferred, ADR-0028).
  Var,
  /// The instance a class's initializer `new` initializes.
  Init,
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
    // The result is the prelude's `Error`, which the type checker knows.
    Builtin::ErrorNew => FnSig { receiver: None, params: vec![Type::String], ret: Type::Error },
  }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FieldDef {
  pub name: String,
  pub ty: Type,
  pub public: bool,
  /// Whether the field can change after construction: struct fields and `var` class fields.
  pub mutable: bool,
  /// A `weak` class field: an `Option<C>` that does not keep its instance alive.
  pub weak: bool,
  pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructDef {
  pub name: String,
  /// In declaration order.
  pub fields: Vec<FieldDef>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassDef {
  pub name: String,
  /// The class after `extends`, with type arguments in terms of this class's parameters.
  pub parent: Option<ClassRef>,
  pub is_abstract: bool,
  /// The class's own fields in declaration order; inherited ones belong to its ancestors.
  pub fields: Vec<FieldDef>,
  /// The class's own function `new` that initializes instances, if it declares one.
  pub initializer: Option<FnId>,
  /// The methods called through the vtable of instances of this class, by slot.
  pub vtable: Vec<FnId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InterfaceDef {
  pub name: String,
  /// The interfaces after `extends`, with type arguments in terms of this interface's
  /// parameters.
  pub parents: Vec<InterfaceRef>,
  /// The fields an implementing type must have.
  pub fields: Vec<FieldDef>,
  /// The interface's own methods, required and default, in declaration order.
  pub methods: Vec<FnId>,
  /// Whether the interface or an ancestor mentions `Self`, so it cannot be a type.
  pub uses_self: bool,
}

/// A type parameter of a generic declaration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GenericDef {
  pub name: String,
  /// The interfaces after `:` and in the `where` clause.
  pub bounds: Vec<InterfaceRef>,
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
  /// `Option::map` and `Result::map`, with a closure.
  Map,
  /// `Result::map_err`
  MapErr,
  /// `Option::and_then` and `Result::and_then`
  AndThen,
  /// `Option::unwrap_or_else` and `Result::unwrap_or_else`
  UnwrapOrElse,
  /// `Option::filter`
  Filter,
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
  /// Indexed by `ClassId`.
  pub classes: Vec<ClassDef>,
  /// Indexed by `InterfaceId`.
  pub interfaces: Vec<InterfaceDef>,
  /// The interfaces each struct, enum and class declares with `impl`, with type arguments in
  /// terms of the type's parameters.
  pub impls: HashMap<TypeItem, Vec<InterfaceRef>>,
  /// The type parameters of each generic declaration.
  pub generics: HashMap<ParamOwner, Vec<GenericDef>>,
  /// The type arguments of each use of a generic function, method or initializer, by the node
  /// id of the call or `new`: those of the function's type, then the function's own.
  pub call_args: HashMap<NodeId, Vec<Type>>,
  /// The prelude's `Error` interface and the class `Error(message)` creates.
  pub error_types: Option<(InterfaceId, ClassId)>,
  /// Calls to methods of `Option` and `Result`, by node id.
  pub builtin_methods: HashMap<NodeId, BuiltinMethod>,
  /// Type of each local binding, indexed by `FnId` then `LocalId`.
  pub local_types: Vec<Vec<Type>>,
  pub expr_types: HashMap<NodeId, Type>,
  /// The function called by each method call expression; absent for built-in methods such as
  /// the `len()` of arrays.
  pub method_calls: HashMap<NodeId, FnId>,
  /// Calls through `zod`, which run the named implementation without dynamic dispatch.
  pub zod_calls: HashSet<NodeId>,
  /// `value.name(args)` calling a field of a function type, by node id.
  pub field_calls: HashSet<NodeId>,
  /// The vtable slot of each method dispatched dynamically: one that is abstract, or is
  /// overridden in some subclass, and its overrides. Other methods are called directly.
  pub slots: HashMap<FnId, usize>,
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

  pub fn class_def(&self, id: ClassId) -> Option<&ClassDef> {
    self.classes.get(id.0 as usize)
  }

  /// A class followed by its ancestors, nearest first.
  pub fn class_chain(&self, id: ClassId) -> Vec<ClassId> {
    let mut chain = vec![id];

    // Cycles are reported and removed when the classes are defined; the bound is a safeguard.
    while let Some(parent) = chain.last().and_then(|last| self.class_def(*last)).and_then(|def| def.parent.as_ref().map(|parent| parent.id)) {
      if chain.len() > self.classes.len() {
        break;
      }

      chain.push(parent);
    }

    chain
  }

  /// Whether `sub` is `sup` or one of its subclasses.
  pub fn is_subclass(&self, sub: ClassId, sup: ClassId) -> bool {
    self.class_chain(sub).contains(&sup)
  }

  /// A field of a class or one of its ancestors: the declaring class, its position there, and
  /// the field.
  pub fn class_field(&self, id: ClassId, name: &str) -> Option<(ClassId, usize, &FieldDef)> {
    self.class_chain(id).into_iter().find_map(|class| {
      let def = self.class_def(class)?;
      let index = def.fields.iter().position(|field| field.name == name)?;

      Some((class, index, def.fields.get(index)?))
    })
  }

  pub fn interface_def(&self, id: InterfaceId) -> Option<&InterfaceDef> {
    self.interfaces.get(id.0 as usize)
  }

  /// An interface followed by its ancestors, breadth first, without repetitions.
  pub fn interface_ancestors(&self, id: InterfaceId) -> Vec<InterfaceId> {
    let mut found = vec![id];
    let mut next = 0;

    while let Some(current) = found.get(next).copied() {
      next += 1;

      for parent in self.interface_def(current).map(|def| def.parents.clone()).unwrap_or_default() {
        if !found.contains(&parent.id) {
          found.push(parent.id);
        }
      }
    }

    found
  }

  /// The positions in `parents` leading from interface `from` to its ancestor `to`.
  pub fn interface_path(&self, from: InterfaceId, to: InterfaceId) -> Option<Vec<usize>> {
    let mut queue: Vec<(InterfaceId, Vec<usize>)> = vec![(from, Vec::new())];
    let mut next = 0;

    while let Some((current, path)) = queue.get(next).cloned() {
      next += 1;

      if current == to {
        return Some(path);
      }

      if queue.len() > self.interfaces.len() * self.interfaces.len() + 1 {
        break;
      }

      for (index, parent) in self.interface_def(current).map(|def| def.parents.clone()).unwrap_or_default().into_iter().enumerate() {
        let mut path = path.clone();

        path.push(index);
        queue.push((parent.id, path));
      }
    }

    None
  }

  /// The interfaces a struct, enum or class implements, with their ancestors (for a class, also
  /// those of its ancestors).
  pub fn implemented(&self, item: TypeItem) -> Vec<InterfaceId> {
    let owners: Vec<TypeItem> = match item {
      TypeItem::Class(id) => self.class_chain(id).into_iter().map(TypeItem::Class).collect(),
      TypeItem::Interface(id) => return self.interface_ancestors(id),
      other => vec![other],
    };
    let mut found = Vec::new();

    for owner in owners {
      for interface in self.impls.get(&owner).cloned().unwrap_or_default() {
        for ancestor in self.interface_ancestors(interface.id) {
          if !found.contains(&ancestor) {
            found.push(ancestor);
          }
        }
      }
    }

    found
  }

  /// A method of an interface or one of its ancestors: the declaring interface and the method.
  pub fn interface_method(&self, id: InterfaceId, name: &str, resolution: &Resolution, package: &Package) -> Option<(InterfaceId, FnId)> {
    self.interface_ancestors(id).into_iter().find_map(|interface| {
      let methods = self.interface_def(interface)?.methods.clone();

      methods
        .into_iter()
        .find(|method| resolution.fn_decl(package, *method).is_some_and(|decl| decl.name.name == name))
        .map(|method| (interface, method))
    })
  }

  /// A field of an interface or one of its ancestors: the declaring interface, its position and
  /// the field.
  pub fn interface_field(&self, id: InterfaceId, name: &str) -> Option<(InterfaceId, usize, &FieldDef)> {
    self.interface_ancestors(id).into_iter().find_map(|interface| {
      let def = self.interface_def(interface)?;
      let index = def.fields.iter().position(|field| field.name == name)?;

      Some((interface, index, def.fields.get(index)?))
    })
  }

  /// The type item of a struct, enum, class or interface type.
  pub fn type_item(ty: &Type) -> Option<TypeItem> {
    match ty {
      Type::Struct(inner) => Some(TypeItem::Struct(inner.id)),
      Type::Enum(inner) => Some(TypeItem::Enum(inner.id)),
      Type::Class(inner) => Some(TypeItem::Class(inner.id)),
      Type::Interface(inner) => Some(TypeItem::Interface(inner.id)),
      _ => None,
    }
  }

  /// Whether a value of type `actual` can be used where `expected` is required, converting it if
  /// needed: an instance as one of its ancestors, a value as an interface its type implements
  /// (type parameters implement their bounds).
  pub fn fits(&self, actual: &Type, expected: &Type) -> bool {
    match (actual, expected) {
      (Type::Class(actual), Type::Class(expected)) => self.class_ancestors(actual).contains(expected),
      (Type::Struct(_) | Type::Enum(_) | Type::Class(_) | Type::Interface(_) | Type::Param(_), Type::Interface(expected)) => {
        self.implements(actual, expected)
      }
      _ => actual.fits(expected),
    }
  }

  /// The initializer `zod.new(...)` runs for a class: the nearest ancestor's.
  pub fn inherited_initializer(&self, id: ClassId) -> Option<FnId> {
    self.class_chain(id).into_iter().skip(1).find_map(|class| self.class_def(class).and_then(|def| def.initializer))
  }

  /// The variants of an enum, `Option` or `Result` type, with their field types.
  pub fn variants(&self, ty: &Type) -> Option<Vec<VariantDef>> {
    match ty {
      Type::Enum(ty) => {
        let subst = Subst::of(ParamOwner::Item(TypeItem::Enum(ty.id)), ty.args.clone());

        self.enums.get(ty.id.0 as usize).map(|def| {
          def
            .variants
            .iter()
            .map(|variant| VariantDef {
              fields: variant.fields.iter().map(|(name, field)| (name.clone(), field.substitute(&subst))).collect(),
              ..variant.clone()
            })
            .collect()
        })
      }
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
    mutates_self: vec![false; resolution.functions.len()],
    interface_types: Vec::new(),
    type_bounds: Vec::new(),
    lambda_stack: Vec::new(),
    inferring_ret: false,
    self_calls: Vec::new(),
    receiver_checks: Vec::new(),
    zod_new_calls: Vec::new(),
  };

  checker.struct_defs();
  checker.enum_defs();
  checker.class_defs();
  checker.interface_defs();
  checker.generic_defs();
  checker.check_recursive_types();
  checker.check_repr_c_structs();
  checker.extern_sigs();

  let decls = resolution.fn_decls(package);

  for (id, decl) in &decls {
    checker.owner = resolution.owner(*id);

    let sig = checker.signature(*id, decl);

    if decl.extern_c {
      checker.check_exported(decl, &sig);
    }

    checker.typed.sigs.push(sig);
  }

  checker.check_symbols();
  checker.check_classes();
  checker.check_interfaces();

  if let Some((id, main)) = decls.iter().find(|(id, _)| Some(*id) == resolution.main) {
    checker.check_main(*id, main);
  }

  for (id, decl) in &decls {
    checker.function(*id, decl);
  }

  checker.check_pointees();
  checker.infer_mutating_methods();
  // The bounds of the types written inside function bodies.
  checker.check_type_bounds();

  let typed = checker.typed;

  for (id, decl) in decls.iter().filter(|(_, decl)| !decl.is_abstract) {
    let owner = match resolution.owner(*id) {
      Some(TypeItem::Class(class)) => {
        let initializer = typed.class_def(class).filter(|def| def.initializer == Some(*id));

        flow::Owner::Class { initializer: initializer.map(|def| (def.name.as_str(), def.fields.as_slice())) }
      }
      // In a default method, `self` is a reference to the implementing value.
      Some(TypeItem::Interface(_)) => flow::Owner::Class { initializer: None },
      _ => flow::Owner::Value,
    };

    flow::check_function(decl, resolution.locals(*id), owner, resolution, &typed, diagnostics);
  }

  typed
}

/// What a place expression (a binding, or a field or element of one) is used for.
#[derive(Clone, Copy)]
enum PlaceUse<'a> {
  Assign,
  /// The receiver of the named method, which modifies it.
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
  /// Interfaces used as types, checked once it is known whether they mention `Self`.
  interface_types: Vec<(Span, InterfaceId)>,
  /// Generic types written with arguments, whose bounds are checked once every bound is known.
  type_bounds: Vec<(Span, Type)>,
  /// The lambdas being checked, innermost last.
  lambda_stack: Vec<NodeId>,
  /// Whether the result of the lambda being checked comes from its body: the first `return`
  /// sets it.
  inferring_ret: bool,
  /// Whether each struct or enum method modifies `self` directly, by `FnId`.
  mutates_self: Vec<bool>,
  /// Methods called on `self` (or a part of it) inside a method, as (caller, callee): the
  /// caller modifies `self` if the callee does.
  self_calls: Vec<(FnId, FnId)>,
  /// Methods called on values that cannot be modified, with the error to report if the method
  /// turns out to modify `self`.
  receiver_checks: Vec<(FnId, Diagnostic)>,
  /// The `zod.new(...)` calls that start an initializer, the only place they are allowed.
  zod_new_calls: Vec<NodeId>,
}

impl Checker<'_> {
  fn error(&mut self, code: Code, span: Span, message: impl Into<String>) {
    self.diagnostics.push(Diagnostic::error(code, span, message));
  }

  /// Whether a value of type `actual` can be used where `expected` is required (see
  /// `TypedModule::fits`).
  fn fits(&self, actual: &Type, expected: &Type) -> bool {
    self.typed.fits(actual, expected)
  }

  fn struct_ref(&self, id: StructId) -> Type {
    self.type_ref(TypeItem::Struct(id))
  }

  /// A program type used inside its own declaration: generic types take their own parameters
  /// as arguments.
  fn type_ref(&self, item: TypeItem) -> Type {
    self.named_type(item, self.identity_args(ParamOwner::Item(item)))
  }

  /// The parameters of a declaration as types.
  fn identity_args(&self, owner: ParamOwner) -> Vec<Type> {
    let generics = self.resolution.generics(self.package, owner);

    generics.iter().enumerate().map(|(index, param)| Type::Param(ParamRef { owner, index: index as u32, name: param.name.name.clone() })).collect()
  }

  /// A program type with type arguments.
  fn named_type(&self, item: TypeItem, args: Vec<Type>) -> Type {
    let name = self.resolution.type_name(self.package, item);

    match item {
      TypeItem::Struct(id) => Type::Struct(StructRef { id, name, args }),
      TypeItem::Enum(id) => Type::Enum(EnumRef { id, name, args }),
      TypeItem::Class(id) => Type::Class(ClassRef { id, name, args }),
      TypeItem::Interface(id) => Type::Interface(InterfaceRef { id, name, args }),
    }
  }

  /// The number of type parameters of a declaration.
  fn generic_count(&self, owner: ParamOwner) -> usize {
    self.resolution.generics(self.package, owner).len()
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
        .map(|field| {
          if field.weak {
            self.diagnostics.push(
              Diagnostic::error(codes::INVALID_WEAK, field.name.span, "only class fields can be `weak`")
                .with_note("a struct is a value; keep the weak reference in a class field"),
            );
          }

          FieldDef {
            name: field.name.name.clone(),
            ty: self.type_expr(&field.ty),
            public: field.visibility != Visibility::Private,
            mutable: true,
            weak: false,
            span: field.span,
          }
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
      // Classes and interfaces are references, so they never contain values inline.
      TypeItem::Class(_) | TypeItem::Interface(_) => Vec::new(),
    };

    parts.iter().any(|part| self.contains_type(part, target, visited))
  }

  fn signature(&mut self, id: FnId, decl: &FnDecl) -> FnSig {
    // Methods that modify `self` become `Receiver::Var` once every body is checked.
    let receiver = match decl.self_param {
      Some(_) => Some(Receiver::Value),
      None if self.resolution.is_initializer(self.package, id) => Some(Receiver::Init),
      None => None,
    };
    let params = decl.params.iter().map(|param| self.type_expr(&param.ty)).collect();
    // An initializer's return type is an error reported by `check_classes`.
    let ret = match &decl.return_type {
      Some(ty) if receiver != Some(Receiver::Init) => self.type_expr(ty),
      _ => Type::Unit,
    };

    FnSig { receiver, params, ret }
  }

  /// `fn main()` or `fn main() -> Result<(), String>`, with any visibility (docs/spec/07-errors.md).
  fn check_main(&mut self, id: FnId, main: &FnDecl) {
    let ret = self.typed.sig(id).map_or(Type::Unit, |sig| sig.ret.clone());
    // The error is a `String` or implements `Error` (docs/spec/07-errors.md).
    let valid_ret = match &ret {
      Type::Unit | Type::Error => true,
      Type::Result(value, error) => **value == Type::Unit && (**error == Type::String || self.implements_error(error)),
      _ => false,
    };

    if !main.params.is_empty() || !valid_ret {
      let mut diagnostic =
        Diagnostic::error(codes::INVALID_MAIN, main.name.span, "`main` must be declared as `fn main()` or `fn main() -> Result<(), E>`");

      if let Type::Result(..) = ret {
        diagnostic = diagnostic.with_note("the error type of `main` must be `String` or implement `Error`");
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
    // In a default method, `self` is the type implementing the interface: its `Self`.
    let self_ty = self.owner.filter(|_| sig.receiver.is_some()).map(|owner| match owner {
      TypeItem::Interface(id) => {
        Type::Param(ParamRef { owner: ParamOwner::Item(TypeItem::Interface(id)), index: SELF_PARAM, name: "Self".to_string() })
      }
      owner => self.type_ref(owner),
    });
    let param_types = self_ty.into_iter().chain(sig.params.iter().cloned());

    for (index, ty) in param_types.enumerate() {
      if let Some(slot) = self.locals.get_mut(index) {
        *slot = Some(ty);
      }
    }

    // An abstract method has no body to check.
    let body_ty = if decl.is_abstract { sig.ret.clone() } else { self.block(&decl.body, Some(&sig.ret)) };

    if !self.fits(&body_ty, &sig.ret) {
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
      TypeExprKind::Fn { params, ret } => Type::Function {
        params: params.iter().map(|param| self.type_expr(param)).collect(),
        ret: Box::new(ret.as_deref().map_or(Type::Unit, |ret| self.type_expr(ret))),
      },
      TypeExprKind::Unit => Type::Unit,
      TypeExprKind::Array { elem, len } => {
        let elem = self.type_expr(elem);

        match self.array_len(len) {
          Some(len) => Type::Array(Box::new(elem), len),
          None => Type::Error,
        }
      }
      TypeExprKind::Generic { path, args } if matches!(self.resolution.type_paths.get(&path.id), Some(TypeRes::Item(_))) => {
        let Some(TypeRes::Item(item)) = self.resolution.type_paths.get(&path.id).copied() else { return Type::Error };
        let name = path.segments.last().map_or("", |segment| segment.name.as_str());
        let types: Vec<Type> = args.iter().map(|arg| self.type_expr(arg)).collect();
        let expected = self.generic_count(ParamOwner::Item(item));

        if !self.check_arg_count(ty.span, &format!("`{name}`"), expected, types.len()) {
          return Type::Error;
        }

        let result = self.named_type(item, types);

        if let TypeItem::Interface(id) = item {
          self.interface_types.push((path.span, id));
        }

        self.type_bounds.push((ty.span, result.clone()));

        result
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
          Some(TypeRes::Item(item)) if self.generic_count(ParamOwner::Item(*item)) > 0 => {
            let name = self.resolution.type_name(self.package, *item);
            let params: Vec<String> =
              self.resolution.generics(self.package, ParamOwner::Item(*item)).iter().map(|param| param.name.name.clone()).collect();

            self.error(codes::WRONG_TYPE_ARGUMENTS, path.span, format!("`{name}` needs type arguments: `{name}<{}>`", params.join(", ")));

            return Type::Error;
          }
          Some(TypeRes::Item(TypeItem::Interface(id))) => {
            self.interface_types.push((path.span, *id));

            return self.type_ref(TypeItem::Interface(*id));
          }
          Some(TypeRes::Item(item)) => return self.type_ref(*item),
          Some(TypeRes::Param(owner, index)) => {
            let name = self.resolution.generics(self.package, *owner).get(*index as usize).map(|param| param.name.name.clone()).unwrap_or_default();

            return Type::Param(ParamRef { owner: *owner, index: *index, name });
          }
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
            // Inside an interface, `Self` is the type implementing it.
            Some(TypeItem::Interface(id)) => {
              Type::Param(ParamRef { owner: ParamOwner::Item(TypeItem::Interface(id)), index: SELF_PARAM, name: "Self".to_string() })
            }
            Some(owner) => self.type_ref(owner),
            None => {
              self.error(codes::UNRESOLVED_NAME, segment.span, "`Self` is only available inside a struct, enum or class");

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
      self.check_closure_cycle(target, value);

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

    if self.is_captured(id) {
      let name = self.resolution.locals(self.function).get(id.0 as usize).map(|info| info.name.clone()).unwrap_or_default();
      let diagnostic = self.captured_error(target.span, &name);

      self.diagnostics.push(diagnostic);
    }

    // `self = value` in a class method is reported by the flow analysis.
    if self.resolution.locals(self.function).get(id.0 as usize).is_some_and(|info| info.kind == LocalKind::SelfParam)
      && !matches!(self.owner, Some(TypeItem::Class(_) | TypeItem::Interface(_)))
    {
      self.mark_self_modified();
    }

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

  fn expect(&mut self, expr: &Expr, expected: &Type) {
    self.expect_because(expr, expected, None);
  }

  /// Like `expect`, pointing at what imposes the expected type when it is a mismatch.
  fn expect_because(&mut self, expr: &Expr, expected: &Type, because: Option<(Span, &str)>) {
    let actual = self.expr(expr, Some(expected));

    if self.fits(&actual, expected) {
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
      ExprKind::Lambda { params, body } => self.lambda(expr, params, body, expected),
      ExprKind::Zod => {
        if self.resolution.paths.contains_key(&expr.id) {
          self.diagnostics.push(
            Diagnostic::error(codes::WRONG_CALL_FORM, expr.span, "`zod` can only call a method of the parent class")
              .with_help("write `zod.method(...)`, or use `self` to reach the instance"),
          );
        }

        Type::Error
      }
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
        Some(Res::Fn(id)) => match &expr.kind {
          ExprKind::Path(path) => self.fn_value(expr, path, *id, expected),
          _ => Type::Error,
        },
        Some(Res::Extern(_) | Res::Builtin(_)) => {
          self.diagnostics.push(
            Diagnostic::error(codes::WRONG_CALL_FORM, expr.span, "C functions and prelude functions cannot be used as values")
              .with_help("wrap the call in a lambda, such as `(text) => println(text)`"),
          );

          Type::Error
        }
        Some(Res::Variant(variant)) => {
          let path = match &expr.kind {
            ExprKind::Path(path) => Some(path),
            _ => None,
          };

          self.variant_value(expr.span, *variant, None, expected, path)
        }
        Some(Res::Struct(_)) | None => Type::Error,
      },
      ExprKind::Call { callee, args } => match self.resolution.paths.get(&callee.id).copied() {
        Some(Res::Variant(variant)) if matches!(callee.kind, ExprKind::Path(_)) => {
          let path = match &callee.kind {
            ExprKind::Path(path) => Some(path),
            _ => None,
          };
          let ty = self.variant_value(expr.span, variant, Some(args), expected, path);

          self.typed.expr_types.insert(callee.id, ty.clone());

          ty
        }
        _ => self.call(expr, callee, args, expected),
      },
      ExprKind::Try(inner) => self.try_expr(inner),
      ExprKind::New { path, args } => self.new_expr(expr, path, args, expected),
      ExprKind::Match { scrutinee, arms } => self.match_expr(scrutinee, arms, expected),
      ExprKind::MethodCall { receiver, method, args } => self.method_call(expr, receiver, method, args),
      ExprKind::Field { base, name } => self.field(base, name),
      ExprKind::Index { base, index } => self.index(base, index),
      ExprKind::StructLit { path, fields } => self.struct_literal(expr, path, fields, expected),
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
        // In a lambda whose result is inferred, the first `return` decides it.
        if self.inferring_ret && self.ret == Type::Never {
          self.ret = match value {
            Some(value) => match self.expr(value, None) {
              Type::Never => Type::Never,
              ty => ty,
            },
            None => Type::Unit,
          };

          return Type::Never;
        }

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
      // Instances of related classes compare by identity.
      _ if self.fits(&lhs_ty, &rhs_ty) => rhs_ty,
      _ if self.fits(&rhs_ty, &lhs_ty) => lhs_ty,
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
      // Pointers compare by address, and class instances by identity.
      Type::Ptr { .. } | Type::Class(_) => matches!(op, BinaryOp::Eq | BinaryOp::Ne),
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
      _ if expected.is_some_and(|expected| self.fits(&then_ty, expected) && self.fits(&else_ty, expected)) => expected.cloned().unwrap_or(then_ty),
      _ if self.fits(&else_ty, &then_ty) => then_ty,
      _ if self.fits(&then_ty, &else_ty) => else_ty,
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

  fn call(&mut self, expr: &Expr, callee: &Expr, args: &[Expr], expected: Option<&Type>) -> Type {
    let sig = match (&callee.kind, self.resolution.paths.get(&callee.id).copied()) {
      (ExprKind::Path(path), Some(Res::Fn(id))) => return self.fn_call(expr, path, id, args, expected),
      (ExprKind::Path(_), Some(Res::Extern(id))) => self.extern_call_sig(callee, id),
      (ExprKind::Path(_), Some(Res::Builtin(Builtin::ErrorNew))) => {
        let ret = self.typed.error_types.map_or(Type::Error, |(error, _)| self.type_ref(TypeItem::Interface(error)));

        Some(FnSig { receiver: None, params: vec![Type::String], ret })
      }
      (ExprKind::Path(_), Some(Res::Builtin(builtin))) => {
        if builtin == Builtin::StringFromC {
          self.require_unsafe(callee.span, "reading a C string");
        }

        Some(builtin_sig(builtin))
      }
      (ExprKind::Path(_), None) => None,
      // A binding, field, call result or lambda of a function type.
      _ => return self.call_value(expr, callee, args),
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
    if receiver.kind == ExprKind::Zod {
      return self.zod_call(expr, receiver, method, args);
    }

    let receiver_ty = self.expr(receiver, None);
    let name = method.name.as_str();

    if let Some(ty) = self.ffi_method(expr, receiver, &receiver_ty, method, args) {
      return ty;
    }

    // A type parameter has the methods of its bounds.
    if let Type::Param(_) = &receiver_ty {
      let Some(function) = self.interface_method_of(&receiver_ty, name) else {
        self.error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{receiver_ty}`"));
        self.unchecked_args(args);

        return Type::Error;
      };

      self.typed.method_calls.insert(expr.id, function);

      return self.generic_method_call(expr, &receiver_ty, function, args);
    }

    let owner = match &receiver_ty {
      Type::Struct(ty) => TypeItem::Struct(ty.id),
      Type::Enum(ty) => TypeItem::Enum(ty.id),
      Type::Class(ty) => TypeItem::Class(ty.id),
      Type::Interface(ty) => TypeItem::Interface(ty.id),
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
    let found = match owner {
      TypeItem::Class(id) => self.class_method(id, name),
      TypeItem::Interface(id) => self.typed.interface_method(id, name, self.resolution, self.package).map(|(_, method)| method),
      _ => self.resolution.type_function(self.package, owner, name),
    }
    // A default method of an interface the type implements.
    .or_else(|| self.default_method(owner, name));
    let (Some(function), Some(decl)) = (found, found.and_then(|function| self.resolution.fn_decl(self.package, function))) else {
      // A field of a function type called like a method.
      if let Some(field_ty) = self.function_field(&receiver_ty, name) {
        return self.field_call(expr, field_ty, args);
      }

      let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{receiver_ty}`"));
      let functions = self.resolution.type_functions(self.package, owner);
      let methods: Vec<String> =
        functions.iter().filter(|function| function.self_param.is_some()).map(|function| function.name.name.clone()).collect();

      if let Some(similar) = sagitario_diagnostics::closest_match(name, methods.iter().map(String::as_str)) {
        diagnostic = diagnostic.with_help(format!("a method with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);
      self.unchecked_args(args);

      return Type::Error;
    };
    let Some(sig) = self.typed.sig(function).cloned() else { return Type::Error };

    match sig.receiver {
      Some(Receiver::Init) => {
        self.diagnostics.push(
          Diagnostic::error(codes::WRONG_CALL_FORM, method.span, format!("the initializer of `{receiver_ty}` cannot be called as a method"))
            .with_help(format!("create a new instance with `new {receiver_ty}(...)`")),
        );
      }
      None => {
        self.diagnostics.push(
          Diagnostic::error(codes::WRONG_CALL_FORM, method.span, format!("`{name}` is a static function of `{receiver_ty}`, not a method"))
            .with_label(decl.name.span, "declared without a `self` parameter here")
            .with_help(format!("call it as `{receiver_ty}::{name}(...)`")),
        );
      }
      Some(_) => {
        // An inherited method is private to the class that declares it.
        if decl.visibility == Visibility::Private && self.owner != self.resolution.owner(function) {
          self.diagnostics.push(
            Diagnostic::error(codes::PRIVATE_MEMBER, method.span, format!("the method `{name}` of `{receiver_ty}` is private"))
              .with_label(decl.name.span, "declared `prv` here"),
          );
        }

        // Whether the method modifies `self` is only known once every body is checked. Default
        // methods of interfaces receive the value as a reference instead.
        if matches!(owner, TypeItem::Struct(_) | TypeItem::Enum(_)) && !matches!(self.resolution.owner(function), Some(TypeItem::Interface(_))) {
          self.receiver_use(receiver, function, name);
        }

        self.typed.method_calls.insert(expr.id, function);
      }
    }

    self.generic_method_call(expr, &receiver_ty, function, args)
  }

  fn field(&mut self, base: &Expr, name: &Ident) -> Type {
    let base_ty = self.expr(base, None);
    let field_name = name.name.as_str();

    if let Type::Class(class) = &base_ty {
      return self.class_field(class.id, &base_ty, name);
    }

    if let Type::Interface(_) | Type::Param(_) = &base_ty {
      return self.interface_field_type(&base_ty, name);
    }

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
        diagnostic =
          diagnostic.with_help(format!("`{field_name}` is a method; call it with `{field_name}(...)`, or wrap it in a lambda to use it as a value"));
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

    field.ty.substitute(&Subst::of(ParamOwner::Item(TypeItem::Struct(owner.id)), owner.args.clone()))
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

  fn struct_literal(&mut self, expr: &Expr, path: &Path, fields: &[FieldInit], expected: Option<&Type>) -> Type {
    let id = match self.resolution.paths.get(&expr.id).copied() {
      Some(Res::Struct(id)) => id,
      Some(Res::Variant(variant)) => return self.named_variant_literal(path, variant, fields, expected),
      _ => {
        for field in fields {
          self.expr(&field.value, None);
        }

        return Type::Error;
      }
    };
    let def = self.typed.struct_def(id).cloned().unwrap_or(StructDef { name: String::new(), fields: Vec::new() });
    let mut seen: Vec<(&str, Span)> = Vec::new();
    // The type arguments of a generic struct come from the path, the expected type and the
    // field values.
    let owner = ParamOwner::Item(TypeItem::Struct(id));
    let mut unknowns = generic::Unknowns::new();
    let count = self.generic_count(owner);

    if count > 0 {
      let explicit = self.explicit_args(path).map(|(_, args)| args).filter(|args| self.check_arg_count(path.span, "this struct", count, args.len()));
      let identity = self.type_ref(TypeItem::Struct(id));

      unknowns.add(owner, count, explicit);

      if let Some(expected) = expected {
        unknowns.unify(&identity, expected);
      }
    }

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
        Some(field) => self.generic_arg(&init.value, &field.ty, &Subst::default(), &mut unknowns),
        None => {
          let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, init.name.span, format!("the struct `{}` has no field `{name}`", def.name));

          if let Some(similar) = sagitario_diagnostics::closest_match(name, def.fields.iter().map(|field| field.name.as_str())) {
            diagnostic = diagnostic.with_help(format!("a field with a similar name exists: `{similar}`"));
          }

          self.diagnostics.push(diagnostic);
          self.expr(&init.value, None);
        }
      }
    }

    let ty = if unknowns.is_empty() {
      self.struct_ref(id)
    } else {
      let subst = self.finish_inference(path.span, &format!("`{}`", def.name), unknowns);

      self.named_type(TypeItem::Struct(id), subst.params.into_iter().next().map(|(_, args)| args).unwrap_or_default())
    };
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
  fn variant_value(&mut self, span: Span, variant: VariantRef, args: Option<&[Expr]>, expected: Option<&Type>, written: Option<&Path>) -> Type {
    let path = self.variant_path(variant);

    // A variant of a generic enum infers the enum's type arguments.
    let generic = match variant.owner {
      EnumKind::User(id) if self.generic_count(ParamOwner::Item(TypeItem::Enum(id))) > 0 => Some(id),
      _ => None,
    };
    let mut unknowns = generic.map(|id| self.enum_unknowns(id, expected, written));

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

    if let (Some(id), Some(unknowns)) = (generic, unknowns.as_mut()) {
      let form_error = match (args, def.fields.is_empty(), def.named) {
        (Some(_), true, _) => Some(format!("`{path}` carries no data; write `{path}` without parentheses")),
        (None, false, false) => Some(format!("`{path}` carries data; write `{path}(...)`")),
        (_, false, true) => Some(format!("`{path}` has named fields; write `{path} {{ ... }}`")),
        _ => None,
      };

      if let Some(message) = form_error {
        self.error(codes::WRONG_CALL_FORM, span, message);
        self.unchecked_args(args.unwrap_or_default());
      } else if let Some(args) = args {
        self.generic_args(span, "variant", &def.field_types(), args, &Subst::default(), unknowns);
      }

      return self.finish_enum(span, id, unknowns);
    }
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
  fn named_variant_literal(&mut self, path: &Path, variant: VariantRef, fields: &[FieldInit], expected: Option<&Type>) -> Type {
    let name = self.variant_path(variant);
    let ty = match variant.owner {
      EnumKind::User(id) => self.type_ref(TypeItem::Enum(id)),
      EnumKind::Option | EnumKind::Result => Type::Error,
    };
    let mut unknowns = match variant.owner {
      EnumKind::User(id) if self.generic_count(ParamOwner::Item(TypeItem::Enum(id))) > 0 => Some(self.enum_unknowns(id, expected, Some(path))),
      _ => None,
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
        Some((_, field_ty)) => match unknowns.as_mut() {
          Some(unknowns) => self.generic_arg(&init.value, &field_ty.clone(), &Subst::default(), unknowns),
          None => self.expect(&init.value, &field_ty.clone()),
        },
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

    match (variant.owner, unknowns) {
      (EnumKind::User(id), Some(mut unknowns)) => self.finish_enum(path.span, id, &mut unknowns),
      _ => ty,
    }
  }

  /// A method of `Option` or `Result` (docs/spec/07-errors.md).
  fn builtin_method(&mut self, expr: &Expr, receiver_ty: &Type, method: &Ident, args: &[Expr]) -> Type {
    if let Some(ty) = self.closure_method(expr, receiver_ty, method, args) {
      return ty;
    }

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
        // Like Rust's `?` into `Box<dyn Error>`, a `String` error becomes an `Error`.
        let into_error =
          **error == Type::String && self.typed.error_types.is_some_and(|(id, _)| matches!(&**expected, Type::Interface(e) if e.id == id));

        if !self.fits(error, expected) && !into_error {
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
    let first_ty = self.expr(first, expected_elem.as_ref());
    // With a known element type, elements may be instances of its subclasses.
    let mut elem_ty = match &expected_elem {
      Some(expected) if self.fits(&first_ty, expected) && first_ty != Type::Never => expected.clone(),
      _ => first_ty,
    };

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
      pub fn set(self, x: i32) { self.x = x; }
    }";

  fn with_point(body: &str) -> Vec<&'static str> {
    diagnostics(&format!("{POINT}\npub fn main() {{ {body} }}"))
  }

  #[test]
  fn checks_struct_members() {
    assert!(with_point("var p = Point::new(1); p.set(p.get() + 1); p.x += 1; const a = [p; 2]; const _ = a[1].x;").is_empty());
    assert_eq!(with_point("const p = Point::new(1); p.set(2);"), vec!["E0313"]);
    assert!(with_point("const p = Point::new(1); const _ = p.get();").is_empty());
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
