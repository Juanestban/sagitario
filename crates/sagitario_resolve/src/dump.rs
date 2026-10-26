//! The resolution as text, for the differential tests of the self-hosted compiler (`sag check
//! --dump=resolve`, docs/adr/0052-resolver-port-and-resolution-dumps.md). Sections in a fixed
//! order, one entry per line; names in a module and the maps by node id are sorted.

use sagitario_span::Span;

use crate::{Builtin, EnumKind, ItemRef, LocalKind, ParamOwner, Res, Resolution, StdItem, TypeInfo, TypeItem, TypeRes};

pub fn dump(resolution: &Resolution) -> String {
  let mut out = String::new();

  for (index, module) in resolution.modules.iter().enumerate() {
    let path = if module.path.is_empty() { "-".to_string() } else { module.path.join("::") };
    let root = if module.root.is_empty() { "-" } else { module.root.as_str() };

    out.push_str(&format!(
      "module {index} path={path} std={} package={} root={root} source={} parent={}\n",
      module.in_std,
      module.package,
      optional(module.source),
      optional(module.parent.map(|parent| parent.0)),
    ));

    let mut children: Vec<_> = module.children.iter().collect();

    children.sort_by(|a, b| a.0.cmp(b.0));

    for (name, child) in children {
      out.push_str(&format!("  child {name} -> {}\n", child.0));
    }

    let mut items: Vec<_> = module.items.iter().collect();

    items.sort_by(|a, b| a.0.cmp(b.0));

    for (name, declared) in items {
      let visibility = match declared.visibility {
        sagitario_ast::Visibility::Module => "module",
        sagitario_ast::Visibility::Public => "pub",
        sagitario_ast::Visibility::Private => "prv",
      };

      out.push_str(&format!("  item {name} -> {} {visibility} {}\n", item_ref(declared.item), span(declared.span)));
    }

    let mut imports: Vec<_> = module.imports.iter().collect();

    imports.sort_by(|a, b| a.0.cmp(b.0));

    for (name, imported) in imports {
      out.push_str(&format!("  import {name} -> {} {}\n", item_ref(imported.item), span(imported.span)));
    }
  }

  for (index, info) in resolution.functions.iter().enumerate() {
    out.push_str(&format!(
      "fn {index} module={} item={} member={} owner={}\n",
      info.module,
      info.item,
      optional(info.member),
      info.owner.map_or("-".to_string(), type_item)
    ));
  }

  for (kind, types) in
    [("struct", &resolution.structs), ("enum", &resolution.enums), ("class", &resolution.classes), ("interface", &resolution.interfaces)]
  {
    for (index, info) in types.iter().enumerate() {
      out.push_str(&type_info(kind, index, info));
    }
  }

  for (index, info) in resolution.consts.iter().enumerate() {
    out.push_str(&format!("const {index} module={} item={}\n", info.module, info.item));
  }

  for (index, info) in resolution.externs.iter().enumerate() {
    out.push_str(&format!("extern {index} module={} item={} member={}\n", info.module, info.item, info.member));
  }

  out.push_str(&format!("prelude {}\n", optional(resolution.prelude.map(|id| id.0))));
  out.push_str(&format!("std_root {}\n", optional(resolution.std_root.map(|id| id.0))));
  out.push_str(&format!("package_roots {}\n", list(resolution.package_roots.iter().map(|id| id.0))));
  out.push_str(&format!("error_interface {}\n", optional(resolution.error_interface.map(|id| id.0))));
  out.push_str(&format!("main {}\n", optional(resolution.main.map(|id| id.0))));

  for (function, locals) in resolution.locals.iter().enumerate() {
    out.push_str(&format!("locals {function}\n"));

    for (index, local) in locals.iter().enumerate() {
      let kind = match local.kind {
        LocalKind::Param => "param",
        LocalKind::SelfParam => "self",
        LocalKind::Const => "const",
        LocalKind::Var => "var",
        LocalKind::ForVar => "for",
        LocalKind::Pattern => "pattern",
      };

      out.push_str(&format!("  local {index} {} {kind} {}\n", local.name, span(local.span)));
    }
  }

  let mut paths: Vec<_> = resolution.paths.iter().collect();

  paths.sort_by_key(|(id, _)| **id);

  for (id, res) in paths {
    out.push_str(&format!("path #{} -> {}\n", id.0, resolved(*res)));
  }

  let mut type_paths: Vec<_> = resolution.type_paths.iter().collect();

  type_paths.sort_by_key(|(id, _)| **id);

  for (id, res) in type_paths {
    out.push_str(&format!("type_path #{} -> {}\n", id.0, type_res(*res)));
  }

  let mut bindings: Vec<_> = resolution.bindings.iter().collect();

  bindings.sort_by_key(|(id, _)| **id);

  for (id, local) in bindings {
    out.push_str(&format!("binding #{} -> local {}\n", id.0, local.0));
  }

  let mut lambdas: Vec<_> = resolution.lambdas.iter().collect();

  lambdas.sort_by_key(|(id, _)| **id);

  for (id, info) in lambdas {
    out.push_str(&format!(
      "lambda #{} params={} first={} end={} captures={}\n",
      id.0,
      list(info.params.iter().map(|local| local.0)),
      info.first_local,
      info.end_local,
      list(info.captures.iter().map(|local| local.0))
    ));
  }

  out
}

