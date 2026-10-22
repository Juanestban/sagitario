//! Name resolution: builds the module's symbol table and binds every path expression to what it
//! names. Lookup order: parameters, module functions, then the prelude.

use std::collections::HashMap;

use sagitario_ast::{Expr, ExprKind, FnDecl, Item, Module, NodeId, Path, Stmt};
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

/// What a path expression refers to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Res {
  Fn(FnId),
  Builtin(Builtin),
  /// A parameter of the enclosing function, by position.
  Param(u32),
}

#[derive(Debug, Default)]
pub struct Resolution {
  /// Index in `Module::items` of each function, indexed by `FnId`.
  pub functions: Vec<usize>,
  pub paths: HashMap<NodeId, Res>,
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
  pub fn fn_decls<'m>(&self, module: &'m Module) -> impl Iterator<Item = (FnId, &'m FnDecl)> {
    (0..self.functions.len() as u32).filter_map(move |id| self.fn_decl(module, FnId(id)).map(|decl| (FnId(id), decl)))
  }
}

/// Names reserved for the future concurrency model (docs/spec/01-lexical.md).
const RESERVED_NAMES: &[&str] = &["Send", "Sync", "spawn"];

pub fn resolve(module: &Module, diagnostics: &mut Diagnostics) -> Resolution {
  let mut resolution = Resolution::default();
  let mut by_name: HashMap<&str, FnId> = HashMap::new();

  for (index, item) in module.items.iter().enumerate() {
    let Item::Fn(decl) = item;

    check_reserved(&decl.name.name, decl.name.span, diagnostics);

    let id = FnId(resolution.functions.len() as u32);

    resolution.functions.push(index);

    if by_name.contains_key(decl.name.name.as_str()) {
      diagnostics.push(Diagnostic::error(
        codes::DUPLICATE_DEFINITION,
        decl.name.span,
        format!("the function `{}` is defined more than once", decl.name.name),
      ));
    } else {
      by_name.insert(&decl.name.name, id);
    }
  }

  resolution.main = by_name.get("main").copied();

  if resolution.main.is_none() {
    diagnostics.push(
      Diagnostic::error(codes::MISSING_MAIN, Span::new(module.file, 0, 0), "the program has no `main` function").with_note("add `pub fn main() { }`"),
    );
  }

  let decls: Vec<&FnDecl> = resolution.fn_decls(module).map(|(_, decl)| decl).collect();

  for decl in decls {
    let mut params: Vec<&str> = Vec::new();

    for param in &decl.params {
      check_reserved(&param.name.name, param.name.span, diagnostics);

      if params.contains(&param.name.name.as_str()) {
        diagnostics.push(Diagnostic::error(
          codes::DUPLICATE_DEFINITION,
          param.name.span,
          format!("the parameter `{}` is declared more than once", param.name.name),
        ));
      }

      params.push(&param.name.name);
    }

    let mut resolver = Resolver { functions: &by_name, params: &params, paths: &mut resolution.paths, diagnostics };

    for Stmt::Expr(expr) in &decl.body.stmts {
      resolver.expr(expr);
    }

    if let Some(tail) = &decl.body.tail {
      resolver.expr(tail);
    }
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
  functions: &'a HashMap<&'a str, FnId>,
  params: &'a [&'a str],
  paths: &'a mut HashMap<NodeId, Res>,
  diagnostics: &'a mut Diagnostics,
}

impl Resolver<'_> {
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
      ExprKind::Int { .. } | ExprKind::Float { .. } | ExprKind::Bool(_) | ExprKind::Char(_) | ExprKind::Str(_) | ExprKind::Unit => {}
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
    // Later parameters with the same name shadow earlier ones.

    if let Some(index) = self.params.iter().rposition(|param| *param == name) {
      return Some(Res::Param(index as u32));
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
