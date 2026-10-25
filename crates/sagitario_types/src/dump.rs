//! The typed module as text, for the differential tests of the self-hosted compiler (`sag check
//! --dump=declarations` and `--dump=types`, docs/adr/0053-type-checker-port-and-type-dumps.md).
//! Sections in a fixed order, one entry per line; maps are sorted by key. Types are written as in
//! messages, with the identity of program types and parameters: `Point#struct2<i32>`,
//! `T#fn3.0`.

use sagitario_ast::dump::quote;
use sagitario_resolve::{ClassId, EnumId, FnId, InterfaceId, ParamOwner, StructId, TypeItem};
use sagitario_span::Span;

use crate::{BuiltinMethod, ConstValue, FieldDef, FnSig, InterfaceRef, ListMethod, Receiver, SELF_PARAM, Type, TypedModule};

pub fn dump(typed: &TypedModule) -> String {
  let mut out = String::new();

  for (index, sig) in typed.sigs.iter().enumerate() {
    out.push_str(&format!("sig {index} {}\n", signature(sig)));
  }

  for (index, sig) in typed.extern_sigs.iter().enumerate() {
    out.push_str(&format!("extern_sig {index} {}\n", signature(sig)));
  }

  for (index, def) in typed.structs.iter().enumerate() {
    out.push_str(&format!("struct {index} {}\n", def.name));
    fields(&mut out, &def.fields);
  }

  for (index, def) in typed.enums.iter().enumerate() {
    out.push_str(&format!("enum {index} {}\n", def.name));

    for variant in &def.variants {
      let parts: Vec<String> = variant
        .fields
        .iter()
        .map(|(name, field)| match name {
          Some(name) => format!("{name}: {}", ty(field)),
          None => ty(field),
        })
        .collect();
      let kind = if variant.named { "named" } else { "tuple" };

      out.push_str(&format!("  variant {} {kind} [{}]\n", variant.name, parts.join(", ")));
    }
  }

  for (index, def) in typed.classes.iter().enumerate() {
    let parent = def.parent.as_ref().map_or("-".to_string(), |parent| ty(&Type::Class(parent.clone())));

    out.push_str(&format!(
      "class {index} {} parent={parent} abstract={} initializer={} vtable={}\n",
      def.name,
      def.is_abstract,
      optional_fn(def.initializer),
      fn_list(&def.vtable)
    ));
    fields(&mut out, &def.fields);
  }

  for (index, def) in typed.interfaces.iter().enumerate() {
    out.push_str(&format!(
      "interface {index} {} parents={} methods={} uses_self={}\n",
      def.name,
      interfaces(&def.parents),
      fn_list(&def.methods),
      def.uses_self
    ));
    fields(&mut out, &def.fields);
  }

  let mut impls: Vec<_> = typed.impls.iter().collect();

  impls.sort_by_key(|(item, _)| item_key(**item));

  for (item, list) in impls {
    out.push_str(&format!("impl {} -> {}\n", type_item(*item), interfaces(list)));
  }

  let mut generics: Vec<_> = typed.generics.iter().collect();

  generics.sort_by_key(|(owner, _)| owner_key(**owner));

  for (owner, params) in generics {
    let params: Vec<String> = params.iter().map(|param| format!("{}: {}", param.name, interfaces(&param.bounds))).collect();

    out.push_str(&format!("generics {} -> {}\n", param_owner(*owner), params.join("; ")));
  }

  let error_types = typed.error_types.map_or("-".to_string(), |(interface, class)| format!("interface {} class {}", interface.0, class.0));

  out.push_str(&format!("error_types {error_types}\n"));

  let prelude = &typed.prelude;

  out.push_str(&format!(
    "prelude ordering={} eq={} ord={} hash={} entry={} iter={} map={} send={}\n",
    optional(prelude.ordering.map(|id: EnumId| id.0)),
    optional(prelude.eq.map(|id: InterfaceId| id.0)),
    optional(prelude.ord.map(|id| id.0)),
    optional(prelude.hash.map(|id| id.0)),
    optional(prelude.entry.map(|id: StructId| id.0)),
    optional(prelude.iter.map(|id: ClassId| id.0)),
    optional(prelude.map.map(|id| id.0)),
    optional(prelude.send.map(|id| id.0)),
  ));

  let mut slots: Vec<_> = typed.slots.iter().collect();

  slots.sort_by_key(|(id, _)| **id);

  for (id, slot) in slots {
    out.push_str(&format!("slot fn {} -> {slot}\n", id.0));
  }

  let sync: Vec<String> = typed.sync_classes.iter().map(|id| id.0.to_string()).collect();

  out.push_str(&format!("sync_classes {}\n", if sync.is_empty() { "-".to_string() } else { sync.join(",") }));

  for (index, const_ty) in typed.const_types.iter().enumerate() {
    let value = match typed.const_values.get(index).cloned().flatten() {
      Some(ConstValue::Bool(value)) => format!("bool {value}"),
      Some(ConstValue::Char(value)) => format!("char {}", quote(&value.to_string())),
      Some(ConstValue::Int(value)) => format!("int {value}"),
      Some(ConstValue::Float(value)) => format!("float {value}"),
      Some(ConstValue::Str(value)) => format!("str {}", quote(&value)),
      None => "-".to_string(),
    };

    out.push_str(&format!("const {index} {} = {value}\n", ty(const_ty)));
  }

  for (function, locals) in typed.local_types.iter().enumerate() {
    out.push_str(&format!("locals {function}\n"));

    for (index, local) in locals.iter().enumerate() {
      out.push_str(&format!("  local {index} {}\n", ty(local)));
    }
  }

  let mut exprs: Vec<_> = typed.expr_types.iter().collect();

  exprs.sort_by_key(|(id, _)| **id);

  for (id, expr_ty) in exprs {
    out.push_str(&format!("expr #{} -> {}\n", id.0, ty(expr_ty)));
  }

  let mut call_args: Vec<_> = typed.call_args.iter().collect();

  call_args.sort_by_key(|(id, _)| **id);

  for (id, args) in call_args {
    out.push_str(&format!("call_args #{} -> {}\n", id.0, types(args)));
  }

  let mut methods: Vec<_> = typed.builtin_methods.iter().collect();

  methods.sort_by_key(|(id, _)| **id);

  for (id, method) in methods {
    out.push_str(&format!("builtin_method #{} -> {}\n", id.0, builtin_method(method)));
  }

  let mut calls: Vec<_> = typed.method_calls.iter().collect();

  calls.sort_by_key(|(id, _)| **id);

  for (id, function) in calls {
    out.push_str(&format!("method_call #{} -> fn {}\n", id.0, function.0));
  }

  let mut zod_calls: Vec<_> = typed.zod_calls.iter().collect();

  zod_calls.sort();

  for id in zod_calls {
    out.push_str(&format!("zod_call #{}\n", id.0));
  }

  let mut field_calls: Vec<_> = typed.field_calls.iter().collect();

  field_calls.sort();

  for id in field_calls {
    out.push_str(&format!("field_call #{}\n", id.0));
  }

  let mut to_iter: Vec<_> = typed.to_iter.iter().collect();

  to_iter.sort_by_key(|(id, _)| **id);

  for (id, (function, args)) in to_iter {
    out.push_str(&format!("to_iter #{} -> fn {} {}\n", id.0, function.0, types(args)));
  }

  let mut ranges: Vec<_> = typed.ranges.iter().collect();

  ranges.sort_by_key(|(id, _)| **id);

  for (id, function) in ranges {
    out.push_str(&format!("range #{} -> fn {}\n", id.0, function.0));
  }

  out
}

