//! Name resolution (docs/spec/09-modules.md): builds the module tree of the package, resolves
//! `use` declarations and binds every path expression, pattern and type path to what it names.
//! Lookup order for a single name: local bindings (innermost scope first, later shadowing
//! earlier), the module's items and imports, then the prelude. Functions, structs, enums and
//! imports share one namespace per module; the members of a struct (fields and functions) or of
//! an enum (variants and functions) share another one per type.

mod modules;

use std::collections::{HashMap, HashSet};

use sagitario_ast::{
  Block, EnumDecl, Expr, ExprKind, FnDecl, Ident, Item, Module, NodeId, Package, Path, Pattern, PatternKind, Stmt, StructDecl, TemplatePart,
  TypeExpr, TypeExprKind, VariantFields, Visibility,
};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_span::Span;

pub use modules::module_references;

/// Prelude functions implemented by the runtime (docs/spec/09-modules.md#prelude).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
  Print,
  Println,
  Eprintln,
  Panic,
}

impl Builtin {
  const ALL: [Builtin; 4] = [Builtin::Print, Builtin::Println, Builtin::Eprintln, Builtin::Panic];

  pub fn name(self) -> &'static str {
    match self {
      Builtin::Print => "print",
      Builtin::Println => "println",
      Builtin::Eprintln => "eprintln",
      Builtin::Panic => "panic",
    }
  }

  fn from_name(name: &str) -> Option<Builtin> {
    Builtin::ALL.into_iter().find(|builtin| builtin.name() == name)
  }
}

/// Index of a function (module function, method or static function) in `Resolution::functions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnId(pub u32);

/// Index of a struct in `Resolution::structs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructId(pub u32);

/// Index of an enum in `Resolution::enums`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct EnumId(pub u32);

/// Index of a module in `Resolution::modules`; the root module is `ModId(0)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ModId(pub u32);

/// A type declared by the program, which can have functions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TypeItem {
  Struct(StructId),
  Enum(EnumId),
}

/// The enum a variant belongs to: a program enum or one of the prelude's.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EnumKind {
  User(EnumId),
  /// Variants `None` (0) and `Some` (1).
  Option,
  /// Variants `Ok` (0) and `Err` (1).
  Result,
}

impl EnumKind {
  /// Variant names of the prelude enums, by index.
  pub fn prelude_variants(self) -> &'static [&'static str] {
    match self {
      EnumKind::Option => &["None", "Some"],
      EnumKind::Result => &["Ok", "Err"],
      EnumKind::User(_) => &[],
    }
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct VariantRef {
  pub owner: EnumKind,
  /// Position of the variant in its enum.
  pub index: u32,
}

/// The prelude variant with this name (`Some`, `None`, `Ok`, `Err`).
fn prelude_variant(name: &str) -> Option<VariantRef> {
  [EnumKind::Option, EnumKind::Result]
    .into_iter()
    .find_map(|owner| owner.prelude_variants().iter().position(|variant| *variant == name).map(|index| VariantRef { owner, index: index as u32 }))
}

/// A local binding of one function: `self` comes first in methods, then the parameters in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
  Param,
  /// `self` in a method declared with `self`: immutable, like a parameter.
  SelfParam,
  /// `self` in a method declared with `var self`: the caller's value, mutable.
  VarSelf,
  Const,
  Var,
  /// The variable of a `for` loop, a fresh `const` on each iteration.
  ForVar,
  /// A name bound by a `match` pattern: an immutable copy of part of the value.
  Pattern,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInfo {
  pub name: String,
  pub span: Span,
  pub kind: LocalKind,
}

/// What a path expression, struct literal or pattern refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
  Fn(FnId),
  Builtin(Builtin),
  Local(LocalId),
  Struct(StructId),
  Variant(VariantRef),
}

/// What a type path refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TypeRes {
  Item(TypeItem),
  /// A primitive or prelude type, or `Self`, which the type checker resolves by name.
  Builtin,
  /// Already reported.
  Error,
}

/// What a module-level name refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ItemRef {
  Fn(FnId),
  Type(TypeItem),
  Module(ModId),
}

/// An item declared in a module.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Declared {
  pub item: ItemRef,
  pub span: Span,
  pub visibility: Visibility,
}

/// A name brought into a module by `use`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Imported {
  pub item: ItemRef,
  /// The imported name in the `use` declaration.
  pub span: Span,
}

/// A module of the package: a source file, or a directory without its own file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ModuleInfo {
  /// The logical path from the package root (`pkg`).
  pub path: Vec<String>,
  /// Index in `Package::modules`; `None` for a directory without a file.
  pub source: Option<usize>,
  pub parent: Option<ModId>,
  pub children: HashMap<String, ModId>,
  pub items: HashMap<String, Declared>,
  pub imports: HashMap<String, Imported>,
}

