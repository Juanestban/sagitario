//! The module tree of a package, `use` declarations, the warnings about imports and visibility,
//! and the discovery of the modules a file refers to (docs/spec/09-modules.md).

use std::collections::{BTreeSet, HashSet};

use crate::{Imported, ItemRef, ModId, ModuleInfo, Named, Resolution, Resolver, TypeItem, TypeRes};
use sagitario_ast::{
  Block, ClassDecl, Expr, ExprKind, FnDecl, Ident, InterfaceDecl, Item, Module, Package, Path, Pattern, PatternKind, Stmt, StructDecl, TemplatePart,
  TypeExpr, TypeExprKind, UseTree, UseTreeKind, VariantFields, Visibility,
};
use sagitario_diagnostics::{Diagnostic, Diagnostics, codes};

/// Creates a module for every file of the package and for the directories above them. Returns
/// the module of each file, by its index in `Package::modules`.
pub(crate) fn build_tree(resolution: &mut Resolution, package: &Package) -> Vec<ModId> {
  resolution.modules.push(ModuleInfo::default());

  let mut ids = Vec::new();

  for (source, module) in package.modules.iter().enumerate() {
    // The prelude is a module of its own, outside the tree of `pkg`.
    if package.prelude == Some(source) {
      let node = ModId(resolution.modules.len() as u32);

      resolution.modules.push(ModuleInfo { path: module.path.clone(), source: Some(source), ..ModuleInfo::default() });
      resolution.prelude = Some(node);
      ids.push(node);

      continue;
    }

    let mut node = ModId(0);

    for segment in &module.path {
      node = child_module(resolution, node, segment);
    }

    if let Some(info) = resolution.modules.get_mut(node.0 as usize) {
      info.source = Some(source);
    }

    ids.push(node);
  }

  ids
}

/// The submodule `name` of `parent`, created if needed.
fn child_module(resolution: &mut Resolution, parent: ModId, name: &str) -> ModId {
  let Some(info) = resolution.module(parent) else { return parent };

  if let Some(child) = info.children.get(name) {
    return *child;
  }

  let child = ModId(resolution.modules.len() as u32);
  let mut path = info.path.clone();

  path.push(name.to_string());
  resolution.modules.push(ModuleInfo { path, parent: Some(parent), ..ModuleInfo::default() });

  if let Some(info) = resolution.modules.get_mut(parent.0 as usize) {
    info.children.insert(name.to_string(), child);
  }

  child
}

/// One name imported by a `use` declaration: the path to it and its alias.
struct FlatImport<'a> {
  segments: Vec<&'a Ident>,
  alias: Option<&'a Ident>,
}

/// Expands the groups of a `use` tree; a trailing `self` inside braces names the prefix.
fn flatten<'a>(tree: &'a UseTree, prefix: &[&'a Ident], out: &mut Vec<FlatImport<'a>>) {
  let mut segments: Vec<&Ident> = prefix.to_vec();

  segments.extend(tree.prefix.iter());

  match &tree.kind {
    UseTreeKind::Simple { alias } => {
      if segments.len() > 1 && segments.last().is_some_and(|segment| segment.name == "self") {
        segments.pop();
      }

      out.push(FlatImport { segments, alias: alias.as_ref() });
    }
    UseTreeKind::Group(trees) => {
      for tree in trees {
        flatten(tree, &segments, out);
      }
    }
  }
}

