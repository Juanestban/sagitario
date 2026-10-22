//! Name resolution: builds the module's symbol table and binds every path expression and pattern
//! to what it names. Lookup order: local bindings (innermost scope first, later shadowing
//! earlier), module items, then the prelude. Functions, structs and enums share one namespace
//! per module; the members of a struct (fields and functions) or of an enum (variants and
//! functions) share another one per type.

use std::collections::HashMap;

use sagitario_ast::{
  Block, EnumDecl, Expr, ExprKind, FnDecl, Item, Module, NodeId, Path, Pattern, PatternKind, Stmt, StructDecl, TemplatePart, VariantFields,
  Visibility,
};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};
use sagitario_span::Span;

/// Prelude functions implemented by the runtime (docs/spec/09-modules.md#prelude).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Builtin {
  Print,
  Println,
  Eprintln,
}

impl Builtin {
  const ALL: [Builtin; 3] = [Builtin::Print, Builtin::Println, Builtin::Eprintln];

  pub fn name(self) -> &'static str {
    match self {
      Builtin::Print => "print",
      Builtin::Println => "println",
      Builtin::Eprintln => "eprintln",
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FnInfo {
  /// Index in `Module::items` of the function, or of the type that declares it.
  pub item: usize,
  /// Index in the declaring type's `functions`.
  pub member: Option<usize>,
  pub owner: Option<TypeItem>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeInfo {
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
  /// Structs and enums by name.
  pub type_names: HashMap<String, TypeItem>,
  /// Local bindings of each function, indexed by `FnId` then `LocalId`.
  pub locals: Vec<Vec<LocalInfo>>,
  /// Path expressions, struct literals, variant constructors and variant patterns, by node id.
  pub paths: HashMap<NodeId, Res>,
  /// The local declared by each `const`/`var` statement, `for` expression and binding pattern.
  pub bindings: HashMap<NodeId, LocalId>,
  pub main: Option<FnId>,
}

impl Resolution {
  pub fn fn_decl<'m>(&self, module: &'m Module, id: FnId) -> Option<&'m FnDecl> {
    let info = self.functions.get(id.0 as usize)?;

    match (module.items.get(info.item)?, info.member) {
      (Item::Fn(decl), None) => Some(decl),
      (Item::Struct(decl), Some(member)) => decl.functions.get(member),
      (Item::Enum(decl), Some(member)) => decl.functions.get(member),
      _ => None,
    }
  }

  /// Every function with its id, in declaration order.
  pub fn fn_decls<'m>(&self, module: &'m Module) -> Vec<(FnId, &'m FnDecl)> {
    (0..self.functions.len() as u32).filter_map(|id| self.fn_decl(module, FnId(id)).map(|decl| (FnId(id), decl))).collect()
  }

  /// The type that declares a function.
  pub fn owner(&self, id: FnId) -> Option<TypeItem> {
    self.functions.get(id.0 as usize).and_then(|info| info.owner)
  }

  pub fn struct_decl<'m>(&self, module: &'m Module, id: StructId) -> Option<&'m StructDecl> {
    match module.items.get(self.structs.get(id.0 as usize)?.item)? {
      Item::Struct(decl) => Some(decl),
      _ => None,
    }
  }

  pub fn enum_decl<'m>(&self, module: &'m Module, id: EnumId) -> Option<&'m EnumDecl> {
    match module.items.get(self.enums.get(id.0 as usize)?.item)? {
      Item::Enum(decl) => Some(decl),
      _ => None,
    }
  }

  /// The name of a struct or enum.
  pub fn type_name(&self, module: &Module, item: TypeItem) -> String {
    match item {
      TypeItem::Struct(id) => self.struct_decl(module, id).map(|decl| decl.name.name.clone()),
      TypeItem::Enum(id) => self.enum_decl(module, id).map(|decl| decl.name.name.clone()),
    }
    .unwrap_or_default()
  }

  /// A type's function (method or static) by name.
  pub fn type_function(&self, module: &Module, item: TypeItem, name: &str) -> Option<FnId> {
    let info = match item {
      TypeItem::Struct(id) => self.structs.get(id.0 as usize),
      TypeItem::Enum(id) => self.enums.get(id.0 as usize),
    }?;

    info.functions.iter().copied().find(|function| self.fn_decl(module, *function).is_some_and(|decl| decl.name.name == name))
  }

  pub fn locals(&self, id: FnId) -> &[LocalInfo] {
    self.locals.get(id.0 as usize).map_or(&[], Vec::as_slice)
  }
}