impl ModuleInfo {
  /// The path as written in source: `pkg`, `pkg::net::http`.
  pub fn display_path(&self) -> String {
    std::iter::once("pkg").chain(self.path.iter().map(String::as_str)).collect::<Vec<_>>().join("::")
  }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FnInfo {
  /// Index in `Package::modules` of the declaring module.
  pub module: usize,
  /// Index in `Module::items` of the function, or of the type that declares it.
  pub item: usize,
  /// Index in the declaring type's `functions`.
  pub member: Option<usize>,
  pub owner: Option<TypeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
  /// Index in `Package::modules` of the declaring module.
  pub module: usize,
  /// Index in `Module::items`.
  pub item: usize,
  /// The type's functions, in declaration order.
  pub functions: Vec<FnId>,
}

#[derive(Debug, Default)]
pub struct Resolution {
  /// Indexed by `FnId`.
  pub functions: Vec<FnInfo>,
  /// Indexed by `StructId`.
  pub structs: Vec<TypeInfo>,
  /// Indexed by `EnumId`.
  pub enums: Vec<TypeInfo>,
  /// The module tree, indexed by `ModId`.
  pub modules: Vec<ModuleInfo>,
  /// Local bindings of each function, indexed by `FnId` then `LocalId`.
  pub locals: Vec<Vec<LocalInfo>>,
  /// Path expressions, struct literals, variant constructors and variant patterns, by node id.
  pub paths: HashMap<NodeId, Res>,
  /// Type paths, by the id of the `Path`; also the paths of `new` that name a program type.
  pub type_paths: HashMap<NodeId, TypeRes>,
  /// The local declared by each `const`/`var` statement, `for` expression and binding pattern.
  pub bindings: HashMap<NodeId, LocalId>,
  pub main: Option<FnId>,
}

impl Resolution {
  pub fn fn_decl<'p>(&self, package: &'p Package, id: FnId) -> Option<&'p FnDecl> {
    let info = self.functions.get(id.0 as usize)?;

    match (package.modules.get(info.module)?.items.get(info.item)?, info.member) {
      (Item::Fn(decl), None) => Some(decl),
      (Item::Struct(decl), Some(member)) => decl.functions.get(member),
      (Item::Enum(decl), Some(member)) => decl.functions.get(member),
      _ => None,
    }
  }

  /// Every function with its id, in declaration order.
  pub fn fn_decls<'p>(&self, package: &'p Package) -> Vec<(FnId, &'p FnDecl)> {
    (0..self.functions.len() as u32).filter_map(|id| self.fn_decl(package, FnId(id)).map(|decl| (FnId(id), decl))).collect()
  }

  /// The type that declares a function.
  pub fn owner(&self, id: FnId) -> Option<TypeItem> {
    self.functions.get(id.0 as usize).and_then(|info| info.owner)
  }

  pub fn struct_decl<'p>(&self, package: &'p Package, id: StructId) -> Option<&'p StructDecl> {
    let info = self.structs.get(id.0 as usize)?;

    match package.modules.get(info.module)?.items.get(info.item)? {
      Item::Struct(decl) => Some(decl),
      _ => None,
    }
  }

  pub fn enum_decl<'p>(&self, package: &'p Package, id: EnumId) -> Option<&'p EnumDecl> {
    let info = self.enums.get(id.0 as usize)?;

    match package.modules.get(info.module)?.items.get(info.item)? {
      Item::Enum(decl) => Some(decl),
      _ => None,
    }
  }

  /// The name of a struct or enum.
  pub fn type_name(&self, package: &Package, item: TypeItem) -> String {
    match item {
      TypeItem::Struct(id) => self.struct_decl(package, id).map(|decl| decl.name.name.clone()),
      TypeItem::Enum(id) => self.enum_decl(package, id).map(|decl| decl.name.name.clone()),
    }
    .unwrap_or_default()
  }

  /// A type's function (method or static) by name.
  pub fn type_function(&self, package: &Package, item: TypeItem, name: &str) -> Option<FnId> {
    let info = match item {
      TypeItem::Struct(id) => self.structs.get(id.0 as usize),
      TypeItem::Enum(id) => self.enums.get(id.0 as usize),
    }?;

    info.functions.iter().copied().find(|function| self.fn_decl(package, *function).is_some_and(|decl| decl.name.name == name))
  }

  pub fn locals(&self, id: FnId) -> &[LocalInfo] {
    self.locals.get(id.0 as usize).map_or(&[], Vec::as_slice)
  }

  fn module(&self, id: ModId) -> Option<&ModuleInfo> {
    self.modules.get(id.0 as usize)
  }

  /// Whether `inner` is `outer` or one of its submodules, which see `outer`'s private items.
  fn is_within(&self, inner: ModId, outer: ModId) -> bool {
    let mut current = Some(inner);

    while let Some(id) = current {
      if id == outer {
        return true;
      }

      current = self.module(id).and_then(|module| module.parent);
    }

    false
  }
}

/// Names reserved for the future concurrency model (docs/spec/01-lexical.md).
const RESERVED_NAMES: &[&str] = &["Send", "Sync", "spawn"];

/// Integer and float type names, which take precedence over program types.
const NUMERIC_TYPES: &[&str] = &["i8", "i16", "i32", "i64", "i128", "isize", "u8", "u16", "u32", "u64", "u128", "usize", "f32", "f64"];

/// Prelude and primitive type names that program types can shadow.
const PRELUDE_TYPES: &[&str] = &["bool", "char", "String", "StringBuilder", "Option", "Result"];