/// Resolves the `use` declarations of one module and adds the imported names to it.
pub(crate) fn resolve_imports(resolution: &mut Resolution, package: &Package, node: ModId, module: &Module, diagnostics: &mut Diagnostics) {
  let mut flat = Vec::new();

  for decl in &module.uses {
    flatten(&decl.tree, &[], &mut flat);
  }

  let mut imports: Vec<(String, Imported)> = Vec::new();

  for import in flat {
    let Some(first) = import.segments.first() else { continue };
    let Some(last) = import.segments.last() else { continue };
    let name = import.alias.unwrap_or(last);

    if !matches!(first.name.as_str(), "pkg" | "self" | "super" | "std") {
      diagnostics.push(
        Diagnostic::error(codes::UNRESOLVED_NAME, first.span, format!("cannot find `{}` to import", first.name))
          .with_help(format!("`use` paths start with `pkg`, `self`, `super` or `std`: `use pkg::{}`", first.name)),
      );

      continue;
    }

    if matches!(name.name.as_str(), "pkg" | "self" | "super") {
      diagnostics.push(
        Diagnostic::error(codes::UNRESOLVED_NAME, name.span, format!("importing `{}` needs a name", name.name))
          .with_help(format!("give it one with `as`: `use {} as name;`", name.name)),
      );

      continue;
    }

    let segments: Vec<Ident> = import.segments.iter().map(|segment| (*segment).clone()).collect();
    let mut resolver = Resolver::new(package, resolution, node, None, diagnostics);
    let resolved = resolver.qualified(&segments);

    let item = match resolved {
      Some((Named::Item(item), _, [])) => item,
      Some((Named::Item(ItemRef::Type(_) | ItemRef::Std(crate::StdItem::CString)) | Named::Prelude(_), _, _)) => {
        resolver.diagnostics.push(Diagnostic::error(
          codes::NOT_IMPLEMENTED,
          last.span,
          "importing the variants or functions of a type is not implemented yet; import the type and write `Type::name`",
        ));

        continue;
      }
      Some((_, function, _)) => {
        resolver.diagnostics.push(Diagnostic::error(codes::UNRESOLVED_NAME, last.span, format!("`{}` is a function, not a module", function.name)));

        continue;
      }
      None => continue,
    };

    imports.push((name.name.clone(), Imported { item, span: name.span }));
  }

  let Some(info) = resolution.modules.get_mut(node.0 as usize) else { return };

  for (name, imported) in imports {
    crate::check_reserved(&name, imported.span, diagnostics);

    let previous = info.items.get(&name).map(|declared| declared.span).or_else(|| info.imports.get(&name).map(|previous| previous.span));

    if let Some(previous) = previous {
      diagnostics.push(
        Diagnostic::error(codes::DUPLICATE_DEFINITION, imported.span, format!("the name `{name}` is defined more than once"))
          .with_label(previous, "previous definition here")
          .with_help(format!("import it under another name: `as other_{name}`")),
      );
    } else {
      info.imports.insert(name, imported);
    }
  }
}

pub(crate) fn report_unused_imports(resolution: &Resolution, used: &HashSet<(ModId, String)>, diagnostics: &mut Diagnostics) {
  for (index, module) in resolution.modules.iter().enumerate() {
    for (name, imported) in &module.imports {
      if !used.contains(&(ModId(index as u32), name.clone())) {
        diagnostics.push(Diagnostic::warning(codes::UNUSED_IMPORT, imported.span, format!("unused import `{name}`")).with_help("remove it"));
      }
    }
  }
}