/// Names reserved for the future concurrency model (docs/spec/01-lexical.md).
const RESERVED_NAMES: &[&str] = &["Send", "Sync", "spawn"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemRef {
  Fn(FnId),
  Type(TypeItem),
}

pub fn resolve(module: &Module, diagnostics: &mut Diagnostics) -> Resolution {
  let mut resolution = Resolution::default();
  let mut items: HashMap<String, (ItemRef, Span)> = HashMap::new();

  for (index, item) in module.items.iter().enumerate() {
    let (name, item_ref) = match item {
      Item::Fn(decl) => {
        let id = FnId(resolution.functions.len() as u32);

        resolution.functions.push(FnInfo { item: index, member: None, owner: None });

        (&decl.name, ItemRef::Fn(id))
      }
      Item::Struct(decl) => {
        let owner = TypeItem::Struct(StructId(resolution.structs.len() as u32));
        let functions = register_functions(&mut resolution, index, decl.functions.len(), owner);

        resolution.structs.push(TypeInfo { item: index, functions });

        let names = decl.fields.iter().map(|field| &field.name).chain(decl.functions.iter().map(|function| &function.name));

        check_members(&decl.name.name, "fields and functions of a struct share one namespace", names, diagnostics);

        (&decl.name, ItemRef::Type(owner))
      }
      Item::Enum(decl) => {
        let owner = TypeItem::Enum(EnumId(resolution.enums.len() as u32));
        let functions = register_functions(&mut resolution, index, decl.functions.len(), owner);

        resolution.enums.push(TypeInfo { item: index, functions });

        let names = decl.variants.iter().map(|variant| &variant.name).chain(decl.functions.iter().map(|function| &function.name));

        check_members(&decl.name.name, "variants and functions of an enum share one namespace", names, diagnostics);

        for variant in &decl.variants {
          if let VariantFields::Named(fields) = &variant.fields {
            check_members(&variant.name.name, "the fields of a variant need different names", fields.iter().map(|field| &field.name), diagnostics);
          }
        }

        (&decl.name, ItemRef::Type(owner))
      }
    };

    check_reserved(&name.name, name.span, diagnostics);

    if let Some((_, previous)) = items.get(&name.name) {
      diagnostics.push(
        Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("the name `{}` is defined more than once", name.name))
          .with_label(*previous, "previous definition here")
          .with_note("functions, structs and enums share one namespace per module"),
      );
    } else {
      items.insert(name.name.clone(), (item_ref, name.span));

      if let ItemRef::Type(item) = item_ref {
        resolution.type_names.insert(name.name.clone(), item);
      }
    }
  }

  resolution.main = match items.get("main") {
    Some((ItemRef::Fn(id), _)) => Some(*id),
    _ => None,
  };

  if resolution.main.is_none() {
    diagnostics.push(
      Diagnostic::error(codes::MISSING_MAIN, Span::new(module.file, 0, 0), "the program has no `main` function").with_note("add `pub fn main() { }`"),
    );
  }

  let items: HashMap<String, ItemRef> = items.into_iter().map(|(name, (item, _))| (name, item)).collect();

  for (id, decl) in resolution.fn_decls(module) {
    let owner = resolution.owner(id);
    let mut resolver = Resolver {
      module,
      resolution: &resolution,
      items: &items,
      owner,
      locals: Vec::new(),
      scopes: vec![Vec::new()],
      paths: HashMap::new(),
      bindings: HashMap::new(),
      diagnostics,
    };

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

      resolver.declare(&param.name.name, param.name.span, LocalKind::Param);
    }

    resolver.block(&decl.body);

    let Resolver { locals, paths, bindings, .. } = resolver;

    resolution.locals.push(locals);
    resolution.paths.extend(paths);
    resolution.bindings.extend(bindings);
  }

  resolution
}