pub fn resolve(package: &Package, diagnostics: &mut Diagnostics) -> Resolution {
  let mut resolution = Resolution::default();
  let module_ids = modules::build_tree(&mut resolution, package);

  for (source, module) in package.modules.iter().enumerate() {
    declare_items(&mut resolution, source, module_of(&module_ids, source), module, diagnostics);
  }

  resolution.main = match resolution.module(ModId(0)).and_then(|root| root.items.get("main")) {
    Some(Declared { item: ItemRef::Fn(id), .. }) => Some(*id),
    _ => None,
  };

  if resolution.main.is_none()
    && let Some(root) = package.modules.first()
  {
    diagnostics.push(
      Diagnostic::error(codes::MISSING_MAIN, Span::new(root.file, 0, 0), "the program has no `main` function").with_note("add `fn main() { }`"),
    );
  }

  for (source, module) in package.modules.iter().enumerate() {
    modules::resolve_imports(&mut resolution, package, module_of(&module_ids, source), module, diagnostics);
  }

  let mut used = HashSet::new();

  // The types in the fields of structs and variants, then each function.
  let mut declarations: Vec<(TypeItem, usize, Vec<&TypeExpr>)> = Vec::new();

  for index in 0..resolution.structs.len() {
    let id = StructId(index as u32);

    if let (Some(decl), Some(info)) = (resolution.struct_decl(package, id), resolution.structs.get(index)) {
      declarations.push((TypeItem::Struct(id), info.module, decl.fields.iter().map(|field| &field.ty).collect()));
    }
  }

  for index in 0..resolution.enums.len() {
    let id = EnumId(index as u32);

    if let (Some(decl), Some(info)) = (resolution.enum_decl(package, id), resolution.enums.get(index)) {
      let types = decl.variants.iter().flat_map(|variant| match &variant.fields {
        VariantFields::Unit => Vec::new(),
        VariantFields::Tuple(types) => types.iter().collect(),
        VariantFields::Named(fields) => fields.iter().map(|field| &field.ty).collect(),
      });

      declarations.push((TypeItem::Enum(id), info.module, types.collect()));
    }
  }

  for (owner, source, types) in declarations {
    let mut resolver = Resolver::new(package, &resolution, module_of(&module_ids, source), Some(owner), diagnostics);

    for ty in types {
      resolver.type_expr(ty);
    }

    let output = resolver.finish();

    used.extend(output.used);
    resolution.type_paths.extend(output.type_paths);
  }

  for (id, decl) in resolution.fn_decls(package) {
    let Some(info) = resolution.functions.get(id.0 as usize).copied() else { continue };
    let mut resolver = Resolver::new(package, &resolution, module_of(&module_ids, info.module), info.owner, diagnostics);

    if let Some(self_param) = &decl.self_param {
      let kind = if self_param.mutable { LocalKind::VarSelf } else { LocalKind::SelfParam };

      resolver.declare("self", self_param.span, kind);
    }

    for param in &decl.params {
      let previous = resolver.lookup_local(&param.name.name).and_then(|id| resolver.locals.get(id.0 as usize)).map(|info| info.span);

      if let Some(previous) = previous {
        resolver.diagnostics.push(
          Diagnostic::error(codes::DUPLICATE_DEFINITION, param.name.span, format!("the parameter `{}` is declared more than once", param.name.name))
            .with_label(previous, "previous declaration here"),
        );
      }

      resolver.type_expr(&param.ty);
      resolver.declare(&param.name.name, param.name.span, LocalKind::Param);
    }

    if let Some(ty) = &decl.return_type {
      resolver.type_expr(ty);
    }

    resolver.block(&decl.body);

    let output = resolver.finish();

    resolution.locals.push(output.locals);
    resolution.paths.extend(output.paths);
    resolution.bindings.extend(output.bindings);
    resolution.type_paths.extend(output.type_paths);
    used.extend(output.used);
  }

  modules::report_unused_imports(&resolution, &used, diagnostics);
  modules::report_private_interfaces(&resolution, package, diagnostics);

  resolution
}

/// The module of the file at `source` in `Package::modules`.
fn module_of(module_ids: &[ModId], source: usize) -> ModId {
  module_ids.get(source).copied().unwrap_or(ModId(0))
}

/// Registers the functions, structs and enums of one module.
fn declare_items(resolution: &mut Resolution, source: usize, node: ModId, module: &Module, diagnostics: &mut Diagnostics) {
  for (index, item) in module.items.iter().enumerate() {
    let (name, item_ref, visibility) = match item {
      Item::Fn(decl) => {
        let id = FnId(resolution.functions.len() as u32);

        resolution.functions.push(FnInfo { module: source, item: index, member: None, owner: None });

        (&decl.name, ItemRef::Fn(id), decl.visibility)
      }
      Item::Struct(decl) => {
        let owner = TypeItem::Struct(StructId(resolution.structs.len() as u32));
        let functions = register_functions(resolution, source, index, decl.functions.len(), owner);

        resolution.structs.push(TypeInfo { module: source, item: index, functions });

        let names = decl.fields.iter().map(|field| &field.name).chain(decl.functions.iter().map(|function| &function.name));

        check_members(&decl.name.name, "fields and functions of a struct share one namespace", names, diagnostics);

        (&decl.name, ItemRef::Type(owner), decl.visibility)
      }
      Item::Enum(decl) => {
        let owner = TypeItem::Enum(EnumId(resolution.enums.len() as u32));
        let functions = register_functions(resolution, source, index, decl.functions.len(), owner);

        resolution.enums.push(TypeInfo { module: source, item: index, functions });

        let names = decl.variants.iter().map(|variant| &variant.name).chain(decl.functions.iter().map(|function| &function.name));

        check_members(&decl.name.name, "variants and functions of an enum share one namespace", names, diagnostics);

        for variant in &decl.variants {
          if let VariantFields::Named(fields) = &variant.fields {
            check_members(&variant.name.name, "the fields of a variant need different names", fields.iter().map(|field| &field.name), diagnostics);
          }
        }

        (&decl.name, ItemRef::Type(owner), decl.visibility)
      }
    };

    check_reserved(&name.name, name.span, diagnostics);

    let Some(info) = resolution.modules.get_mut(node.0 as usize) else { continue };

    if let Some(previous) = info.items.get(&name.name) {
      diagnostics.push(
        Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("the name `{}` is defined more than once", name.name))
          .with_label(previous.span, "previous definition here")
          .with_note("functions, structs and enums share one namespace per module"),
      );
    } else if info.children.contains_key(&name.name) {
      let module_path = format!("{}::{}", info.display_path(), name.name);

      diagnostics.push(
        Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("the name `{}` is also the module `{module_path}`", name.name))
          .with_note("items and modules share one namespace; rename the item or the module's file or directory"),
      );
    } else {
      info.items.insert(name.name.clone(), Declared { item: item_ref, span: name.span, visibility });
    }
  }
}