/// Warns when an item visible to the whole package mentions, in its signature or fields, a type
/// that only its own module can see (as Rust's `private_interfaces` lint).
pub(crate) fn report_private_interfaces(resolution: &Resolution, package: &Package, diagnostics: &mut Diagnostics) {
  let public = |visibility: Visibility| visibility == Visibility::Public;

  for module in &package.modules {
    for item in &module.items {
      let mut checks: Vec<(&str, Vec<&TypeExpr>)> = Vec::new();
      let mut parents: Vec<(&str, &Path)> = Vec::new();

      match item {
        Item::Fn(decl) if public(decl.visibility) => checks.push((&decl.name.name, signature_types(decl))),
        Item::Struct(decl) if public(decl.visibility) => {
          let fields = decl.fields.iter().filter(|field| field.visibility != Visibility::Private).map(|field| &field.ty).collect();

          checks.push((&decl.name.name, fields));

          for function in decl.functions.iter().filter(|function| function.visibility != Visibility::Private) {
            checks.push((&function.name.name, signature_types(function)));
          }
        }
        Item::Class(decl) if public(decl.visibility) => {
          let fields = decl.fields.iter().filter(|field| field.visibility != Visibility::Private).map(|field| &field.ty).collect();

          checks.push((&decl.name.name, fields));

          for supertype in decl.parent.iter().chain(&decl.impls) {
            parents.push((&decl.name.name, supertype));
          }

          for function in decl.functions.iter().filter(|function| function.visibility != Visibility::Private) {
            checks.push((&function.name.name, signature_types(function)));
          }
        }
        Item::Enum(decl) if public(decl.visibility) => {
          let fields = decl.variants.iter().flat_map(|variant| match &variant.fields {
            VariantFields::Unit => Vec::new(),
            VariantFields::Tuple(types) => types.iter().collect(),
            VariantFields::Named(fields) => fields.iter().map(|field| &field.ty).collect(),
          });

          checks.push((&decl.name.name, fields.collect()));

          for function in decl.functions.iter().filter(|function| function.visibility != Visibility::Private) {
            checks.push((&function.name.name, signature_types(function)));
          }
        }
        Item::Interface(decl) if public(decl.visibility) => {
          checks.push((&decl.name.name, decl.fields.iter().map(|field| &field.ty).collect()));

          for parent in &decl.parents {
            parents.push((&decl.name.name, parent));
          }

          for function in &decl.functions {
            checks.push((&function.name.name, signature_types(function)));
          }
        }
        Item::Extern(block) => {
          for function in block.functions.iter().filter(|function| public(function.visibility)) {
            let types = function.params.iter().map(|param| &param.ty).chain(function.return_type.as_ref()).collect();

            checks.push((&function.name.name, types));
          }
        }
        _ => {}
      }

      let mut entries: Vec<(&str, Vec<&Path>)> = parents.into_iter().map(|(owner, parent)| (owner, vec![parent])).collect();

      for (owner, types) in checks {
        let mut paths = Vec::new();

        for ty in types {
          type_paths(ty, &mut paths);
        }

        entries.push((owner, paths));
      }

      for (owner, paths) in entries {
        for path in paths {
          let Some(TypeRes::Item(item)) = resolution.type_paths.get(&path.id) else { continue };
          let (visibility, name, declared) = match item {
            TypeItem::Struct(id) => match resolution.struct_decl(package, *id) {
              Some(decl) => (decl.visibility, &decl.name.name, decl.name.span),
              None => continue,
            },
            TypeItem::Enum(id) => match resolution.enum_decl(package, *id) {
              Some(decl) => (decl.visibility, &decl.name.name, decl.name.span),
              None => continue,
            },
            TypeItem::Class(id) => match resolution.class_decl(package, *id) {
              Some(decl) => (decl.visibility, &decl.name.name, decl.name.span),
              None => continue,
            },
            TypeItem::Interface(id) => match resolution.interface_decl(package, *id) {
              Some(decl) => (decl.visibility, &decl.name.name, decl.name.span),
              None => continue,
            },
          };

          if visibility == Visibility::Module {
            diagnostics.push(
              Diagnostic::warning(
                codes::PRIVATE_INTERFACE,
                path.span,
                format!("the private type `{name}` appears in the public interface of `{owner}`"),
              )
              .with_label(declared, "declared without `pub` here")
              .with_help(format!("mark `{name}` as `pub`, or make `{owner}` private")),
            );
          }
        }
      }
    }
  }
}

fn signature_types(decl: &FnDecl) -> Vec<&TypeExpr> {
  decl.params.iter().map(|param| &param.ty).chain(decl.return_type.as_ref()).collect()
}

/// Every path inside a type expression.
fn type_paths<'a>(ty: &'a TypeExpr, out: &mut Vec<&'a Path>) {
  match &ty.kind {
    TypeExprKind::Unit => {}
    TypeExprKind::Path(path) => out.push(path),
    TypeExprKind::Generic { path, args } => {
      out.push(path);

      for arg in args {
        type_paths(arg, out);
      }
    }
    TypeExprKind::Array { elem, .. } | TypeExprKind::Ptr { pointee: elem, .. } => type_paths(elem, out),
  }
}

