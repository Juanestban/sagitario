//! Name resolution: builds the module's symbol table and binds every path expression to what it
//! names. Lookup order: local bindings (innermost scope first, later shadowing earlier), module
//! functions, then the prelude.

use std::collections::HashMap;

use sagitario_ast::{Block, Expr, ExprKind, FnDecl, Item, Module, NodeId, Path, Stmt, TemplatePart};
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

/// Index of a function in `Resolution::functions`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FnId(pub u32);

/// A local binding of one function: parameters come first, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LocalId(pub u32);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LocalKind {
  Param,
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

/// What a path expression refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
  Fn(FnId),
  Builtin(Builtin),
  Local(LocalId),
}

#[derive(Debug, Default)]
pub struct Resolution {
  /// Index in `Module::items` of each function, indexed by `FnId`.
  pub functions: Vec<usize>,
  /// Local bindings of each function, indexed by `FnId` then `LocalId`.
  pub locals: Vec<Vec<LocalInfo>>,
  pub paths: HashMap<NodeId, Res>,
  /// The local declared by each `const`/`var` statement and `for` expression, by node id.
  pub bindings: HashMap<NodeId, LocalId>,
  pub main: Option<FnId>,
}

impl Resolution {
  pub fn fn_decl<'m>(&self, module: &'m Module, id: FnId) -> Option<&'m FnDecl> {
    let index = *self.functions.get(id.0 as usize)?;

    match module.items.get(index)? {
      Item::Fn(decl) => Some(decl),
    }
  }

  /// Every function with its id, in declaration order.
  pub fn fn_decls<'m>(&self, module: &'m Module) -> Vec<(FnId, &'m FnDecl)> {
    (0..self.functions.len() as u32).filter_map(|id| self.fn_decl(module, FnId(id)).map(|decl| (FnId(id), decl))).collect()
  }

  pub fn locals(&self, id: FnId) -> &[LocalInfo] {
    self.locals.get(id.0 as usize).map_or(&[], Vec::as_slice)
  }
}

/// Names reserved for the future concurrency model (docs/spec/01-lexical.md).
const RESERVED_NAMES: &[&str] = &["Send", "Sync", "spawn"];

pub fn resolve(module: &Module, diagnostics: &mut Diagnostics) -> Resolution {
  let mut resolution = Resolution::default();
  let mut by_name: HashMap<String, FnId> = HashMap::new();

  for (index, item) in module.items.iter().enumerate() {
    let Item::Fn(decl) = item;

    check_reserved(&decl.name.name, decl.name.span, diagnostics);

    let id = FnId(resolution.functions.len() as u32);

    resolution.functions.push(index);

    if by_name.contains_key(&decl.name.name) {
      diagnostics.push(Diagnostic::error(
        codes::DUPLICATE_DEFINITION,
        decl.name.span,
        format!("the function `{}` is defined more than once", decl.name.name),
      ));
    } else {
      by_name.insert(decl.name.name.clone(), id);
    }
  }

  resolution.main = by_name.get("main").copied();

  if resolution.main.is_none() {
    diagnostics.push(
      Diagnostic::error(codes::MISSING_MAIN, Span::new(module.file, 0, 0), "the program has no `main` function").with_note("add `pub fn main() { }`"),
    );
  }

  for (_, decl) in resolution.fn_decls(module) {
    let mut resolver = Resolver {
      functions: &by_name,
      locals: Vec::new(),
      scopes: vec![Vec::new()],
      paths: &mut resolution.paths,
      bindings: &mut resolution.bindings,
      diagnostics,
    };

    for param in &decl.params {
      let duplicate = resolver.scopes.last().is_some_and(|scope| scope.iter().any(|(name, _)| *name == param.name.name));

      if duplicate {
        resolver.diagnostics.push(Diagnostic::error(
          codes::DUPLICATE_DEFINITION,
          param.name.span,
          format!("the parameter `{}` is declared more than once", param.name.name),
        ));
      }

      resolver.declare(&param.name.name, param.name.span, LocalKind::Param);
    }

    resolver.block(&decl.body);

    let locals = resolver.locals;

    resolution.locals.push(locals);
  }

  resolution
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
  functions: &'a HashMap<String, FnId>,
  locals: Vec<LocalInfo>,
  /// Names visible in each nested block, innermost last.
  scopes: Vec<Vec<(String, LocalId)>>,
  paths: &'a mut HashMap<NodeId, Res>,
  bindings: &'a mut HashMap<NodeId, LocalId>,
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

  fn expr(&mut self, expr: &Expr) {
    match &expr.kind {
      ExprKind::Path(path) => {
        if let Some(res) = self.path(path) {
          self.paths.insert(expr.id, res);
        }
      }
      ExprKind::Call { callee, args } => {
        self.expr(callee);

        for arg in args {
          self.expr(arg);
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

  fn path(&mut self, path: &Path) -> Option<Res> {
    let [segment] = path.segments.as_slice() else {
      self.diagnostics.push(Diagnostic::error(
        codes::NOT_IMPLEMENTED,
        path.span,
        "paths with `::` are not implemented yet (modules arrive in F1 increment 7)",
      ));

      return None;
    };
    let name = segment.name.as_str();

    if let Some(id) = self.lookup_local(name) {
      return Some(Res::Local(id));
    }

    if let Some(id) = self.functions.get(name) {
      return Some(Res::Fn(*id));
    }

    if let Some(builtin) = Builtin::from_name(name) {
      return Some(Res::Builtin(builtin));
    }

    self.diagnostics.push(Diagnostic::error(codes::UNRESOLVED_NAME, segment.span, format!("cannot find `{name}` in this scope")));

    None
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
}