fn register_functions(resolution: &mut Resolution, source: usize, item: usize, count: usize, owner: TypeItem) -> Vec<FnId> {
  let mut functions = Vec::new();

  for member in 0..count {
    functions.push(FnId(resolution.functions.len() as u32));
    resolution.functions.push(FnInfo { module: source, item, member: Some(member), owner: Some(owner) });
  }

  functions
}

/// Reports names declared twice among the members of a type.
fn check_members<'a>(owner: &str, note: &str, names: impl Iterator<Item = &'a sagitario_ast::Ident>, diagnostics: &mut Diagnostics) {
  let mut seen: HashMap<&str, Span> = HashMap::new();

  for name in names {
    check_reserved(&name.name, name.span, diagnostics);

    if let Some(previous) = seen.get(name.name.as_str()) {
      diagnostics.push(
        Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("the name `{}` is declared more than once in `{owner}`", name.name))
          .with_label(*previous, "previous declaration here")
          .with_note(note),
      );
    } else {
      seen.insert(&name.name, name.span);
    }
  }
}

fn check_reserved(name: &str, span: Span, diagnostics: &mut Diagnostics) {
  if RESERVED_NAMES.contains(&name) {
    diagnostics.push(Diagnostic::error(
      codes::RESERVED_NAME,
      span,
      format!("`{name}` is reserved for the future concurrency model and cannot be declared"),
    ));
  }
}

/// What the start of a qualified path names: a module-level item, or a prelude enum.
#[derive(Clone, Copy)]
enum Named {
  Item(ItemRef),
  Prelude(EnumKind),
}

/// A name bound by a pattern, for checking the alternatives of `|`.
type PatternBinding = (String, LocalId, Span);

/// What one resolver produced, merged into the `Resolution` afterwards.
struct Output {
  locals: Vec<LocalInfo>,
  paths: HashMap<NodeId, Res>,
  bindings: HashMap<NodeId, LocalId>,
  type_paths: HashMap<NodeId, TypeRes>,
  used: HashSet<(ModId, String)>,
}

struct Resolver<'a> {
  package: &'a Package,
  resolution: &'a Resolution,
  /// The module whose code is being resolved.
  module: ModId,
  /// The type whose function is being resolved, for `Self` and `prv` members.
  owner: Option<TypeItem>,
  locals: Vec<LocalInfo>,
  /// Names visible in each nested block, innermost last.
  scopes: Vec<Vec<(String, LocalId)>>,
  paths: HashMap<NodeId, Res>,
  bindings: HashMap<NodeId, LocalId>,
  type_paths: HashMap<NodeId, TypeRes>,
  /// Imports that were used, by module and name.
  used: HashSet<(ModId, String)>,
  diagnostics: &'a mut Diagnostics,
}

