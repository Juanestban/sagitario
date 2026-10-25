//! SIR (Sagitario Intermediate Representation) and lowering from the type-checked AST
//! (docs/architecture.md#sir).
//!
//! SIR is structured (`If`/`Loop` instead of a control-flow graph) because it is emitted as C.
//! Every intermediate value lives in a numbered local; fields and array elements are reached
//! through places. ARC is explicit: locals of a *managed* type (one that contains a `String` or
//! another reference: a class instance, a `StringBuilder`, a `CString`) own one reference to each
//! one inside, released at the end of their block, on `break`/`continue`/`return`, or when
//! overwritten. Temporaries are released at the end of the statement or branch that created them,
//! so the set of pending releases always matches the values created on the current path.
//!
//! A class instance is a reference to a heap object; its fields are places behind it. Any call
//! may change a `var` field through another reference, so reading a managed value from a field of
//! an instance takes an owned copy instead of borrowing it. Methods that some subclass overrides
//! (or abstract ones) are called through the instance's vtable; every initializer first points
//! the vtable at its own class (after `zod.new(...)`), so methods called during the parent's
//! initialization run the parent's implementations. A `weak` field stores a bare reference that
//! counts as weak: reading it (`WeakLoad`) gives an owned `Option`, `None` once the instance is
//! destroyed, and assigning it goes through `WeakStore`.
//!
//! Enums, `Option` and `Result` values carry a tag (the variant's index) and the fields of their
//! variant; `match` and `?` (in `matching`) test the tag before reading those fields.
//!
//! Generic code is lowered per instance (`mono`): the types of a template's expressions are
//! substituted with the instance's type arguments, so every SIR type is concrete.
//!
//! Lowering only runs on programs without errors, so an inconsistency it finds is an internal
//! compiler error, reported as `LowerError`.

mod closures;
mod collections;
mod interfaces;
mod matching;
mod mono;
mod text;

pub use interfaces::{FieldAt, Itable, ItableEntry};
pub use mono::ClassInstance;

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind, FnDecl, Package, Stmt as AstStmt, TemplatePart, UnaryOp};
use sagitario_diagnostics::Diagnostics;
use sagitario_resolve::{Builtin, ClassId, ExternId, FnId, ParamOwner, Res, Resolution, TypeItem};

use crate::interfaces::Registry;
use sagitario_span::{SourceMap, Span};
use sagitario_types::{BuiltinMethod, ClassRef, EnumRef, FloatTy, IntTy, Receiver, StructRef, Subst, Type, TypedModule};

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
  /// Indexed by `FnId`: the program's functions (templates are placeholders), then the instances
  /// of templates.
  pub functions: Vec<Function>,
  pub layouts: Layouts,
  /// The class types the program uses, with their type arguments.
  pub classes: Vec<ClassInstance>,
  /// The types captured by each closure environment `sag_envN`.
  pub envs: Vec<Vec<Type>>,
  /// The named functions used as values, which get a wrapper taking an environment.
  pub wrapped: Vec<FnId>,
  /// `main`; a library has none, and C calls its exported functions instead.
  pub entry: Option<FnId>,
  /// Path of each source file, by `FileId` index, for `#line` directives.
  pub source_paths: Vec<String>,
  /// The C functions of the `extern` blocks, indexed by `ExternId`.
  pub externs: Vec<ExternDecl>,
  /// The itables of the interface values the program creates.
  pub itables: Vec<Itable>,
  /// The struct and enum types boxed to become interface values.
  pub boxes: Vec<Type>,
  /// How to get the message of an `Err` returned by `main`, when its error is not a `String`.
  pub main_error: Option<MainError>,
}

/// The message of `main`'s error: converted to `Error` with `itable` if its type is concrete,
/// then `message()` is the method `slot` of the itable reached through `path`.
#[derive(Debug, Clone, PartialEq)]
pub struct MainError {
  pub itable: Option<usize>,
  pub path: Vec<usize>,
  pub slot: usize,
}

/// A C function the program calls.
#[derive(Debug, Clone, PartialEq)]
pub struct ExternDecl {
  pub symbol: String,
  pub params: Vec<Type>,
  pub ret: Type,
  /// A function of the runtime, declared by its header and called directly.
  pub runtime: bool,
}

/// The fields of the program's structs and enums, with type parameters in generic ones.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layouts {
  /// Indexed by `StructId`.
  pub structs: Vec<StructLayout>,
  /// Indexed by `EnumId`.
  pub enums: Vec<EnumLayout>,
}

impl Layouts {
  /// Whether values of `ty` hold references, so copies must be retained and released.
  pub fn is_managed(&self, ty: &Type) -> bool {
    match ty {
      Type::String | Type::StringBuilder | Type::CString | Type::Class(_) | Type::Interface(_) | Type::Function { .. } | Type::List(_) => true,
      Type::Array(elem, _) => self.is_managed(elem),
      Type::Struct(inner) => self.struct_fields(inner).iter().any(|field| self.is_managed(field)),
      Type::Enum(_) | Type::Option(_) | Type::Result(..) => self.variants(ty).iter().flatten().flatten().any(|field| self.is_managed(field)),
      _ => false,
    }
  }

