//! Name resolution: builds the module's symbol table and binds every path expression to what it
//! names. Lookup order: local bindings (innermost scope first, later shadowing earlier), module
//! items, then the prelude. Functions and structs share one namespace per module; the members of
//! a struct share another one per struct.

use std::collections::HashMap;

use sagitario_ast::{Block, Expr, ExprKind, FnDecl, Item, Module, NodeId, Path, Stmt, StructDecl, TemplatePart, Visibility};
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

/// Index of a function (module function, struct method or static function) in
/// `Resolution::functions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnId(pub u32);

/// Index of a struct in `Resolution::structs`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct StructId(pub u32);

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
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalInfo {
  pub name: String,
  pub span: Span,
  pub kind: LocalKind,
}

/// What a path expression refers to. Struct literals resolve to `Struct`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
  Fn(FnId),
  Builtin(Builtin),
  Local(LocalId),
  Struct(StructId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FnInfo {
  /// Index in `Module::items` of the function, or of the struct that declares it.
  pub item: usize,
  /// Index in `StructDecl::functions` for struct functions.
  pub member: Option<usize>,
  pub owner: Option<StructId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StructInfo {
  /// Index in `Module::items`.
  pub item: usize,
  /// The struct's functions, in declaration order.
  pub functions: Vec<FnId>,
}

#[derive(Debug, Default)]
pub struct Resolution {
  /// Indexed by `FnId`.
  pub functions: Vec<FnInfo>,
  /// Indexed by `StructId`.
  pub structs: Vec<StructInfo>,
  pub struct_names: HashMap<String, StructId>,
  /// Local bindings of each function, indexed by `FnId` then `LocalId`.
  pub locals: Vec<Vec<LocalInfo>>,
  pub paths: HashMap<NodeId, Res>,
  /// The local declared by each `const`/`var` statement and `for` expression, by node id.
  pub bindings: HashMap<NodeId, LocalId>,
  pub main: Option<FnId>,
}

impl Resolution {
  pub fn fn_decl<'m>(&self, module: &'m Module, id: FnId) -> Option<&'m FnDecl> {
    let info = self.functions.get(id.0 as usize)?;

    match (module.items.get(info.item)?, info.member) {
      (Item::Fn(decl), None) => Some(decl),
      (Item::Struct(decl), Some(member)) => decl.functions.get(member),
      _ => None,
    }
  }

  /// Every function with its id, in declaration order.
  pub fn fn_decls<'m>(&self, module: &'m Module) -> Vec<(FnId, &'m FnDecl)> {
    (0..self.functions.len() as u32).filter_map(|id| self.fn_decl(module, FnId(id)).map(|decl| (FnId(id), decl))).collect()
  }

  /// The struct that declares a function.
  pub fn owner(&self, id: FnId) -> Option<StructId> {
    self.functions.get(id.0 as usize).and_then(|info| info.owner)
  }

  pub fn struct_decl<'m>(&self, module: &'m Module, id: StructId) -> Option<&'m StructDecl> {
    match module.items.get(self.structs.get(id.0 as usize)?.item)? {
      Item::Struct(decl) => Some(decl),
      Item::Fn(_) => None,
    }
  }

  /// A struct's function (method or static) by name.
  pub fn struct_function(&self, module: &Module, id: StructId, name: &str) -> Option<FnId> {
    let info = self.structs.get(id.0 as usize)?;

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
  Struct(StructId),
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
        let id = StructId(resolution.structs.len() as u32);
        let mut functions = Vec::new();

        for member in 0..decl.functions.len() {
          functions.push(FnId(resolution.functions.len() as u32));
          resolution.functions.push(FnInfo { item: index, member: Some(member), owner: Some(id) });
        }

        resolution.structs.push(StructInfo { item: index, functions });
        check_members(decl, diagnostics);

        (&decl.name, ItemRef::Struct(id))
      }
    };

    check_reserved(&name.name, name.span, diagnostics);

    if let Some((_, previous)) = items.get(&name.name) {
      diagnostics.push(
        Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("the name `{}` is defined more than once", name.name))
          .with_label(*previous, "previous definition here")
          .with_note("functions and structs share one namespace per module"),
      );
    } else {
      items.insert(name.name.clone(), (item_ref, name.span));

      if let ItemRef::Struct(id) = item_ref {
        resolution.struct_names.insert(name.name.clone(), id);
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
      structs: &resolution.structs,
      functions: &resolution.functions,
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

/// Fields and functions of a struct share one namespace.
fn check_members(decl: &StructDecl, diagnostics: &mut Diagnostics) {
  let mut seen: HashMap<&str, Span> = HashMap::new();
  let names = decl.fields.iter().map(|field| &field.name).chain(decl.functions.iter().map(|function| &function.name));

  for name in names {
    check_reserved(&name.name, name.span, diagnostics);

    if let Some(previous) = seen.get(name.name.as_str()) {
      diagnostics.push(
        Diagnostic::error(
          codes::DUPLICATE_DEFINITION,
          name.span,
          format!("the member `{}` is declared more than once in `{}`", name.name, decl.name.name),
        )
        .with_label(*previous, "previous declaration here")
        .with_note("fields and functions of a struct share one namespace"),
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

struct Resolver<'a> {
  module: &'a Module,
  structs: &'a [StructInfo],
  functions: &'a [FnInfo],
  items: &'a HashMap<String, ItemRef>,
  /// The struct whose function is being resolved, for `Self` and `prv` members.
  owner: Option<StructId>,
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
        if let Some(id) = self.struct_path(path) {
          self.paths.insert(expr.id, Res::Struct(id));
        }

        for field in fields {
          self.expr(&field.value);
        }
      }
      ExprKind::ArrayLit(elems) => self.exprs(elems),
      ExprKind::ArrayRepeat { value, .. } => self.expr(value),
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

  fn error(&mut self, span: Span, message: impl Into<String>) {
    self.diagnostics.push(Diagnostic::error(codes::UNRESOLVED_NAME, span, message));
  }

  fn path(&mut self, path: &Path) -> Option<Res> {
    match path.segments.as_slice() {
      [segment] => self.value_name(&segment.name, segment.span),
      [owner, function] => {
        let id = self.struct_name(&owner.name, owner.span)?;

        self.struct_function(id, &function.name, function.span)
      }
      _ => {
        self.diagnostics.push(Diagnostic::error(
          codes::NOT_IMPLEMENTED,
          path.span,
          "paths with more than two segments are not implemented yet (modules arrive in F1 increment 7)",
        ));

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
            .with_help("add a `self` parameter to make this function a method of its struct"),
        );

        return None;
      }
      ("Self", _) => {
        self.error(span, "`Self` is a type, not a value");

        return None;
      }
      (_, Some(ItemRef::Fn(id))) => return Some(Res::Fn(*id)),
      (_, Some(ItemRef::Struct(_))) => {
        self.diagnostics.push(
          Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("`{name}` is a struct, not a value"))
            .with_help(format!("build a value with `{name} {{ ... }}` or call one of its static functions, such as `{name}::new(...)`")),
        );

        return None;
      }
      (_, None) => {}
    }

    if let Some(builtin) = Builtin::from_name(name) {
      return Some(Res::Builtin(builtin));
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

  /// A name used as a struct: `Self` or a struct of the module.
  fn struct_name(&mut self, name: &str, span: Span) -> Option<StructId> {
    if name == "Self" {
      if self.owner.is_none() {
        self.error(span, "`Self` is only available inside a struct");
      }

      return self.owner;
    }

    match self.items.get(name) {
      Some(ItemRef::Struct(id)) => Some(*id),
      Some(ItemRef::Fn(_)) => {
        self.error(span, format!("`{name}` is a function, not a struct"));

        None
      }
      None => {
        let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("cannot find struct `{name}` in this scope"));
        let structs = self.items.iter().filter(|(_, item)| matches!(item, ItemRef::Struct(_))).map(|(name, _)| name.as_str());

        if let Some(similar) = sagitario_diagnostics::closest_match(name, structs) {
          diagnostic = diagnostic.with_help(format!("a struct with a similar name exists: `{similar}`"));
        }

        self.diagnostics.push(diagnostic);

        None
      }
    }
  }

  fn struct_path(&mut self, path: &Path) -> Option<StructId> {
    match path.segments.as_slice() {
      [segment] => self.struct_name(&segment.name, segment.span),
      _ => {
        self.diagnostics.push(Diagnostic::error(
          codes::NOT_IMPLEMENTED,
          path.span,
          "struct literals with a path are not implemented yet (enums and modules arrive later)",
        ));

        None
      }
    }
  }

  fn struct_decl(&self, id: StructId) -> Option<&StructDecl> {
    match self.module.items.get(self.structs.get(id.0 as usize)?.item)? {
      Item::Struct(decl) => Some(decl),
      Item::Fn(_) => None,
    }
  }

  /// `Type::function`: a static function of a struct.
  fn struct_function(&mut self, id: StructId, name: &str, span: Span) -> Option<Res> {
    let decl = self.struct_decl(id)?;
    let struct_name = decl.name.name.clone();
    let found = decl.functions.iter().position(|function| function.name.name == name);
    let Some(member) = found else {
      let mut diagnostic = Diagnostic::error(codes::UNRESOLVED_NAME, span, format!("no function `{name}` in `{struct_name}`"));
      let candidates = decl.functions.iter().map(|function| function.name.name.as_str());

      if let Some(similar) = sagitario_diagnostics::closest_match(name, candidates) {
        diagnostic = diagnostic.with_help(format!("a function with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);

      return None;
    };
    let function = decl.functions.get(member)?;
    let (is_method, is_private, declared) = (function.self_param.is_some(), function.visibility == Visibility::Private, function.name.span);

    if is_method {
      self.diagnostics.push(
        Diagnostic::error(codes::WRONG_CALL_FORM, span, format!("`{name}` is a method of `{struct_name}`, not a static function"))
          .with_label(declared, "declared with a `self` parameter here")
          .with_help(format!("call it on a value: `value.{name}(...)`")),
      );

      return None;
    }

    if is_private && self.owner != Some(id) {
      self.diagnostics.push(
        Diagnostic::error(codes::PRIVATE_MEMBER, span, format!("the function `{name}` of `{struct_name}` is private"))
          .with_label(declared, "declared `prv` here"),
      );
    }

    self.functions.iter().position(|info| info.owner == Some(id) && info.member == Some(member)).map(|index| Res::Fn(FnId(index as u32)))
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
    assert_eq!(resolution.struct_function(&module, StructId(0), "get"), Some(FnId(1)));
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
}