fn optional(value: Option<u32>) -> String {
  value.map_or("-".to_string(), |value| value.to_string())
}

fn optional_fn(value: Option<FnId>) -> String {
  value.map_or("-".to_string(), |id| id.0.to_string())
}

fn fn_list(functions: &[FnId]) -> String {
  let ids: Vec<String> = functions.iter().map(|id| id.0.to_string()).collect();

  if ids.is_empty() { "-".to_string() } else { ids.join(",") }
}

fn span(span: Span) -> String {
  format!("{}:{}..{}", span.file.index(), span.start, span.end)
}

fn fields(out: &mut String, fields: &[FieldDef]) {
  for field in fields {
    let visibility = if field.public { "pub" } else { "prv" };
    let mutability = if field.mutable { "var" } else { "const" };
    let weak = if field.weak { " weak" } else { "" };

    out.push_str(&format!("  field {}: {} {visibility} {mutability}{weak} {}\n", field.name, ty(&field.ty), span(field.span)));
  }
}

fn signature(sig: &FnSig) -> String {
  let receiver = match sig.receiver {
    None => "-",
    Some(Receiver::Value) => "value",
    Some(Receiver::Var) => "var",
    Some(Receiver::Init) => "init",
  };

  format!("{receiver} {} -> {}", types(&sig.params), ty(&sig.ret))
}

/// `[A, B]`
fn types(list: &[Type]) -> String {
  let parts: Vec<String> = list.iter().map(ty).collect();

  format!("[{}]", parts.join(", "))
}

fn interfaces(list: &[InterfaceRef]) -> String {
  let parts: Vec<String> = list.iter().map(|interface| ty(&Type::Interface(interface.clone()))).collect();

  format!("[{}]", parts.join(", "))
}

/// The order of type items in the dump: by kind, then by id.
fn item_key(item: TypeItem) -> (u32, u32) {
  match item {
    TypeItem::Struct(id) => (0, id.0),
    TypeItem::Enum(id) => (1, id.0),
    TypeItem::Class(id) => (2, id.0),
    TypeItem::Interface(id) => (3, id.0),
  }
}