/// The logical paths, from the package root, of the modules a file may refer to: every prefix of
/// its `use` paths and of the paths that start with `pkg`, `self`, `super` or an imported name.
/// The driver loads the ones that exist as files, then asks again for each loaded file.
pub fn module_references(module: &Module) -> Vec<Vec<String>> {
  let mut flat = Vec::new();

  for decl in &module.uses {
    flatten(&decl.tree, &[], &mut flat);
  }

  let mut found = BTreeSet::new();
  let mut aliases: Vec<(String, Vec<String>)> = Vec::new();

  for import in &flat {
    let names: Vec<&str> = import.segments.iter().map(|segment| segment.name.as_str()).collect();

    if let Some(absolute) = absolute(&names, &module.path) {
      add_prefixes(&absolute, &mut found);

      if let Some(name) = import.alias.or(import.segments.last().copied()) {
        aliases.push((name.name.clone(), absolute));
      }
    }
  }

  let mut paths = Vec::new();

  for item in &module.items {
    match item {
      Item::Fn(decl) => fn_paths(decl, &mut paths),
      Item::Struct(StructDecl { fields, functions, impls, .. })
      | Item::Class(ClassDecl { fields, functions, impls, .. })
      | Item::Interface(InterfaceDecl { fields, functions, parents: impls, .. }) => {
        if let Item::Class(ClassDecl { parent: Some(parent), .. }) = item {
          paths.push(parent);
        }

        paths.extend(impls);

        for field in fields {
          type_paths(&field.ty, &mut paths);
        }

        for function in functions {
          fn_paths(function, &mut paths);
        }
      }
      Item::Enum(decl) => {
        paths.extend(&decl.impls);

        for variant in &decl.variants {
          match &variant.fields {
            VariantFields::Unit => {}
            VariantFields::Tuple(types) => types.iter().for_each(|ty| type_paths(ty, &mut paths)),
            VariantFields::Named(fields) => fields.iter().for_each(|field| type_paths(&field.ty, &mut paths)),
          }
        }

        for function in &decl.functions {
          fn_paths(function, &mut paths);
        }
      }
      Item::Extern(block) => {
        for function in &block.functions {
          for ty in function.params.iter().map(|param| &param.ty).chain(function.return_type.as_ref()) {
            type_paths(ty, &mut paths);
          }
        }
      }
    }
  }

  for path in paths {
    let names: Vec<&str> = path.segments.iter().map(|segment| segment.name.as_str()).collect();
    let Some((first, rest)) = names.split_first() else { continue };

    if rest.is_empty() {
      continue;
    }

    let absolute = match aliases.iter().find(|(alias, _)| alias == first) {
      Some((_, target)) => Some(target.iter().cloned().chain(rest.iter().map(|name| name.to_string())).collect()),
      None => absolute(&names, &module.path),
    };

    if let Some(absolute) = absolute {
      add_prefixes(&absolute, &mut found);
    }
  }

  found.remove(&module.path);
  found.into_iter().collect()
}

/// The path from the package root named by a path starting with `pkg`, `self` or `super`.
fn absolute(names: &[&str], current: &[String]) -> Option<Vec<String>> {
  let (first, rest) = names.split_first()?;
  let mut base: Vec<String> = match *first {
    "pkg" => Vec::new(),
    "self" | "super" => current.to_vec(),
    _ => return None,
  };
  let mut rest = rest;

  if *first == "super" {
    base.pop()?;

    while let Some((&"super", tail)) = rest.split_first() {
      base.pop()?;
      rest = tail;
    }
  }

  base.extend(rest.iter().map(|name| name.to_string()));

  Some(base)
}

fn add_prefixes(path: &[String], found: &mut BTreeSet<Vec<String>>) {
  let mut prefix = Vec::new();

  for segment in path {
    prefix.push(segment.clone());
    found.insert(prefix.clone());
  }
}

fn fn_paths<'a>(decl: &'a FnDecl, out: &mut Vec<&'a Path>) {
  for ty in signature_types(decl) {
    type_paths(ty, out);
  }

  block_paths(&decl.body, out);
}