impl<'a> Resolver<'a> {
  fn new(package: &'a Package, resolution: &'a Resolution, module: ModId, owner: Option<TypeItem>, diagnostics: &'a mut Diagnostics) -> Resolver<'a> {
    Resolver {
      package,
      resolution,
      module,
      owner,
      locals: Vec::new(),
      scopes: vec![Vec::new()],
      paths: HashMap::new(),
      bindings: HashMap::new(),
      type_paths: HashMap::new(),
      used: HashSet::new(),
      diagnostics,
    }
  }

  fn finish(self) -> Output {
    Output { locals: self.locals, paths: self.paths, bindings: self.bindings, type_paths: self.type_paths, used: self.used }
  }

  fn declare(&mut self, name: &str, span: Span, kind: LocalKind) -> LocalId {
    check_reserved(name, span, self.diagnostics);

    let id = LocalId(self.locals.len() as u32);

    self.locals.push(LocalInfo { name: name.to_string(), span, kind });

    if let Some(scope) = self.scopes.last_mut() {
      scope.push((name.to_string(), id));
    }

    id
  }

  fn lookup_local(&self, name: &str) -> Option<LocalId> {
    self.scopes.iter().rev().find_map(|scope| scope.iter().rev().find(|(local, _)| local == name).map(|(_, id)| *id))
  }

  /// A module-level name of the current module: one of its items or imports.
  fn lookup_item(&mut self, name: &str) -> Option<ItemRef> {
    let info = self.resolution.module(self.module)?;

    if let Some(declared) = info.items.get(name) {
      return Some(declared.item);
    }

    let imported = info.imports.get(name)?.item;

    self.used.insert((self.module, name.to_string()));

    Some(imported)
  }

  /// Names of the current module's items and imports, for "similar name" suggestions.
  fn item_names(&self) -> Vec<String> {
    let Some(info) = self.resolution.module(self.module) else { return Vec::new() };

    info.items.keys().chain(info.imports.keys()).cloned().collect()
  }

  fn block(&mut self, block: &Block) {
    self.scopes.push(Vec::new());

    for stmt in &block.stmts {
      match stmt {
        Stmt::Let(stmt) => {
          if let Some(ty) = &stmt.ty {
            self.type_expr(ty);
          }

          // The initializer is resolved first, so `const x = x + 1;` reads the outer `x`.
          if let Some(init) = &stmt.init {
            self.expr(init);
          }

          let kind = if stmt.mutable { LocalKind::Var } else { LocalKind::Const };
          let id = self.declare(&stmt.name.name, stmt.name.span, kind);

          self.bindings.insert(stmt.id, id);
        }
        Stmt::Assign { target, value, .. } => {
          self.expr(target);
          self.expr(value);
        }
        Stmt::Expr(expr) => self.expr(expr),
      }
    }

    if let Some(tail) = &block.tail {
      self.expr(tail);
    }

    self.scopes.pop();
  }

  fn exprs(&mut self, exprs: &[Expr]) {
    for expr in exprs {
      self.expr(expr);
    }
  }

  fn expr(&mut self, expr: &Expr) {
    match &expr.kind {
      ExprKind::Path(path) => {
        if let Some(res) = self.path(path) {
          self.paths.insert(expr.id, res);
        }
      }
      ExprKind::Call { callee, args } => {
        self.expr(callee);
        self.exprs(args);
      }
      ExprKind::MethodCall { receiver, args, .. } => {
        self.expr(receiver);
        self.exprs(args);
      }
      ExprKind::Field { base, .. } => self.expr(base),
      ExprKind::Index { base, index } => {
        self.expr(base);
        self.expr(index);
      }
      ExprKind::StructLit { path, fields } => {
        if let Some(res) = self.struct_literal_path(path) {
          self.paths.insert(expr.id, res);
        }

        for field in fields {
          self.expr(&field.value);
        }
      }
      ExprKind::ArrayLit(elems) => self.exprs(elems),
      ExprKind::ArrayRepeat { value, .. } => self.expr(value),
      ExprKind::Try(inner) => self.expr(inner),
      // In F1 only `new StringBuilder()` exists; the type checker reports any other type.
      ExprKind::New { path, args } => {
        self.new_path(path);
        self.exprs(args);
      }
      ExprKind::Match { scrutinee, arms } => {
        self.expr(scrutinee);

        for arm in arms {
          self.scopes.push(Vec::new());
          self.pattern(&arm.pattern, &mut Vec::new(), None);

          if let Some(guard) = &arm.guard {
            self.expr(guard);
          }

          self.expr(&arm.body);
          self.scopes.pop();
        }
      }
      ExprKind::Template(parts) => {
        for part in parts {
          if let TemplatePart::Expr(expr) = part {
            self.expr(expr);
          }
        }
      }
      ExprKind::Unary { operand, .. } => self.expr(operand),
      ExprKind::Binary { lhs, rhs, .. } => {
        self.expr(lhs);
        self.expr(rhs);
      }
      ExprKind::Cast { expr, ty } => {
        self.expr(expr);
        self.type_expr(ty);
      }
      ExprKind::Range { start, end, .. } => {
        self.expr(start);
        self.expr(end);
      }
      ExprKind::Block(block) | ExprKind::Loop { body: block } => self.block(block),
      ExprKind::If { cond, then_block, else_branch } => {
        self.expr(cond);
        self.block(then_block);

        if let Some(branch) = else_branch {
          self.expr(branch);
        }
      }
      ExprKind::While { cond, body } => {
        self.expr(cond);
        self.block(body);
      }
      ExprKind::For { var, iter, body } => {
        self.expr(iter);
        self.scopes.push(Vec::new());

        let id = self.declare(&var.name, var.span, LocalKind::ForVar);

        self.bindings.insert(expr.id, id);
        self.block(body);
        self.scopes.pop();
      }
      ExprKind::Break { value } | ExprKind::Return { value } => {
        if let Some(value) = value {
          self.expr(value);
        }
      }
      ExprKind::Int { .. }
      | ExprKind::Float { .. }
      | ExprKind::Bool(_)
      | ExprKind::Char(_)
      | ExprKind::Str(_)
      | ExprKind::Unit
      | ExprKind::Continue => {}
    }
  }

  /// Resolves a pattern, declaring its bindings in the current scope. `binds` collects the
  /// names it binds; with `reuse`, names must be ones already bound by the first alternative of
  /// an enclosing `|` pattern, and refer to the same locals.
  fn pattern(&mut self, pattern: &Pattern, binds: &mut Vec<PatternBinding>, reuse: Option<&[PatternBinding]>) {
    match &pattern.kind {
      PatternKind::Wildcard => {}
      PatternKind::Literal(expr) => self.expr(expr),
      PatternKind::Ident(name) => {
        if let Some(variant) = prelude_variant(&name.name).filter(|_| self.lookup_item(&name.name).is_none()) {
          self.paths.insert(pattern.id, Res::Variant(variant));

          return;
        }

        if let Some((_, _, previous)) = binds.iter().find(|(bound, _, _)| *bound == name.name) {
          self.diagnostics.push(
            Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("`{}` is bound more than once in the same pattern", name.name))
              .with_label(*previous, "first bound here"),
          );

          return;
        }

        let id = match reuse {
          Some(first) => match first.iter().find(|(bound, _, _)| *bound == name.name) {
            Some((_, id, _)) => *id,
            None => {
              self.diagnostics.push(
                Diagnostic::error(
                  codes::INCONSISTENT_BINDINGS,
                  name.span,
                  format!("`{}` is not bound in every alternative of the pattern", name.name),
                )
                .with_note("each alternative of `|` must bind the same names"),
              );

              return;
            }
          },
          None => self.declare(&name.name, name.span, LocalKind::Pattern),
        };

        self.bindings.insert(pattern.id, id);
        binds.push((name.name.clone(), id, name.span));
      }
      PatternKind::Path(path) => self.variant_pattern(pattern.id, path),
      PatternKind::Tuple { path, elems } => {
        self.variant_pattern(pattern.id, path);

        for elem in elems {
          self.pattern(elem, binds, reuse);
        }
      }
      PatternKind::Named { path, fields, .. } => {
        self.variant_pattern(pattern.id, path);

        for field in fields {
          self.pattern(&field.pattern, binds, reuse);
        }
      }
      PatternKind::Or(alternatives) => {
        let Some((first, rest)) = alternatives.split_first() else { return };
        let mut first_binds = Vec::new();

        self.pattern(first, &mut first_binds, reuse);

        for alternative in rest {
          let mut alternative_binds = Vec::new();

          self.pattern(alternative, &mut alternative_binds, Some(&first_binds));

          for (name, _, _) in &first_binds {
            if !alternative_binds.iter().any(|(bound, _, _)| bound == name) {
              self.diagnostics.push(
                Diagnostic::error(
                  codes::INCONSISTENT_BINDINGS,
                  alternative.span,
                  format!("`{name}` is not bound in every alternative of the pattern"),
                )
                .with_note("each alternative of `|` must bind the same names"),
              );
            }
          }
        }

        binds.extend(first_binds);
      }
    }
  }