fn optional<T: ToString>(value: Option<T>) -> String {
  value.map_or("-".to_string(), |value| value.to_string())
}

fn list(values: impl Iterator<Item = u32>) -> String {
  let values: Vec<String> = values.map(|value| value.to_string()).collect();

  if values.is_empty() { "-".to_string() } else { values.join(",") }
}

fn span(span: Span) -> String {
  format!("{}:{}..{}", span.file.index(), span.start, span.end)
}

fn type_info(kind: &str, index: usize, info: &TypeInfo) -> String {
  format!("{kind} {index} module={} item={} functions={}\n", info.module, info.item, list(info.functions.iter().map(|id| id.0)))
}

fn type_item(item: TypeItem) -> String {
  match item {
    TypeItem::Struct(id) => format!("struct {}", id.0),
    TypeItem::Enum(id) => format!("enum {}", id.0),
    TypeItem::Class(id) => format!("class {}", id.0),
    TypeItem::Interface(id) => format!("interface {}", id.0),
  }
}

fn item_ref(item: ItemRef) -> String {
  match item {
    ItemRef::Fn(id) => format!("fn {}", id.0),
    ItemRef::Extern(id) => format!("extern {}", id.0),
    ItemRef::Const(id) => format!("const {}", id.0),
    ItemRef::Type(item) => type_item(item),
    ItemRef::Module(id) => format!("module {}", id.0),
    ItemRef::Std(std) => match std {
      StdItem::Root => "std root".to_string(),
      StdItem::Ffi => "std ffi".to_string(),
      StdItem::CType(ty) => format!("std {}", ty.name()),
      StdItem::CString => "std CString".to_string(),
      StdItem::StringFromC => "std string_from_c".to_string(),
      StdItem::Exit => "std exit".to_string(),
    },
  }
}

fn builtin(builtin: Builtin) -> &'static str {
  builtin.name()
}

fn resolved(res: Res) -> String {
  match res {
    Res::Fn(id) => format!("fn {}", id.0),
    Res::Extern(id) => format!("extern {}", id.0),
    Res::Builtin(value) => format!("builtin {}", builtin(value)),
    Res::Local(id) => format!("local {}", id.0),
    Res::Struct(id) => format!("struct {}", id.0),
    Res::Variant(variant) => {
      let owner = match variant.owner {
        EnumKind::User(id) => format!("enum {}", id.0),
        EnumKind::Option => "Option".to_string(),
        EnumKind::Result => "Result".to_string(),
      };

      format!("variant {owner} {}", variant.index)
    }
    Res::Const(id) => format!("const {}", id.0),
  }
}

fn type_res(res: TypeRes) -> String {
  match res {
    TypeRes::Item(item) => type_item(item),
    TypeRes::Param(owner, index) => match owner {
      ParamOwner::Item(item) => format!("param {} {index}", type_item(item)),
      ParamOwner::Fn(id) => format!("param fn {} {index}", id.0),
    },
    TypeRes::Builtin => "builtin".to_string(),
    TypeRes::CType(ty) => format!("ctype {}", ty.name()),
    TypeRes::CString => "CString".to_string(),
    TypeRes::Error => "error".to_string(),
  }
}