  /// The field types of each variant of an enum, `Option` or `Result` type.
  pub fn variants(&self, ty: &Type) -> Option<Vec<Vec<Type>>> {
    match ty {
      Type::Enum(inner) => {
        let subst = enum_subst(inner);

        self
          .enums
          .get(inner.id.0 as usize)
          .map(|layout| layout.variants.iter().map(|fields| fields.iter().map(|field| field.substitute(&subst)).collect()).collect())
      }
      Type::Option(value) => Some(vec![Vec::new(), vec![value.as_ref().clone()]]),
      Type::Result(value, error) => Some(vec![vec![value.as_ref().clone()], vec![error.as_ref().clone()]]),
      _ => None,
    }
  }

  /// The field types of a struct type, with its type arguments.
  pub fn struct_fields(&self, ty: &StructRef) -> Vec<Type> {
    let subst = Subst::of(ParamOwner::Item(TypeItem::Struct(ty.id)), ty.args.clone());

    self.structs.get(ty.id.0 as usize).map(|layout| layout.fields.iter().map(|field| field.substitute(&subst)).collect()).unwrap_or_default()
  }
}

fn enum_subst(ty: &EnumRef) -> Subst {
  Subst::of(ParamOwner::Item(TypeItem::Enum(ty.id)), ty.args.clone())
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
  /// A generic template, only used through its instances: it has no code.
  pub template: bool,
  /// A lambda: its first parameter (local 0) is the environment, a `sag_object *`.
  pub closure: bool,
  /// The first locals are the parameters, starting with `self` in methods.
  pub param_count: usize,
  /// A method that modifies a struct's or enum's `self`: local 0 refers to the caller's value
  /// instead of holding a copy.
  pub self_by_ref: bool,
  /// The C symbol of an `extern "C" fn`.
  pub export: Option<String>,
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
  /// The field at position `index` among those of class instance `class` (the class declaring
  /// it), in the object a reference points to (of that class or a subclass).
  ClassField { class: usize, index: usize },
  /// The capture at `index` of closure environment `env`, behind the environment pointer.
  EnvField { env: usize, index: usize },
  /// A bounds-checked element of an array of length `len`.
  Index { index: Operand, len: u64 },
  /// A bounds-checked element, of type `elem`, of the list a reference points to.
  ListElem { index: Operand, elem: Type },
  /// The tag of an enum value: the index of its variant, as `u32`.
  Tag,
  /// A field of the given variant of an enum value; only valid when the tag matches.
  VariantField { variant: u32, field: usize },
  /// The value a pointer points to; a null pointer panics.
  Deref,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
  /// The following statements come from this 1-based source line.
  Line {
    file: u32,
    line: u32,
  },
  Assign {
    dest: Local,
    value: Rvalue,
  },
  /// Stores into a field or element; like `Assign`, it does not release the previous value.
  AssignPlace {
    dest: Place,
    value: Rvalue,
  },
  /// Makes the `weak` field `dest` refer weakly to the instance in the `Option` `value`,
  /// dropping its previous weak reference.
  WeakStore {
    dest: Place,
    value: Operand,
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
  /// `for var in start..end` (or `..=`); `start`, `end` and `step` (a `usize`, 1 if absent)
  /// are evaluated once, before the loop. `reverse` counts down from the last value.
  ForRange {
    var: Local,
    ty: IntTy,
    start: Operand,
    end: Operand,
    inclusive: bool,
    reverse: bool,
    step: Option<Operand>,
    body: Vec<Stmt>,
  },
  /// Appends an element (already retained) to a list.
  ListPush {
    list: Operand,
    value: Operand,
  },
  /// Inserts an element (already retained) at a position of a list, moving the following ones;
  /// a position past the end panics.
  ListInsert {
    list: Operand,
    index: Operand,
    value: Operand,
  },
  /// Releases every element of a list and empties it.
  ListClear(Operand),
  /// Reverses the order of the elements of a list.
  ListReverse(Operand),
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
  /// Makes the instance in `object` dispatch through the vtable of class instance `class`.
  SetVtable {
    object: Local,
    class: usize,
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
  /// A new object of class instance `N` with one reference and every field zeroed, which its
  /// initializer then assigns.
  Alloc(usize),
  /// The `Option` in a `weak` field: `Some` with a new reference while the instance is alive.
  WeakLoad(Place),
  /// A closure of the lambda `function`, with a new environment `env` holding copies of the
  /// captures (retained), or no environment.
  Closure {
    function: FnId,
    env: Option<usize>,
    captures: Vec<Operand>,
  },
  /// A named function as a value: its wrapper, without environment.
  FnValue(FnId),
  /// A new empty list of the destination's type.
  ListNew,
  /// The number of elements of a list, a `usize`.
  ListLen(Operand),
  /// Moves the last element out of a list, which must not be empty.
  ListPop(Operand),
  /// Moves the element at a position out of a list, moving the following ones; a position out
  /// of bounds panics.
  ListRemove {
    list: Operand,
    index: Operand,
  },
  /// A new list with the elements of an array of type `array_ty` (already retained for it).
  ListFromArray {
    array: Operand,
    array_ty: Type,
  },
  /// A value of a class, struct or enum as an interface value with the given itable: a new
  /// reference to the instance, or a box holding a copy of the value.
  ToDyn {
    value: Operand,
    itable: usize,
  },
  /// An interface value as a value of an ancestor interface, whose itable is reached through
  /// `path`; a new reference.
  DynUpcast {
    value: Operand,
    path: Vec<usize>,
  },
}

#[derive(Debug, Clone, PartialEq)]
pub enum TemplatePiece {
  Text(String),
  Value(Type, Operand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Callee {
  Fn(FnId),
  /// The method in `slot` of the vtable of the first argument, an instance; `function` is the
  /// method the call names, which gives the signature.
  Virtual {
    slot: usize,
    function: FnId,
  },
  /// The function value given as the first argument, called with its environment and the other
  /// arguments, of types `params`.
  Closure {
    params: Vec<Type>,
    ret: Type,
  },
  /// The entry `slot` of the itable reached through `path` from the first argument, an
  /// interface value, which it receives followed by arguments of types `params`.
  Dyn {
    path: Vec<usize>,
    slot: usize,
    params: Vec<Type>,
    ret: Type,
  },
  /// A C function of an `extern` block.
  Extern(ExternId),
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
  /// The address of a place: the receiver of a method that modifies `self`.
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

/// Lowers the program; errors found while instantiating generic code (an endless series of
/// instances) are added to `diagnostics`.
pub fn lower(
  package: &Package,
  resolution: &Resolution,
  typed: &TypedModule,
  sources: &SourceMap,
  diagnostics: &mut Diagnostics,
) -> LResult<Program> {
  let entry = resolution.main;

  if entry.is_none() && !package.library {
    return error("program without `main`");
  }

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
  let mut registry = Registry { first_instance: resolution.functions.len() as u32, ..Registry::default() };

  for (id, decl) in resolution.fn_decls(package) {
    if mono::is_template(resolution, typed, id) {
      functions.push(Function {
        name: decl.name.name.clone(),
        template: true,
        closure: false,
        param_count: 0,
        self_by_ref: false,
        export: None,
        ret: Type::Unit,
        locals: Vec::new(),
        body: Vec::new(),
      });

      continue;
    }

    let mut cx = Context { package, resolution, typed, layouts: &layouts, sources, registry: &mut registry };

    functions.push(lower_function(id, decl, Subst::default(), &mut cx)?);
  }

  let main_error = entry.and_then(|entry| main_error(entry, package, resolution, typed, &layouts, sources, &mut registry));
  let mut instances: Vec<Option<Function>> = Vec::new();

  // Lowering an instance may need more instances.
  while let Some(index) = registry.pending.pop() {
    let Some(instance) = registry.instances.get(index).cloned() else { continue };
    let Some(decl) = resolution.fn_decl(package, instance.template) else { continue };
    let mut cx = Context { package, resolution, typed, layouts: &layouts, sources, registry: &mut registry };
    let function = lower_function(instance.template, decl, instance.subst, &mut cx)?;

    if instances.len() <= index {
      instances.resize(index + 1, None);
    }

    if let Some(slot) = instances.get_mut(index) {
      *slot = Some(function);
    }
  }

  for (index, function) in std::mem::take(&mut registry.lambdas) {
    if instances.len() <= index {
      instances.resize(index + 1, None);
    }

    if let Some(slot) = instances.get_mut(index) {
      *slot = Some(function);
    }
  }

  if instances.len() < registry.instances.len() {
    instances.resize(registry.instances.len(), None);
  }

  for function in instances {
    functions.push(function.ok_or_else(|| LowerError("an instance was not lowered".to_string()))?);
  }

  for diagnostic in std::mem::take(&mut registry.diagnostics) {
    diagnostics.push(diagnostic);
  }

  let source_paths = sources.files().iter().map(|source| source.path().display().to_string()).collect();
  let mut externs = Vec::new();

  for (id, decl) in resolution.extern_decls(package) {
    let sig = typed.extern_sig(id).ok_or_else(|| LowerError(format!("no signature for the C function `{}`", decl.name.name)))?;

    let runtime = resolution.is_runtime_extern(package, id);

    externs.push(ExternDecl { symbol: decl.name.name.clone(), params: sig.params.clone(), ret: sig.ret.clone(), runtime });
  }

  Ok(Program {
    functions,
    layouts,
    classes: registry.classes,
    envs: registry.envs,
    wrapped: registry.wrapped,
    entry,
    source_paths,
    externs,
    itables: registry.itables,
    boxes: registry.boxes,
    main_error,
  })
}

/// What lowering a function needs from the whole program.
struct Context<'a> {
  package: &'a Package,
  resolution: &'a Resolution,
  typed: &'a TypedModule,
  layouts: &'a Layouts,
  sources: &'a SourceMap,
  registry: &'a mut Registry,
}

/// How the C `main` gets the message of an error that `main` returns, when it is not a `String`.
fn main_error(
  entry: FnId,
  package: &Package,
  resolution: &Resolution,
  typed: &TypedModule,
  layouts: &Layouts,
  sources: &SourceMap,
  registry: &mut Registry,
) -> Option<MainError> {
  let Some(Type::Result(_, error_ty)) = typed.sig(entry).map(|sig| sig.ret.clone()) else { return None };
  let (error, _) = typed.error_types?;
  let slot = typed.interface_def(error)?.methods.first().and(Some(0))?;

  match error_ty.as_ref() {
    Type::String => None,
    Type::Interface(interface) => Some(MainError { itable: None, path: typed.interface_path(interface.id, error)?, slot }),
    concrete => {
      let mut cx = Context { package, resolution, typed, layouts, sources, registry };
      let span = resolution.fn_decl(package, entry)?.name.span;
      let mut lowering = FnLowering::new(&mut cx, Type::Unit, Vec::new(), Subst::default(), span);
      let interface = typed.implemented_ref(concrete, error)?;

      Some(MainError { itable: Some(lowering.itable(&interface, concrete)), path: Vec::new(), slot })
    }
  }
}

fn typed_class_name(typed: &TypedModule, owner: TypeItem) -> String {
  match owner {
    TypeItem::Class(id) => typed.class_def(id).map(|def| def.name.clone()).unwrap_or_default(),
    _ => String::new(),
  }
}

/// The `zod.new(...)` call that starts an initializer.
fn first_zod_new(decl: &FnDecl, typed: &TypedModule) -> Option<sagitario_ast::NodeId> {
  let first = match decl.body.stmts.first() {
    Some(AstStmt::Expr(expr)) => expr,
    Some(_) => return None,
    None => decl.body.tail.as_deref()?,
  };

  (matches!(&first.kind, ExprKind::MethodCall { receiver, .. } if receiver.kind == ExprKind::Zod) && typed.zod_calls.contains(&first.id))
    .then_some(first.id)
}

/// Lowers a function, or the instance of a template with the type arguments in `subst`.
fn lower_function(id: FnId, decl: &FnDecl, subst: Subst, cx: &mut Context) -> LResult<Function> {
  let (resolution, typed) = (cx.resolution, cx.typed);
  let sig = typed.sig(id).ok_or_else(|| LowerError(format!("no signature for `{}`", decl.name.name)))?.substitute(&subst);
  let locals = typed.local_types.get(id.0 as usize).ok_or_else(|| LowerError("missing local types".to_string()))?;
  let locals: Vec<Type> = locals.iter().map(|ty| ty.substitute(&subst)).collect();
  let mut lowering = FnLowering::new(cx, sig.ret.clone(), locals.clone(), subst, decl.name.span);

  lowering.source_fn = id;

  for ty in &locals {
    lowering.register_types(ty);
  }

  if decl.is_abstract {
    // Only reachable while the class's own initializer runs (ADR-0029).
    let owner = resolution.owner(id).map_or_else(String::new, |owner| typed_class_name(typed, owner));

    lowering.emit(Stmt::Panic(PanicMessage::Static(format!("called the abstract method `{}` of `{owner}` while initializing it", decl.name.name))));
  } else if let (Some(Receiver::Init), Some(TypeItem::Class(class))) = (sig.receiver, resolution.owner(id)) {
    // The instance dispatches as this class once its parent part is initialized.
    let class = lowering.own_class(class);

    match first_zod_new(decl, typed) {
      Some(call) => lowering.vtable_after = Some((call, class)),
      None => lowering.emit(Stmt::SetVtable { object: Local(0), class }),
    }
  }

  if decl.is_abstract {
    // The panic ends the function.
  } else if sig.ret == Type::Unit {
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
    template: false,
    closure: false,
    param_count: decl.params.len() + usize::from(sig.receiver.is_some()),
    self_by_ref: sig.receiver == Some(Receiver::Var),
    export: decl.extern_c.then(|| decl.export.as_ref().map_or_else(|| decl.name.name.clone(), |export| export.symbol.clone())),
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
  package: &'a Package,
  registry: &'a mut Registry,
  resolution: &'a Resolution,
  typed: &'a TypedModule,
  layouts: &'a Layouts,
  sources: &'a SourceMap,
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
  /// In an initializer, the `zod.new(...)` after which the instance takes this class's vtable
  /// (a class instance).
  vtable_after: Option<(sagitario_ast::NodeId, usize)>,
  /// The type arguments of the instance being lowered; empty outside generic code.
  subst: Subst,
  /// Where the code being lowered is, for errors found while instantiating.
  span: Span,
  /// The source function whose bindings are lowered (the enclosing one for a lambda).
  source_fn: FnId,
  /// Inside a lambda, the SIR local of each binding of the source function it uses.
  local_map: Option<std::collections::HashMap<u32, Local>>,
}

impl<'a> FnLowering<'a> {
  /// The class instance of a class used inside its own declaration, with the instance's type
  /// arguments.
  fn own_class(&mut self, id: ClassId) -> usize {
    let name = self.typed.class_def(id).map(|def| def.name.clone()).unwrap_or_default();
    let args =
      self.subst.params.iter().find(|(owner, _)| *owner == ParamOwner::Item(TypeItem::Class(id))).map(|(_, args)| args.clone()).unwrap_or_default();

    self.class_instance(&ClassRef { id, name, args })
  }

  fn new(cx: &'a mut Context, ret: Type, locals: Vec<Type>, subst: Subst, span: Span) -> FnLowering<'a> {
    FnLowering {
      subst,
      span,
      source_fn: FnId(0),
      local_map: None,
      package: cx.package,
      registry: cx.registry,
      resolution: cx.resolution,
      typed: cx.typed,
      layouts: cx.layouts,
      sources: cx.sources,
      ret,
      locals,
      buffers: vec![Vec::new()],
      scopes: Vec::new(),
      pending: Vec::new(),
      loops: Vec::new(),
      vtable_after: None,
    }
  }
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
    self.register_types(&ty);
    self.locals.push(ty);

    Local(self.locals.len() as u32 - 1)
  }

  fn local_type(&self, local: Local) -> Type {
    self.locals.get(local.0 as usize).cloned().unwrap_or(Type::Error)
  }

  fn type_of(&self, expr: &Expr) -> LResult<Type> {
    let ty = self.typed.expr_type(expr).ok_or_else(|| LowerError(format!("expression without a type at byte {}", expr.span.start)))?;

    Ok(ty.substitute(&self.subst))
  }

  fn binding(&self, id: sagitario_ast::NodeId) -> LResult<Local> {
    self.resolution.bindings.get(&id).map(|local| self.local_id(*local)).ok_or_else(|| LowerError("unresolved binding".to_string()))
  }

  fn line(&self, span: Span) -> Stmt {
    let line = self.sources.get(span.file).map_or(1, |source| source.line_col(span.start).line);

    Stmt::Line { file: span.file.index() as u32, line }
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

  /// Like `store`, converting `value` from type `from` to the type of `dest`.
  fn store_from(&mut self, dest: Local, value: Operand, from: &Type) {
    let to = self.local_type(dest);
    let value = self.convert(value, from, &to);

    self.store(dest, value);
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
      let line = self.line(tail.span);

      self.emit(line);
      self.span = tail.span;

      let mark = self.pending.len();
      let value = self.expr(tail)?;

      self.after_expr(tail.id);

      let tail_ty = self.type_of(tail)?;

      if let Some(dest) = dest
        && !is_valueless(&tail_ty)
      {
        self.store_from(dest, value, &tail_ty);
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
    let span = match stmt {
      AstStmt::Let(stmt) => stmt.span,
      AstStmt::Assign { span, .. } => *span,
      AstStmt::Expr(expr) => expr.span,
    };
    let line = self.line(span);

    self.emit(line);
    self.span = span;

    let mark = self.pending.len();

    match stmt {
      AstStmt::Let(stmt) => {
        let local = self.binding(stmt.id)?;
        let ty = self.local_type(local);
        let managed = self.managed(&ty);

        match &stmt.init {
          Some(init) => {
            let value = self.expr(init)?;
            let init_ty = self.type_of(init)?;

            if !is_valueless(&init_ty) {
              self.store_from(local, value, &init_ty);
            }
          }
          None if managed => self.emit(Stmt::Reset(local)),
          None => {}
        }

        if managed && let Some(scope) = self.scopes.last_mut() {
          scope.push(local);
        }
      }
      AstStmt::Assign { target, op, value, .. } if self.is_interface_field(target) => {
        let ExprKind::Field { base, name } = &target.kind else { return error("interface field without a base") };
        let Type::Interface(interface) = self.type_of(base)? else { return error("interface field of another type") };
        let ty = self.type_of(target)?;
        let receiver = self.expr(base)?;
        let receiver = self.owned_copy(Type::Interface(interface.clone()), receiver);
        let mut new_value = self.operand_as(value, &ty)?;

        if let Some(op) = op {
          let getter = self.dyn_field(&interface, &name.name, false)?;
          let current = self.call(getter, vec![receiver.clone()], ty.clone());

          new_value = self.binary(*op, ty, current, new_value);
        }

        self.set_dyn_field(receiver, &interface, &name.name, new_value)?;
      }
      AstStmt::Assign { target, value, .. } if self.is_weak_field(target) => {
        let value = self.expr(value)?;
        let dest = self.field_place(target)?;

        self.emit(Stmt::WeakStore { dest, value });
      }
      AstStmt::Assign { target, op, value, .. }
        if matches!(target.kind, ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::Unary { op: UnaryOp::Deref, .. }) =>
      {
        let value_ty = self.type_of(value)?;
        let value = self.expr(value)?;
        let place = self.place(target)?;
        let ty = place.ty.clone();
        let value = self.convert(value, &value_ty, &ty);
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
        let local = self.local_id(id);
        let ty = self.local_type(local);
        let value = self.operand_as(value, &ty)?;
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
        self.after_expr(expr.id);
      }
    }

    self.release_pending_from(mark);

    Ok(())
  }

  /// Points the instance at its class's vtable right after the initializer's `zod.new(...)`.
  fn after_expr(&mut self, id: sagitario_ast::NodeId) {
    if let Some((call, class)) = self.vtable_after
      && call == id
    {
      self.emit(Stmt::SetVtable { object: Local(0), class });
      self.vtable_after = None;
    }
  }

  /// The place a binding, field or element expression designates. Its base is lowered first;
  /// a base that is not a place (a call result, a literal) is stored in a temporary.
  fn place(&mut self, expr: &Expr) -> LResult<Place> {
    let ty = self.type_of(expr)?;

    match &expr.kind {
      ExprKind::Path(_) => match self.resolution.paths.get(&expr.id) {
        Some(Res::Local(id)) => Ok(Place { local: self.local_id(*id), projections: Vec::new(), ty }),
        other => error(format!("place of unsupported path {other:?}")),
      },
      ExprKind::Field { base, name } if self.is_interface_field(expr) => {
        let Type::Interface(interface) = self.type_of(base)? else { return error("interface field of another type") };
        let receiver = self.expr(base)?;
        let getter = self.dyn_field(&interface, &name.name, false)?;
        // The getter returns a new reference.
        let value = self.call(getter, vec![receiver], ty.clone());

        self.operand_place(value, ty)
      }
      ExprKind::Field { .. } => {
        let place = self.field_place(expr)?;

        if self.is_weak_field(expr) {
          // The field holds no reference: reading it takes one, if the instance is alive.
          let value = self.new_value(ty.clone(), Rvalue::WeakLoad(place));

          return self.operand_place(value, ty);
        }

        Ok(place)
      }
      ExprKind::Index { base, index } if matches!(self.type_of(base)?, Type::List(_)) => self.list_elem_place(base, index, ty),
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
      ExprKind::Unary { op: UnaryOp::Deref, operand } => {
        let pointer_ty = self.type_of(operand)?;
        let pointer = self.expr(operand)?;
        let mut place = self.operand_place(pointer, pointer_ty)?;

        place.projections.push(Projection::Deref);
        place.ty = ty;

        Ok(place)
      }
      _ => {
        let value = self.expr(expr)?;

        self.operand_place(value, ty)
      }
    }
  }

  /// Whether an expression is a field reached through an interface value.
  fn is_interface_field(&self, expr: &Expr) -> bool {
    let ExprKind::Field { base, .. } = &expr.kind else { return false };

    matches!(self.typed.expr_type(base), Some(Type::Interface(_)))
  }

  /// Whether an expression is a `weak` field of a class instance.
  fn is_weak_field(&self, expr: &Expr) -> bool {
    let ExprKind::Field { base, name } = &expr.kind else { return false };

    match self.typed.expr_type(base) {
      Some(Type::Class(owner)) => self.typed.class_field(owner.id, &name.name).is_some_and(|(_, _, field)| field.weak),
      _ => false,
    }
  }

  /// The place of a field as stored, without loading a `weak` one.
  fn field_place(&mut self, expr: &Expr) -> LResult<Place> {
    let ty = self.type_of(expr)?;
    let ExprKind::Field { base, name } = &expr.kind else {
      return error("field place of an expression that is not a field");
    };

    let projection = match self.type_of(base)? {
      Type::Struct(owner) => self.typed.field_index(owner.id, &name.name).map(Projection::Field),
      Type::Class(owner) => self
        .typed
        .class_field_type(&owner, &name.name)
        .map(|(declaring, index, _)| Projection::ClassField { class: self.class_instance(&declaring), index }),
      _ => return error("field of a value that is not a struct or a class instance"),
    };
    let projection = projection.ok_or_else(|| LowerError(format!("unknown field `{}`", name.name)))?;
    let mut place = self.base_place(base)?;

    // An instance reached through another place stays alive for the statement, even if a
    // call in it replaces that place.
    if matches!(projection, Projection::ClassField { .. }) && !place.projections.is_empty() {
      let base_ty = place.ty.clone();

      place = match self.owned_copy(base_ty.clone(), Operand::Place(place)) {
        Operand::Local(local) => Place { local, projections: Vec::new(), ty: base_ty },
        _ => return error("instance reference without a local"),
      };
    }

    place.projections.push(projection);
    place.ty = ty;

    Ok(place)
  }

  fn base_place(&mut self, base: &Expr) -> LResult<Place> {
    match &base.kind {
      ExprKind::Path(_) | ExprKind::Field { .. } | ExprKind::Index { .. } | ExprKind::Unary { op: UnaryOp::Deref, .. } => self.place(base),
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
      ExprKind::Lambda { body, .. } => self.lambda(expr, body, &ty),
      ExprKind::Path(_) | ExprKind::Zod => match self.resolution.paths.get(&expr.id) {
        Some(Res::Local(id)) => Ok(Operand::Local(self.local_id(*id))),
        Some(Res::Fn(id)) => Ok(self.fn_value(expr, *id, &ty)),
        Some(Res::Variant(variant)) => Ok(self.variant(ty, variant.index, Vec::new())),
        other => error(format!("unsupported path operand {other:?}")),
      },
      ExprKind::Try(inner) => self.try_expr(inner),
      ExprKind::New { .. } if ty == Type::StringBuilder => Ok(self.new_string_builder()),
      ExprKind::New { .. } if matches!(ty, Type::List(_)) => Ok(self.new_value(ty, Rvalue::ListNew)),
      ExprKind::Call { callee, args } if self.is_list_from(callee) => self.list_from(&ty, args),
      ExprKind::New { args, .. } => self.new_instance(&ty, args),
      ExprKind::Match { scrutinee, arms } => self.match_expr(&ty, scrutinee, arms),
      ExprKind::Field { .. } | ExprKind::Index { .. } => {
        let place = self.place(expr)?;

        if self.managed(&ty) {
          // A field of an instance or an element of a list can change during the statement
          // through another reference.
          if place.projections.iter().any(|projection| matches!(projection, Projection::ClassField { .. } | Projection::ListElem { .. })) {
            return Ok(self.owned_copy(ty, Operand::Place(place)));
          }

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
        let callee = match self.resolution.paths.get(&callee.id).copied() {
          Some(Res::Fn(id)) => {
            let subst = self.call_subst(expr.id, id, None);
            let params = self.typed.sig(id).map(|sig| sig.substitute(&subst).params).unwrap_or_default();
            let function = self.instance(id, subst);
            let mut operands = Vec::new();

            for (arg, param) in args.iter().zip(&params) {
              operands.push(self.operand_as(arg, param)?);
            }

            return Ok(self.call(Callee::Fn(function), operands, ty));
          }
          Some(Res::Extern(id)) => Callee::Extern(id),
          Some(Res::Builtin(Builtin::ErrorNew)) => {
            let [text] = args.as_slice() else { return error("`Error` without one argument") };
            let text = self.operand_as(text, &Type::String)?;

            return self.message_error(text);
          }
          Some(Res::Builtin(Builtin::CStringNew)) => {
            let [text] = args.as_slice() else { return error("`CString::new` without one argument") };
            let text = self.operand_as(text, &Type::String)?;

            return Ok(self.cstring_new(&ty, text));
          }
          Some(Res::Builtin(builtin)) => Callee::Builtin(builtin),
          Some(Res::Variant(variant)) => {
            let fields = self.variant_fields(&ty, variant.index);
            let mut operands = Vec::new();

            for (arg, field) in args.iter().zip(&fields) {
              operands.push(self.operand_as(arg, field)?);
            }

            return Ok(self.variant(ty, variant.index, operands));
          }
          // A binding, field, call result or lambda of a function type.
          _ => return self.call_value(callee, args),
        };
        let params = match &callee {
          Callee::Fn(id) | Callee::Virtual { function: id, .. } => self.typed.sig(*id).map(|sig| sig.params.clone()),
          Callee::Extern(id) => self.typed.extern_sig(*id).map(|sig| sig.params.clone()),
          Callee::Builtin(builtin) => Some(sagitario_types::builtin_sig(*builtin, &self.package.target).params),
          Callee::Dyn { params, .. } | Callee::Closure { params, .. } => Some(params.clone()),
          Callee::Runtime { .. } => None,
        }
        .unwrap_or_default();
        let mut operands = Vec::new();

        for (arg, param) in args.iter().zip(&params) {
          operands.push(self.operand_as(arg, param)?);
        }

        Ok(self.call(callee, operands, ty))
      }
      ExprKind::MethodCall { receiver, method, args } if self.typed.field_calls.contains(&expr.id) => self.field_call(receiver, &method.name, args),
      ExprKind::MethodCall { receiver, args, .. } if self.typed.builtin_methods.contains_key(&expr.id) => {
        let method = self.typed.builtin_methods.get(&expr.id).cloned().ok_or_else(|| LowerError("missing built-in method".to_string()))?;

        match method {
          BuiltinMethod::Map | BuiltinMethod::MapErr | BuiltinMethod::AndThen | BuiltinMethod::UnwrapOrElse | BuiltinMethod::Filter => {
            self.closure_method(&method, &ty, receiver, args)
          }
          BuiltinMethod::List(op) => self.list_method(op, &ty, receiver, args),
          BuiltinMethod::Prelude(function) => self.prelude_method(expr, function, &ty, receiver, args),
          BuiltinMethod::TotalCompare => self.total_compare(&ty, receiver, args),
          BuiltinMethod::RangeRev => self.range_iter(receiver, true, &ty),
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
        // A list or array calling a method of `Iter` becomes an iterator first.
        let converted = match self.typed.to_iter.get(&receiver.id).cloned() {
          Some((to_iter, args)) => Some(self.iter_of(receiver, to_iter, args)?),
          None => None,
        };
        let receiver_ty = match &converted {
          Some(Operand::Local(local)) => self.local_type(*local),
          _ => self.type_of(receiver)?,
        };

        if converted.is_none() && self.is_primitive_method(function, &receiver_ty) {
          return self.primitive_method(function, &ty, receiver, args);
        }

        let subst = self.call_subst(expr.id, function, Some(&receiver_ty));
        let sig = self.typed.sig(function).ok_or_else(|| LowerError("method without a signature".to_string()))?.substitute(&subst);
        // Methods of interface values go through the itable; interface methods on values of a
        // known type call that type's method or the default instantiated for it; `zod` calls
        // name their implementation; others go through the vtable when it varies.
        let (callee, by_ref) = match (&receiver_ty, self.resolution.owner(function)) {
          (Type::Interface(interface), _) => (self.dyn_method(interface, function)?, false),
          (_, Some(TypeItem::Interface(_))) => self.concrete_interface_call(expr.id, function, &receiver_ty),
          _ => {
            let by_ref = sig.receiver == Some(Receiver::Var);
            let target = self.instance(function, subst);

            match self.typed.slots.get(&function) {
              Some(slot) if !self.typed.zod_calls.contains(&expr.id) => (Callee::Virtual { slot: *slot, function: target }, false),
              _ => (Callee::Fn(target), by_ref),
            }
          }
        };
        let receiver_value = match converted {
          Some(value) => value,
          None if by_ref => Operand::Ref(self.base_place(receiver)?),
          None => self.expr(receiver)?,
        };
        let mut operands = vec![receiver_value];

        for (arg, param) in args.iter().zip(&sig.params) {
          let value = self.operand_as(arg, param)?;

          // The method may modify the receiver, so arguments must not borrow from it.
          operands.push(if by_ref { self.owned_copy(param.clone(), value) } else { value });
        }

        Ok(self.call(callee, operands, ty))
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
        let field_types = self.layouts.struct_fields(owner);
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
      ExprKind::Unary { op: UnaryOp::Deref, .. } => {
        // The pointee is never managed: its value is copied now.
        let place = self.place(expr)?;

        Ok(self.new_value(ty, Rvalue::Use(Operand::Place(place))))
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
      ExprKind::Block(block) | ExprKind::Unsafe(block) => {
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

        if self.for_range_method(var, iter, body)? || self.for_collection(var, iter, body)? {
          return Ok(UNIT);
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

        self.emit(Stmt::ForRange { var, ty: int, start, end, inclusive: *inclusive, reverse: false, step: None, body: loop_body });

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
            self.store_from(result, operand, &value_ty);
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
            let ret = self.ret.clone();
            let operand = self.operand_as(value, &ret)?;

            if self.managed(&ret) {
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
      ExprKind::Range { .. } => self.range_iter(expr, false, &ty),
    }
  }

  /// Lowers an expression used where a value of type `ty` is stored. A diverging expression
  /// has no value; the unreachable use gets a zero-initialized local of the right C type.
  fn operand_as(&mut self, expr: &Expr, ty: &Type) -> LResult<Operand> {
    let value = self.expr(expr)?;

    let from = self.type_of(expr)?;

    if from == Type::Never && !is_valueless(ty) {
      return Ok(Operand::Local(self.temp(ty.clone())));
    }

    Ok(self.convert(value, &from, ty))
  }

  /// `new C(args)`: allocates the instance and runs the initializer on it: the class's own, or
  /// the one it inherits (without arguments), after which the vtable is the class's again.
  fn new_instance(&mut self, ty: &Type, args: &[Expr]) -> LResult<Operand> {
    let Type::Class(class) = ty else {
      return error("`new` of a type other than a class or `StringBuilder`");
    };
    let class_instance = self.class_instance(class);
    let instance = self.new_value(ty.clone(), Rvalue::Alloc(class_instance));

    if self.typed.class_def(class.id).is_some_and(|def| def.initializer.is_none()) {
      if let (Some(initializer), Operand::Local(object)) = (self.typed.inherited_initializer(class.id), &instance) {
        let subst = match self.resolution.owner(initializer) {
          Some(TypeItem::Class(owner)) => {
            let args = self.typed.class_ancestor(class, owner).map(|ancestor| ancestor.args).unwrap_or_default();

            Subst::of(ParamOwner::Item(TypeItem::Class(owner)), args)
          }
          _ => Subst::default(),
        };
        let initializer = self.instance(initializer, subst);

        self.call(Callee::Fn(initializer), vec![instance.clone()], Type::Unit);
        self.emit(Stmt::SetVtable { object: *object, class: class_instance });
      }

      return Ok(instance);
    }

    let Some(initializer) = self.typed.class_def(class.id).and_then(|def| def.initializer) else {
      return Ok(instance);
    };
    let subst = Subst::of(ParamOwner::Item(TypeItem::Class(class.id)), class.args.clone());
    let params = self.typed.sig(initializer).map(|sig| sig.substitute(&subst).params).unwrap_or_default();
    let initializer = self.instance(initializer, subst);
    let mut operands = vec![instance.clone()];

    for (arg, param) in args.iter().zip(&params) {
      operands.push(self.operand_as(arg, param)?);
    }

    self.call(Callee::Fn(initializer), operands, Type::Unit);

    Ok(instance)
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