  fn variant_pattern(&mut self, id: NodeId, path: &Path) {
    if let Some(variant) = self.variant_path(path) {
      self.paths.insert(id, Res::Variant(variant));
    }
  }

  /// A path that must name a variant: `Some`, `Shape::Circle`, `Self::Empty`,
  /// `shapes::Shape::Circle`.
  fn variant_path(&mut self, path: &Path) -> Option<VariantRef> {
    if let [segment] = path.segments.as_slice() {
      let variant = prelude_variant(&segment.name).filter(|_| self.lookup_item(&segment.name).is_none());

      if variant.is_none() {
        self.error(segment.span, format!("`{}` is not a variant", segment.name));
      }

      return variant;
    }

    let (named, owner, rest) = self.qualified(&path.segments)?;

    match (named, rest) {
      (Named::Item(ItemRef::Type(TypeItem::Enum(id))), [name]) => self.user_variant(id, &name.name, name.span),
      (Named::Prelude(kind), [name]) => self.prelude_variant_of(kind, &name.name, name.span),
      (Named::Item(ItemRef::Type(TypeItem::Struct(_))), [_]) => {
        self.error(owner.span, format!("`{}` is a struct, not an enum", owner.name));

        None
      }
      _ => {
        self.error(path.span, "this path does not name a variant");

        None
      }
    }
  }

  fn user_variant(&mut self, id: EnumId, name: &str, span: Span) -> Option<VariantRef> {
    let decl = self.resolution.enum_decl(self.package, id)?;
    let index = decl.variants.iter().position(|variant| variant.name.name == name);

    if index.is_none() {
      let enum_name = decl.name.name.clone();
      let candidates: Vec<String> = decl.variants.iter().map(|variant| variant.name.name.clone()).collect();
      let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("no variant `{name}` in `{enum_name}`"));

      if let Some(similar) = sagitario_diagnostics::closest_match(name, candidates.iter().map(String::as_str)) {
        diagnostic = diagnostic.with_help(format!("a variant with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);
    }

    index.map(|index| VariantRef { owner: EnumKind::User(id), index: index as u32 })
  }

  fn prelude_variant_of(&mut self, kind: EnumKind, name: &str, span: Span) -> Option<VariantRef> {
    let index = kind.prelude_variants().iter().position(|variant| *variant == name);

    if index.is_none() {
      let enum_name = if kind == EnumKind::Option { "Option" } else { "Result" };

      self.error(span, format!("no variant `{name}` in `{enum_name}`"));
    }

    index.map(|index| VariantRef { owner: kind, index: index as u32 })
  }

  fn error(&mut self, span: Span, message: impl Into<String>) {
    self.diagnostics.push(Diagnostic::error(codes::UNRESOLVED_NAME, span, message));
  }

  fn path(&mut self, path: &Path) -> Option<Res> {
    if let [segment] = path.segments.as_slice() {
      return self.value_name(&segment.name, segment.span);
    }

    let (named, last, rest) = self.qualified(&path.segments)?;

    match (named, rest) {
      (Named::Item(ItemRef::Fn(id)), []) => Some(Res::Fn(id)),
      (Named::Item(ItemRef::Type(item)), []) => {
        self.not_a_value(&last.name, last.span, item);

        None
      }
      (Named::Item(ItemRef::Module(_)), []) => {
        self.error(last.span, format!("`{}` is a module, not a value", last.name));

        None
      }
      (Named::Prelude(_), []) => {
        self.error(last.span, format!("`{}` is an enum, not a value", last.name));

        None
      }
      (Named::Item(ItemRef::Type(item)), [member]) => {
        if let TypeItem::Enum(id) = item
          && let Some(decl) = self.resolution.enum_decl(self.package, id)
          && let Some(index) = decl.variants.iter().position(|variant| variant.name.name == member.name)
        {
          return Some(Res::Variant(VariantRef { owner: EnumKind::User(id), index: index as u32 }));
        }

        self.type_function(item, &member.name, member.span)
      }
      (Named::Prelude(kind), [member]) => self.prelude_variant_of(kind, &member.name, member.span).map(Res::Variant),
      (_, [extra, ..]) => {
        self.error(extra.span, format!("cannot find `{}` in `{}`", extra.name, last.name));

        None
      }
    }
  }

  fn not_a_value(&mut self, name: &str, span: Span, item: TypeItem) {
    let diagnostic = match item {
      TypeItem::Struct(_) => Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("`{name}` is a struct, not a value"))
        .with_help(format!("build a value with `{name} {{ ... }}` or call one of its static functions, such as `{name}::new(...)`")),
      TypeItem::Enum(_) => Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("`{name}` is an enum, not a value"))
        .with_help(format!("name one of its variants: `{name}::Variant`")),
    };

    self.diagnostics.push(diagnostic);
  }

  /// A single name used as a value.
  fn value_name(&mut self, name: &str, span: Span) -> Option<Res> {
    if let Some(id) = self.lookup_local(name) {
      return Some(Res::Local(id));
    }

    match name {
      "self" => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, span, "`self` is only available in methods")
            .with_help("add a `self` parameter to make this function a method of its type"),
        );