fn register_functions(resolution: &mut Resolution, item: usize, count: usize, owner: TypeItem) -> Vec<FnId> {
  let mut functions = Vec::new();

  for member in 0..count {
    functions.push(FnId(resolution.functions.len() as u32));
    resolution.functions.push(FnInfo { item, member: Some(member), owner: Some(owner) });
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

/// The first segment of a two-segment path: a type of the program or a prelude enum.
#[derive(Clone, Copy)]
enum PathOwner {
  Type(TypeItem),
  Prelude(EnumKind),
}

/// A name bound by a pattern, for checking the alternatives of `|`.
type PatternBinding = (String, LocalId, Span);

struct Resolver<'a> {
  module: &'a Module,
  resolution: &'a Resolution,
  items: &'a HashMap<String, ItemRef>,
  /// The type whose function is being resolved, for `Self` and `prv` members.
  owner: Option<TypeItem>,
  locals: Vec<LocalInfo>,
  /// Names visible in each nested block, innermost last.
  scopes: Vec<Vec<(String, LocalId)>>,
  paths: HashMap<NodeId, Res>,
  bindings: HashMap<NodeId, LocalId>,
  diagnostics: &'a mut Diagnostics,
}

impl Resolver<'_> {
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

  fn block(&mut self, block: &Block) {
    self.scopes.push(Vec::new());

    for stmt in &block.stmts {
      match stmt {
        Stmt::Let(stmt) => {
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
      ExprKind::Cast { expr, .. } => self.expr(expr),
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
        if let Some(variant) = prelude_variant(&name.name).filter(|_| !self.items.contains_key(&name.name)) {
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

  /// A path that must name a variant: `Some`, `Shape::Circle`, `Self::Empty`.
  fn variant_path(&mut self, path: &Path) -> Option<VariantRef> {
    match path.segments.as_slice() {
      [segment] => {
        let variant = prelude_variant(&segment.name).filter(|_| !self.items.contains_key(&segment.name));

        if variant.is_none() {
          self.error(segment.span, format!("`{}` is not a variant", segment.name));
        }

        variant
      }
      [owner, name] => match self.path_owner(&owner.name, owner.span)? {
        PathOwner::Type(TypeItem::Enum(id)) => self.user_variant(id, &name.name, name.span),
        PathOwner::Prelude(kind) => self.prelude_variant_of(kind, &name.name, name.span),
        PathOwner::Type(TypeItem::Struct(_)) => {
          self.error(owner.span, format!("`{}` is a struct, not an enum", owner.name));

          None
        }
      },
      _ => {
        self.not_implemented_path(path.span);

        None
      }
    }
  }

  fn user_variant(&mut self, id: EnumId, name: &str, span: Span) -> Option<VariantRef> {
    let decl = self.resolution.enum_decl(self.module, id)?;
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

  fn not_implemented_path(&mut self, span: Span) {
    self.diagnostics.push(Diagnostic::error(
      codes::NOT_IMPLEMENTED,
      span,
      "paths with more than two segments are not implemented yet (modules arrive in F1 increment 7)",
    ));
  }

  fn error(&mut self, span: Span, message: impl Into<String>) {
    self.diagnostics.push(Diagnostic::error(codes::UNRESOLVED_NAME, span, message));
  }

  fn path(&mut self, path: &Path) -> Option<Res> {
    match path.segments.as_slice() {
      [segment] => self.value_name(&segment.name, segment.span),
      [owner, member] => match self.path_owner(&owner.name, owner.span)? {
        PathOwner::Type(item) => {
          if let TypeItem::Enum(id) = item
            && let Some(decl) = self.resolution.enum_decl(self.module, id)
            && let Some(index) = decl.variants.iter().position(|variant| variant.name.name == member.name)
          {
            return Some(Res::Variant(VariantRef { owner: EnumKind::User(id), index: index as u32 }));
          }

          self.type_function(item, &member.name, member.span)
        }
        PathOwner::Prelude(kind) => self.prelude_variant_of(kind, &member.name, member.span).map(Res::Variant),
      },
      _ => {
        self.not_implemented_path(path.span);

        None
      }
    }
  }

  /// A single name used as a value.
  fn value_name(&mut self, name: &str, span: Span) -> Option<Res> {
    if let Some(id) = self.lookup_local(name) {
      return Some(Res::Local(id));
    }

    match (name, self.items.get(name)) {
      ("self", _) => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, span, "`self` is only available in methods")
            .with_help("add a `self` parameter to make this function a method of its type"),
        );

        return None;
      }
      ("Self", _) => {
        self.error(span, "`Self` is a type, not a value");

        return None;
      }
      (_, Some(ItemRef::Fn(id))) => return Some(Res::Fn(*id)),
      (_, Some(ItemRef::Type(TypeItem::Struct(_)))) => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("`{name}` is a struct, not a value"))
            .with_help(format!("build a value with `{name} {{ ... }}` or call one of its static functions, such as `{name}::new(...)`")),
        );

        return None;
      }
      (_, Some(ItemRef::Type(TypeItem::Enum(_)))) => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("`{name}` is an enum, not a value"))
            .with_help(format!("name one of its variants: `{name}::Variant`")),
        );

        return None;
      }
      (_, None) => {}
    }

    if let Some(builtin) = Builtin::from_name(name) {
      return Some(Res::Builtin(builtin));
    }

    if let Some(variant) = prelude_variant(name) {
      return Some(Res::Variant(variant));
    }

    let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("cannot find `{name}` in this scope"));
    let visible = self.scopes.iter().flatten().map(|(local, _)| local.as_str());
    let candidates = visible.chain(self.items.keys().map(String::as_str)).chain(Builtin::ALL.iter().map(|builtin| builtin.name()));

    if let Some(similar) = sagitario_diagnostics::closest_match(name, candidates) {
      diagnostic = diagnostic.with_help(format!("a name with a similar spelling exists: `{similar}`"));
    }

    self.diagnostics.push(diagnostic);

    None
  }

  /// The first segment of `Owner::member`: `Self`, a struct or enum, or a prelude enum.
  fn path_owner(&mut self, name: &str, span: Span) -> Option<PathOwner> {
    if name == "Self" {
      if self.owner.is_none() {
        self.error(span, "`Self` is only available inside a struct or enum");
      }

      return self.owner.map(PathOwner::Type);
    }

    match self.items.get(name) {
      Some(ItemRef::Type(item)) => Some(PathOwner::Type(*item)),
      Some(ItemRef::Fn(_)) => {
        self.error(span, format!("`{name}` is a function, not a type"));

        None
      }
      None if name == "Option" => Some(PathOwner::Prelude(EnumKind::Option)),
      None if name == "Result" => Some(PathOwner::Prelude(EnumKind::Result)),
      None => {
        let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("cannot find type `{name}` in this scope"));
        let types = self.items.iter().filter(|(_, item)| matches!(item, ItemRef::Type(_))).map(|(name, _)| name.as_str());

        if let Some(similar) = sagitario_diagnostics::closest_match(name, types) {
          diagnostic = diagnostic.with_help(format!("a type with a similar name exists: `{similar}`"));
        }

        self.diagnostics.push(diagnostic);

        None
      }
    }
  }

  /// The path of `Name { ... }`: a struct, or an enum variant with named fields.
  fn struct_literal_path(&mut self, path: &Path) -> Option<Res> {
    match path.segments.as_slice() {
      [segment] => match self.path_owner(&segment.name, segment.span)? {
        PathOwner::Type(TypeItem::Struct(id)) => Some(Res::Struct(id)),
        PathOwner::Type(TypeItem::Enum(_)) | PathOwner::Prelude(_) => {
          self.diagnostics.push(
            Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("`{}` is an enum, not a struct", segment.name))
              .with_help("name the variant: `Enum::Variant { ... }`"),
          );

          None
        }
      },
      [_, _] => self.variant_path(path).map(Res::Variant),
      _ => {
        self.not_implemented_path(path.span);

        None
      }
    }
  }

  /// `Type::function`: a static function of a struct or enum.
  fn type_function(&mut self, item: TypeItem, name: &str, span: Span) -> Option<Res> {
    let type_name = self.resolution.type_name(self.module, item);
    let Some(function) = self.resolution.type_function(self.module, item, name) else {
      let functions = match item {
        TypeItem::Struct(id) => self.resolution.struct_decl(self.module, id).map(|decl| &decl.functions),
        TypeItem::Enum(id) => self.resolution.enum_decl(self.module, id).map(|decl| &decl.functions),
      };
      let mut candidates: Vec<String> = functions.into_iter().flatten().map(|function| function.name.name.clone()).collect();
      let what = match item {
        TypeItem::Struct(_) => "function",
        TypeItem::Enum(id) => {
          let variants = self.resolution.enum_decl(self.module, id).map(|decl| &decl.variants);

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
    let decl = self.resolution.fn_decl(self.module, function)?;
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
}

#[cfg(test)]
mod tests {
  use super::*;
  use sagitario_span::SourceMap;
  use std::path::PathBuf;

  fn resolve_text(text: &str) -> (Module, Resolution, Vec<&'static str>) {
    let mut sources = SourceMap::new();
    let file = sources.add(PathBuf::from("test.sag"), text.to_string());
    let mut diagnostics = Diagnostics::new();
    let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
    let module = sagitario_parser::parse_module(file, tokens, &mut diagnostics);

    assert!(!diagnostics.has_errors(), "parse failed: {:?}", diagnostics.take());

    let resolution = resolve(&module, &mut diagnostics);
    let errors = diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect();

    (module, resolution, errors)
  }

  #[test]
  fn resolves_shadowing_and_block_scopes() {
    let (_, resolution, errors) = resolve_text("pub fn main() { const x = 1; const x = x + 1; { const y = x; } for i in 0..x { println(\"\"); } }");

    assert!(errors.is_empty(), "{errors:?}");

    let names: Vec<&str> = resolution.locals(FnId(0)).iter().map(|local| local.name.as_str()).collect();

    assert_eq!(names, vec!["x", "x", "y", "i"]);

    let mut local_refs: Vec<u32> = resolution.paths.values().filter_map(|res| if let Res::Local(id) = res { Some(id.0) } else { None }).collect();

    local_refs.sort();
    // `x + 1` reads the first `x`; `y = x` and `0..x` read the second.
    assert_eq!(local_refs, vec![0, 1, 1]);
  }

  #[test]
  fn bindings_end_with_their_block() {
    assert_eq!(resolve_text("pub fn main() { { const y = 1; } y; }").2, vec!["E0200"]);
    assert_eq!(resolve_text("pub fn main() { for i in 0..3 {} i; }").2, vec!["E0200"]);
  }

  #[test]
  fn reports_duplicates_reserved_names_and_missing_main() {
    assert_eq!(resolve_text("fn f() {}\nfn f() {}\npub fn main() {}").2, vec!["E0201"]);
    assert_eq!(resolve_text("fn f(a: i32, a: i32) {}\npub fn main() {}").2, vec!["E0201"]);
    assert_eq!(resolve_text("pub fn main() { const spawn = 1; }").2, vec!["E0202"]);
    assert_eq!(resolve_text("fn f() {}").2, vec!["E0203"]);
  }

  #[test]
  fn resolves_structs_and_their_functions() {
    let program = "
      struct P {
        pub x: i32;
        pub fn new() -> Self { Self { x: 0 } }
        pub fn get(self) -> i32 { self.x }
        prv fn hidden() {}
        pub fn call_hidden() { Self::hidden(); }
      }
      pub fn main() { const p = P::new(); const q = P { x: p.get() }; }";
    let (module, resolution, errors) = resolve_text(program);

    assert!(errors.is_empty(), "{errors:?}");
    assert_eq!(resolution.functions.len(), 5);
    assert_eq!(resolution.type_function(&module, TypeItem::Struct(StructId(0)), "get"), Some(FnId(1)));
    assert_eq!(resolution.main, Some(FnId(4)));
    assert_eq!(resolution.locals(FnId(1)).first().map(|local| local.kind), Some(LocalKind::SelfParam));
  }

  #[test]
  fn reports_struct_name_errors() {
    let with_struct = |main: &str| resolve_text(&format!("struct P {{ pub fn m(self) {{}} prv fn s() {{}} }}\npub fn main() {{ {main} }}")).2;

    assert_eq!(resolve_text("struct P {}\nfn P() {}\npub fn main() {}").2, vec!["E0201"]);
    assert_eq!(resolve_text("struct P { pub x: i32; pub fn x() {} }\npub fn main() {}").2, vec!["E0201"]);
    assert_eq!(with_struct("P::m();"), vec!["E0314"]);
    assert_eq!(with_struct("P::s();"), vec!["E0205"]);
    assert_eq!(with_struct("P::nope();"), vec!["E0200"]);
    assert_eq!(with_struct("const p = P;"), vec!["E0200"]);
    assert_eq!(with_struct("self;"), vec!["E0200"]);
    assert_eq!(with_struct("Self::m();"), vec!["E0200"]);
    assert_eq!(with_struct("const q = Q {};"), vec!["E0200"]);
  }

  #[test]
  fn resolves_variants_and_patterns() {
    let program = "
      enum Shape { Circle(f64), Rect { w: f64, h: f64 }, Empty }
      pub fn main() {
        const s = Shape::Circle(1.0);
        const _o = Some(1);
        match s {
          Shape::Circle(r) | Shape::Rect { w: r, .. } => println(`${r}`),
          Shape::Empty => {}
        }
        match Option::Some(2) { None => {}, Some(x) => println(`${x}`) }
      }";
    let (_, resolution, errors) = resolve_text(program);

    assert!(errors.is_empty(), "{errors:?}");

    let variants = resolution.paths.values().filter(|res| matches!(res, Res::Variant(_))).count();

    // `Circle(1.0)`, `Some(1)`, three variant patterns, `Option::Some`, `None` and `Some(x)`.
    assert_eq!(variants, 8);

    let names: Vec<&str> = resolution.locals(FnId(0)).iter().map(|local| local.name.as_str()).collect();

    assert_eq!(names, vec!["s", "_o", "r", "x"]);
  }

  #[test]
  fn reports_variant_and_pattern_errors() {
    let with_enum = |main: &str| resolve_text(&format!("enum E {{ A(i32), B }}\npub fn main() {{ {main} }}")).2;

    assert_eq!(resolve_text("enum E { A, A }\npub fn main() {}").2, vec!["E0201"]);
    assert_eq!(with_enum("const _ = E::C;"), vec!["E0200"]);
    assert_eq!(with_enum("const _ = E;"), vec!["E0200"]);
    assert_eq!(with_enum("match E::B { E::A(x) | E::B => {} }"), vec!["E0206"]);
    assert_eq!(with_enum("match E::B { E::A(x) | E::A(y) => {} _ => {} }"), vec!["E0206", "E0206"]);
    assert_eq!(
      resolve_text(
        "enum P { Two(i32, i32) }
pub fn main() { match P::Two(1, 2) { P::Two(x, x) => {} } }"
      )
      .2,
      vec!["E0201"]
    );
    assert_eq!(with_enum("match 1 { Foo(x) => {} }"), vec!["E0200"]);
  }
}