/// The order of generic declarations in the dump: types first, then functions.
fn owner_key(owner: ParamOwner) -> (u32, u32) {
  match owner {
    ParamOwner::Item(item) => item_key(item),
    ParamOwner::Fn(id) => (4, id.0),
  }
}

fn type_item(item: TypeItem) -> String {
  match item {
    TypeItem::Struct(id) => format!("struct{}", id.0),
    TypeItem::Enum(id) => format!("enum{}", id.0),
    TypeItem::Class(id) => format!("class{}", id.0),
    TypeItem::Interface(id) => format!("interface{}", id.0),
  }
}

fn param_owner(owner: ParamOwner) -> String {
  match owner {
    ParamOwner::Item(item) => type_item(item),
    ParamOwner::Fn(id) => format!("fn{}", id.0),
  }
}

/// `Name#kindN` or `Name#kindN<A, B>`.
fn named(name: &str, item: TypeItem, args: &[Type]) -> String {
  let parts: Vec<String> = args.iter().map(ty).collect();
  let args = if parts.is_empty() { String::new() } else { format!("<{}>", parts.join(", ")) };

  format!("{name}#{}{args}", type_item(item))
}

pub fn ty(ty_: &Type) -> String {
  match ty_ {
    Type::Struct(inner) => named(&inner.name, TypeItem::Struct(inner.id), &inner.args),
    Type::Enum(inner) => named(&inner.name, TypeItem::Enum(inner.id), &inner.args),
    Type::Class(inner) => named(&inner.name, TypeItem::Class(inner.id), &inner.args),
    Type::Interface(inner) => named(&inner.name, TypeItem::Interface(inner.id), &inner.args),
    Type::Param(param) => {
      let index = if param.index == SELF_PARAM { "self".to_string() } else { param.index.to_string() };

      format!("{}#{}.{index}", param.name, param_owner(param.owner))
    }
    Type::Function { params, ret } => {
      let params: Vec<String> = params.iter().map(ty).collect();
      let ret = if **ret == Type::Unit { String::new() } else { format!(" -> {}", ty(ret)) };

      format!("fn({}){ret}", params.join(", "))
    }
    Type::Array(elem, len) => format!("[{}; {len}]", ty(elem)),
    Type::List(elem) => format!("List<{}>", ty(elem)),
    Type::Option(value) => format!("Option<{}>", ty(value)),
    Type::Result(value, error) => format!("Result<{}, {}>", ty(value), ty(error)),
    Type::Ptr { mutable, pointee } => format!("*{} {}", if *mutable { "mut" } else { "const" }, ty(pointee)),
    other => other.to_string(),
  }
}

fn list_method(method: ListMethod) -> &'static str {
  match method {
    ListMethod::Len => "len",
    ListMethod::IsEmpty => "is_empty",
    ListMethod::Push => "push",
    ListMethod::Pop => "pop",
    ListMethod::Get => "get",
    ListMethod::First => "first",
    ListMethod::Last => "last",
    ListMethod::Insert => "insert",
    ListMethod::Remove => "remove",
    ListMethod::Clear => "clear",
    ListMethod::Reverse => "reverse",
  }
}

fn builtin_method(method: &BuiltinMethod) -> String {
  match method {
    BuiltinMethod::IsSome => "is_some".to_string(),
    BuiltinMethod::IsNone => "is_none".to_string(),
    BuiltinMethod::IsOk => "is_ok".to_string(),
    BuiltinMethod::IsErr => "is_err".to_string(),
    BuiltinMethod::Unwrap => "unwrap".to_string(),
    BuiltinMethod::Expect => "expect".to_string(),
    BuiltinMethod::UnwrapOr => "unwrap_or".to_string(),
    BuiltinMethod::UnwrapErr => "unwrap_err".to_string(),
    BuiltinMethod::Runtime { function, located } => format!("runtime {function}{}", if *located { " located" } else { "" }),
    BuiltinMethod::Find => "find".to_string(),
    BuiltinMethod::ToDigit => "to_digit".to_string(),
    BuiltinMethod::Parse(target) => format!("parse {}", ty(target)),
    BuiltinMethod::Chars => "chars".to_string(),
    BuiltinMethod::Bytes => "bytes".to_string(),
    BuiltinMethod::Map => "map".to_string(),
    BuiltinMethod::MapErr => "map_err".to_string(),
    BuiltinMethod::AndThen => "and_then".to_string(),
    BuiltinMethod::UnwrapOrElse => "unwrap_or_else".to_string(),
    BuiltinMethod::Filter => "filter".to_string(),
    BuiltinMethod::List(method) => format!("list {}", list_method(*method)),
    BuiltinMethod::Prelude(function) => format!("prelude fn {}", function.0),
    BuiltinMethod::TotalCompare => "total_compare".to_string(),
    BuiltinMethod::RangeRev => "range_rev".to_string(),
  }
}