fn block_paths<'a>(block: &'a Block, out: &mut Vec<&'a Path>) {
  for stmt in &block.stmts {
    match stmt {
      Stmt::Let(stmt) => {
        if let Some(ty) = &stmt.ty {
          type_paths(ty, out);
        }

        if let Some(init) = &stmt.init {
          expr_paths(init, out);
        }
      }
      Stmt::Assign { target, value, .. } => {
        expr_paths(target, out);
        expr_paths(value, out);
      }
      Stmt::Expr(expr) => expr_paths(expr, out),
    }
  }

  if let Some(tail) = &block.tail {
    expr_paths(tail, out);
  }
}

fn expr_paths<'a>(expr: &'a Expr, out: &mut Vec<&'a Path>) {
  let exprs = |items: &'a [Expr], out: &mut Vec<&'a Path>| items.iter().for_each(|item| expr_paths(item, out));

  match &expr.kind {
    ExprKind::Path(path) => out.push(path),
    ExprKind::Zod => {}
    ExprKind::Call { callee, args } => {
      expr_paths(callee, out);
      exprs(args, out);
    }
    ExprKind::MethodCall { receiver, args, .. } => {
      expr_paths(receiver, out);
      exprs(args, out);
    }
    ExprKind::Field { base, .. } => expr_paths(base, out),
    ExprKind::Index { base, index } => {
      expr_paths(base, out);
      expr_paths(index, out);
    }
    ExprKind::StructLit { path, fields } => {
      out.push(path);

      for field in fields {
        expr_paths(&field.value, out);
      }
    }
    ExprKind::ArrayLit(elems) => exprs(elems, out),
    ExprKind::ArrayRepeat { value, .. } | ExprKind::Try(value) | ExprKind::Unary { operand: value, .. } => expr_paths(value, out),
    ExprKind::New { path, args } => {
      out.push(path);
      exprs(args, out);
    }
    ExprKind::Match { scrutinee, arms } => {
      expr_paths(scrutinee, out);

      for arm in arms {
        pattern_paths(&arm.pattern, out);

        if let Some(guard) = &arm.guard {
          expr_paths(guard, out);
        }

        expr_paths(&arm.body, out);
      }
    }
    ExprKind::Template(parts) => {
      for part in parts {
        if let TemplatePart::Expr(expr) = part {
          expr_paths(expr, out);
        }
      }
    }
    ExprKind::Binary { lhs, rhs, .. } => {
      expr_paths(lhs, out);
      expr_paths(rhs, out);
    }
    ExprKind::Cast { expr, ty } => {
      expr_paths(expr, out);
      type_paths(ty, out);
    }
    ExprKind::Range { start, end, .. } => {
      expr_paths(start, out);
      expr_paths(end, out);
    }
    ExprKind::Block(block) | ExprKind::Loop { body: block } | ExprKind::Unsafe(block) => block_paths(block, out),
    ExprKind::If { cond, then_block, else_branch } => {
      expr_paths(cond, out);
      block_paths(then_block, out);

      if let Some(branch) = else_branch {
        expr_paths(branch, out);
      }
    }
    ExprKind::While { cond, body } => {
      expr_paths(cond, out);
      block_paths(body, out);
    }
    ExprKind::For { iter, body, .. } => {
      expr_paths(iter, out);
      block_paths(body, out);
    }
    ExprKind::Break { value } | ExprKind::Return { value } => {
      if let Some(value) = value {
        expr_paths(value, out);
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

fn pattern_paths<'a>(pattern: &'a Pattern, out: &mut Vec<&'a Path>) {
  match &pattern.kind {
    PatternKind::Wildcard | PatternKind::Ident(_) => {}
    PatternKind::Literal(expr) => expr_paths(expr, out),
    PatternKind::Path(path) => out.push(path),
    PatternKind::Tuple { path, elems } => {
      out.push(path);
      elems.iter().for_each(|elem| pattern_paths(elem, out));
    }
    PatternKind::Named { path, fields, .. } => {
      out.push(path);
      fields.iter().for_each(|field| pattern_paths(&field.pattern, out));
    }
    PatternKind::Or(alternatives) => alternatives.iter().for_each(|alternative| pattern_paths(alternative, out)),
  }
}