        return None;
      }
      "Self" => {
        self.error(span, "`Self` is a type, not a value");

        return None;
      }
      "pkg" | "super" => {
        self.error(span, format!("`{name}` is a module, not a value"));

        return None;
      }
      _ => {}
    }

    match self.lookup_item(name) {
      Some(ItemRef::Fn(id)) => return Some(Res::Fn(id)),
      Some(ItemRef::Type(item)) => {
        self.not_a_value(name, span, item);

        return None;
      }
      Some(ItemRef::Module(_)) => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("`{name}` is a module, not a value"))
            .with_help(format!("name one of its items: `{name}::item`")),
        );

        return None;
      }
      None => {}
    }

    if let Some(builtin) = Builtin::from_name(name) {
      return Some(Res::Builtin(builtin));
    }

    if let Some(variant) = prelude_variant(name) {
      return Some(Res::Variant(variant));
    }

    let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("cannot find `{name}` in this scope"));
    let items = self.item_names();
    let visible = self.scopes.iter().flatten().map(|(local, _)| local.as_str());
    let candidates = visible.chain(items.iter().map(String::as_str)).chain(Builtin::ALL.iter().map(|builtin| builtin.name()));

    if let Some(similar) = sagitario_diagnostics::closest_match(name, candidates) {
      diagnostic = diagnostic.with_help(format!("a name with a similar spelling exists: `{similar}`"));
    }

    self.diagnostics.push(diagnostic);

    None
  }

  /// Resolves the first segment of a qualified path (`pkg`, `self`, `super`, `Self`, or a
  /// module-level name), then follows modules: `pkg::net::http::get` stops at the function `get`.
  /// Returns what the path names so far, the last segment consumed and the segments left.
  fn qualified<'p>(&mut self, segments: &'p [Ident]) -> Option<(Named, &'p Ident, &'p [Ident])> {
    let (first, mut rest) = segments.split_first()?;
    let mut named = match first.name.as_str() {
      "pkg" => Named::Item(ItemRef::Module(ModId(0))),
      "self" => Named::Item(ItemRef::Module(self.module)),
      "super" => Named::Item(ItemRef::Module(self.parent(self.module, first.span)?)),
      "Self" => {
        if self.owner.is_none() {
          self.error(first.span, "`Self` is only available inside a struct or enum");
        }

        Named::Item(ItemRef::Type(self.owner?))
      }
      name => match self.lookup_item(name) {
        Some(item) => Named::Item(item),
        None if name == "Option" => Named::Prelude(EnumKind::Option),
        None if name == "Result" => Named::Prelude(EnumKind::Result),
        None if name == "std" => {
          self.diagnostics.push(Diagnostic::error(codes::NOT_IMPLEMENTED, first.span, "the standard library (`std`) is not available yet"));

          return None;
        }
        None => {
          let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, first.span, format!("cannot find `{name}` in this scope"));
          let items = self.item_names();

          if let Some(similar) = sagitario_diagnostics::closest_match(name, items.iter().map(String::as_str)) {
            diagnostic = diagnostic.with_help(format!("a name with a similar spelling exists: `{similar}`"));
          } else {
            diagnostic = diagnostic.with_help(format!("to use a module of this package, import it with `use pkg::{name};`"));
          }

          self.diagnostics.push(diagnostic);

          return None;
        }
      },
    };
    let mut last = first;
    // `super` may only repeat at the start: `super::super::x`.
    let mut leading_super = first.name == "super";

    while let Named::Item(ItemRef::Module(module)) = named
      && let Some((segment, tail)) = rest.split_first()
    {
      named = match segment.name.as_str() {
        "super" if leading_super => Named::Item(ItemRef::Module(self.parent(module, segment.span)?)),
        "pkg" | "self" | "super" | "Self" => {
          self.error(segment.span, format!("`{}` can only start a path", segment.name));

          return None;
        }
        _ => {
          leading_super = false;

          Named::Item(self.module_member(module, segment)?)
        }
      };
      last = segment;
      rest = tail;
    }

    Some((named, last, rest))
  }

  fn parent(&mut self, module: ModId, span: Span) -> Option<ModId> {
    let parent = self.resolution.module(module).and_then(|info| info.parent);

    if parent.is_none() {
      self.error(span, "the root module `pkg` has no parent module");
    }

    parent
  }

  /// An item or submodule of `module`, checking that the current module can see it.
  fn module_member(&mut self, module: ModId, segment: &Ident) -> Option<ItemRef> {
    let info = self.resolution.module(module)?;
    let name = segment.name.as_str();

    if let Some(declared) = info.items.get(name) {
      if declared.visibility == Visibility::Module && !self.resolution.is_within(self.module, module) {
        self.diagnostics.push(
          Diagnostic::error(codes::PRIVATE_ITEM, segment.span, format!("`{name}` is private to the module `{}`", info.display_path()))
            .with_label(declared.span, "declared without `pub` here")
            .with_help("mark it `pub` to use it from other modules"),
        );
      }

      return Some(declared.item);
    }

    if let Some(child) = info.children.get(name) {
      return Some(ItemRef::Module(*child));
    }

    let mut diagnostic =
      Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("cannot find `{name}` in the module `{}`", info.display_path()));
    let candidates: Vec<&str> = info.items.keys().chain(info.children.keys()).map(String::as_str).collect();

    if let Some(similar) = sagitario_diagnostics::closest_match(name, candidates) {
      diagnostic = diagnostic.with_help(format!("a name with a similar spelling exists: `{similar}`"));
    } else if info.source.is_some() || module == ModId(0) {
      let file = info.path.iter().chain([&segment.name]).cloned().collect::<Vec<_>>().join("/");

      diagnostic = diagnostic.with_note(format!("a module `{name}` would be the file `{file}.sag` next to the program's entry file"));
    }

    self.diagnostics.push(diagnostic);

    None
  }

  /// The path of `Name { ... }`: a struct, or an enum variant with named fields.
  fn struct_literal_path(&mut self, path: &Path) -> Option<Res> {
    let (named, last, rest) = self.qualified(&path.segments)?;

    match (named, rest) {
      (Named::Item(ItemRef::Type(TypeItem::Struct(id))), []) => Some(Res::Struct(id)),
      (Named::Item(ItemRef::Type(TypeItem::Enum(_))) | Named::Prelude(_), []) => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, last.span, format!("`{}` is an enum, not a struct", last.name))
            .with_help("name the variant: `Enum::Variant { ... }`"),
        );

        None
      }
      (Named::Item(ItemRef::Type(TypeItem::Enum(id))), [name]) => self.user_variant(id, &name.name, name.span).map(Res::Variant),
      (Named::Prelude(kind), [name]) => self.prelude_variant_of(kind, &name.name, name.span).map(Res::Variant),
      _ => {
        self.error(path.span, "this path does not name a struct or a variant");

        None
      }
    }
  }

  /// `Type::function`: a static function of a struct or enum.
  fn type_function(&mut self, item: TypeItem, name: &str, span: Span) -> Option<Res> {
    let type_name = self.resolution.type_name(self.package, item);
    let Some(function) = self.resolution.type_function(self.package, item, name) else {
      let functions = match item {
        TypeItem::Struct(id) => self.resolution.struct_decl(self.package, id).map(|decl| &decl.functions),
        TypeItem::Enum(id) => self.resolution.enum_decl(self.package, id).map(|decl| &decl.functions),
      };
      let mut candidates: Vec<String> = functions.into_iter().flatten().map(|function| function.name.name.clone()).collect();
      let what = match item {
        TypeItem::Struct(_) => "function",
        TypeItem::Enum(id) => {
          let variants = self.resolution.enum_decl(self.package, id).map(|decl| &decl.variants);

          candidates.extend(variants.into_iter().flatten().map(|variant| variant.name.name.clone()));

          "variant or function"
        }
      };
      let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("no {what} `{name}` in `{type_name}`"));

      if let Some(similar) = sagitario_diagnostics::closest_match(name, candidates.iter().map(String::as_str)) {
        diagnostic = diagnostic.with_help(format!("a {what} with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);

      return None;
    };
    let decl = self.resolution.fn_decl(self.package, function)?;
    let (is_method, is_private, declared) = (decl.self_param.is_some(), decl.visibility == Visibility::Private, decl.name.span);

    if is_method {
      self.diagnostics.push(
        Diagnostic::error(codes::WRONG_CALL_FORM, span, format!("`{name}` is a method of `{type_name}`, not a static function"))
          .with_label(declared, "declared with a `self` parameter here")
          .with_help(format!("call it on a value: `value.{name}(...)`")),
      );

      return None;
    }

    if is_private && self.owner != Some(item) {
      self.diagnostics.push(
        Diagnostic::error(codes::PRIVATE_MEMBER, span, format!("the function `{name}` of `{type_name}` is private"))
          .with_label(declared, "declared `prv` here"),
      );
    }

    Some(Res::Fn(function))
  }

  /// The types named in a type expression.
  fn type_expr(&mut self, ty: &TypeExpr) {
    match &ty.kind {
      TypeExprKind::Unit => {}
      TypeExprKind::Path(path) => self.type_path(path),
      TypeExprKind::Generic { path, args } => {
        self.type_path(path);

        for arg in args {
          self.type_expr(arg);
        }
      }
      TypeExprKind::Array { elem, .. } => self.type_expr(elem),
    }
  }

  /// The type of `new Type(...)`: recorded only when it names a program type, since the type
  /// checker reports every other type.
  fn new_path(&mut self, path: &Path) {
    if path.segments.len() > 1 {
      self.type_path(path);
    } else if let Some(segment) = path.segments.first()
      && let Some(ItemRef::Type(item)) = self.lookup_item(&segment.name)
    {
      self.type_paths.insert(path.id, TypeRes::Item(item));
    }
  }

  /// A type path. Numeric types come first, then program types and imports, then the prelude.
  fn type_path(&mut self, path: &Path) {
    let res = match path.segments.as_slice() {
      [segment] => {
        let name = segment.name.as_str();

        if NUMERIC_TYPES.contains(&name) || name == "Self" {
          TypeRes::Builtin
        } else {
          match self.lookup_item(name) {
            Some(ItemRef::Type(item)) => TypeRes::Item(item),
            Some(item) => {
              self.not_a_type(&segment.name, segment.span, item);

              TypeRes::Error
            }
            None if PRELUDE_TYPES.contains(&name) => TypeRes::Builtin,
            None => {
              let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("cannot find type `{name}`"));
              let items = self.item_names();
              let known = NUMERIC_TYPES.iter().chain(PRELUDE_TYPES).copied().chain(items.iter().map(String::as_str));

              if let Some(similar) = sagitario_diagnostics::closest_match(name, known) {
                diagnostic = diagnostic.with_help(format!("a type with a similar name exists: `{similar}`"));
              }

              self.diagnostics.push(diagnostic);

              TypeRes::Error
            }
          }
        }
      }
      segments => match self.qualified(segments) {
        Some((Named::Item(ItemRef::Type(item)), _, [])) => TypeRes::Item(item),
        Some((Named::Item(item), last, [])) => {
          self.not_a_type(&last.name, last.span, item);

          TypeRes::Error
        }
        Some((_, last, _)) => {
          self.error(path.span, format!("`{}` does not contain types", last.name));

          TypeRes::Error
        }
        None => TypeRes::Error,
      },
    };

    self.type_paths.insert(path.id, res);
  }

  fn not_a_type(&mut self, name: &str, span: Span, item: ItemRef) {
    let what = match item {
      ItemRef::Fn(_) => "a function",
      ItemRef::Module(_) => "a module",
      ItemRef::Type(_) => "a type",
    };

    self.error(span, format!("`{name}` is {what}, not a type"));
  }
}

#[cfg(test)]
mod tests;
