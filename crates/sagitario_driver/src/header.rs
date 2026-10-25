//! The C header of a library (docs/nova.md#library-outputs): the prototypes of its exported
//! functions and the `@repr(C)` structs they use, with the `///` comments of the source.

use sagitario_ast::{FnDecl, Package, TypeExpr, TypeExprKind};
use sagitario_resolve::{CType, Resolution, StructId, TypeRes};
use sagitario_span::SourceMap;
use sagitario_types::{FloatTy, IntTy, Type, TypedModule};

/// A function C can call: its symbol, and its declaration.
pub struct Export<'p> {
  pub symbol: String,
  pub decl: &'p FnDecl,
  pub params: Vec<Type>,
  pub ret: Type,
}

/// The exported functions of the program (`extern "C" fn`), in declaration order.
pub fn exports<'p>(package: &'p Package, resolution: &Resolution, typed: &TypedModule) -> Vec<Export<'p>> {
  let mut exports = Vec::new();

  for (id, decl) in resolution.fn_decls(package) {
    if !decl.extern_c {
      continue;
    }

    let Some(sig) = typed.sig(id) else { continue };
    let symbol = decl.export.as_ref().map_or_else(|| decl.name.name.clone(), |export| export.symbol.clone());

    exports.push(Export { symbol, decl, params: sig.params.clone(), ret: sig.ret.clone() });
  }

  exports
}

/// Words that cannot name a parameter or a field in C.
const C_KEYWORDS: &[&str] = &[
  "auto", "break", "case", "char", "const", "continue", "default", "do", "double", "else", "enum", "extern", "float", "for", "goto", "if", "inline",
  "int", "long", "register", "restrict", "return", "short", "signed", "sizeof", "static", "struct", "switch", "typedef", "union", "unsigned", "void",
  "volatile", "while", "bool", "true", "false", "NULL",
];

/// A Sagitario name usable in C: keywords get a trailing `_`.
fn c_name(name: &str) -> String {
  if C_KEYWORDS.contains(&name) { format!("{name}_") } else { name.to_string() }
}

struct Header<'a> {
  package: &'a Package,
  resolution: &'a Resolution,
  typed: &'a TypedModule,
  /// The C name of each struct mentioned, unique in the header.
  names: Vec<(StructId, String)>,
  /// `@repr(C)` structs in the order they must be defined: a struct after those it holds.
  defined: Vec<StructId>,
}

impl Header<'_> {
  fn struct_name(&mut self, id: StructId) -> String {
    if let Some((_, name)) = self.names.iter().find(|(known, _)| *known == id) {
      return name.clone();
    }

    let base = self.resolution.struct_decl(self.package, id).map_or_else(|| format!("sag_struct_{}", id.0), |decl| c_name(&decl.name.name));
    let mut name = base.clone();
    let mut suffix = 2;

    while self.names.iter().any(|(_, taken)| *taken == name) {
      name = format!("{base}_{suffix}");
      suffix += 1;
    }

    self.names.push((id, name.clone()));

    name
  }

  /// Visits the structs `ty` needs, defining the `@repr(C)` ones held by value or pointed to.
  fn visit(&mut self, ty: &Type) {
    match ty {
      Type::Ptr { pointee, .. } => self.visit(pointee),
      Type::Struct(inner) => {
        let id = inner.id;

        self.struct_name(id);

        let repr_c = self.resolution.struct_decl(self.package, id).is_some_and(|decl| decl.repr_c);

        if !repr_c || self.defined.contains(&id) {
          return;
        }

        let fields: Vec<Type> = self.typed.struct_def(id).map(|def| def.fields.iter().map(|field| field.ty.clone()).collect()).unwrap_or_default();

        for field in &fields {
          self.visit(field);
        }

        if !self.defined.contains(&id) {
          self.defined.push(id);
        }
      }
      _ => {}
    }
  }

  /// The C spelling of `ty`, written `written` in the source, followed by `name` (a declarator)
  /// when given. `c_char` is spelled `char`, which C does not take as `int8_t` or `uint8_t`.
  fn declare(&mut self, ty: &Type, written: Option<&TypeExpr>, name: &str) -> String {
    let c_char = written.is_some_and(|written| match &written.kind {
      TypeExprKind::Path(path) => self.resolution.type_paths.get(&path.id) == Some(&TypeRes::CType(CType::Char)),
      _ => false,
    });
    let base = match ty {
      Type::Int(_) if c_char => "char".to_string(),
      Type::Int(int) => match int {
        IntTy::I8 => "int8_t",
        IntTy::I16 => "int16_t",
        IntTy::I32 => "int32_t",
        IntTy::I64 => "int64_t",
        IntTy::Isize => "intptr_t",
        IntTy::U8 => "uint8_t",
        IntTy::U16 => "uint16_t",
        IntTy::U32 => "uint32_t",
        IntTy::U64 => "uint64_t",
        IntTy::Usize => "size_t",
      }
      .to_string(),
      Type::Float(FloatTy::F32) => "float".to_string(),
      Type::Float(FloatTy::F64) => "double".to_string(),
      Type::Bool => "bool".to_string(),
      Type::Struct(inner) => self.struct_name(inner.id),
      Type::Ptr { mutable, pointee } => {
        let written = written.and_then(|written| match &written.kind {
          TypeExprKind::Ptr { pointee, .. } => Some(&**pointee),
          _ => None,
        });
        let inner = if **pointee == Type::CVoid { "void".to_string() } else { self.declare(pointee, written, "") };
        let pointer = if *mutable { format!("{inner} *") } else { format!("const {inner} *") };

        return if name.is_empty() { pointer } else { format!("{pointer}{name}") };
      }
      _ => "void".to_string(),
    };

    if name.is_empty() { base } else { format!("{base} {name}") }
  }
}

/// The `///` comments right above the declaration starting at byte `start` of `text`, skipping
/// attributes such as `@export`.
fn doc_comment(text: &str, start: usize) -> Vec<String> {
  let before = text.get(..start).unwrap_or_default();
  let mut lines: Vec<&str> = before.lines().collect();
  let mut docs = Vec::new();

  // The declaration's own line, up to its start.
  if !before.ends_with('\n') {
    lines.pop();
  }

  while let Some(line) = lines.pop() {
    let line = line.trim();

    if line.starts_with('@') {
      continue;
    }

    let Some(doc) = line.strip_prefix("///") else { break };

    docs.push(doc.strip_prefix(' ').unwrap_or(doc).replace("*/", "* /"));
  }

  docs.reverse();
  docs
}

/// The header of the library `name`.
pub fn generate(name: &str, package: &Package, resolution: &Resolution, typed: &TypedModule, sources: &SourceMap) -> String {
  let exports = exports(package, resolution, typed);
  let mut header = Header { package, resolution, typed, names: Vec::new(), defined: Vec::new() };

  for export in &exports {
    for ty in export.params.iter().chain([&export.ret]) {
      header.visit(ty);
    }
  }

  let guard: String = name.to_ascii_uppercase().chars().map(|c| if c.is_ascii_alphanumeric() { c } else { '_' }).collect();
  let mut out = format!("/* Generated by nova for the library `{name}`. Do not edit. */\n#ifndef {guard}_H\n#define {guard}_H\n\n");

  out.push_str("#include <stdbool.h>\n#include <stddef.h>\n#include <stdint.h>\n\n");
  out.push_str("#ifdef __cplusplus\nextern \"C\" {\n#endif\n");

  let names = header.names.clone();

  if !names.is_empty() {
    out.push('\n');
  }

  for (_, struct_name) in &names {
    out.push_str(&format!("typedef struct {struct_name} {struct_name};\n"));
  }

  for id in header.defined.clone() {
    let struct_name = header.struct_name(id);
    let fields: Vec<(String, Type)> =
      typed.struct_def(id).map(|def| def.fields.iter().map(|field| (c_name(&field.name), field.ty.clone())).collect()).unwrap_or_default();

    let written = resolution.struct_decl(package, id).map(|decl| &decl.fields);

    out.push_str(&format!("\nstruct {struct_name} {{\n"));

    for (index, (field, ty)) in fields.into_iter().enumerate() {
      let field_type = written.and_then(|fields| fields.get(index)).map(|decl| &decl.ty);

      out.push_str(&format!("  {};\n", header.declare(&ty, field_type, &field)));
    }

    out.push_str("};\n");
  }

  for export in &exports {
    let text = sources.get(export.decl.span.file).map(|source| source.text()).unwrap_or_default();
    let docs = doc_comment(text, export.decl.span.start as usize);
    let params: Vec<String> =
      export.params.iter().zip(&export.decl.params).map(|(ty, param)| header.declare(ty, Some(&param.ty), &c_name(&param.name.name))).collect();
    let params = if params.is_empty() { "void".to_string() } else { params.join(", ") };
    let ret = if matches!(export.ret, Type::Unit | Type::Never) {
      "void".to_string()
    } else {
      header.declare(&export.ret, export.decl.return_type.as_ref(), "")
    };

    out.push('\n');

    if !docs.is_empty() {
      out.push_str(&format!("/* {} */\n", docs.join("\n * ")));
    }

    let separator = if ret.ends_with('*') { "" } else { " " };

    out.push_str(&format!("{ret}{separator}{}({params});\n", export.symbol));
  }

  out.push_str(&format!("\n#ifdef __cplusplus\n}}\n#endif\n\n#endif /* {guard}_H */\n"));
  out
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn finds_doc_comments_above_attributes() {
    let text = "fn a() {}\n\n/// Adds two\n/// numbers.\n@export(\"x\")\nextern \"C\" fn add() {}\n";
    let start = text.find("extern").unwrap();

    assert_eq!(doc_comment(text, start), vec!["Adds two".to_string(), "numbers.".to_string()]);
    assert!(doc_comment(text, 0).is_empty());
  }

  #[test]
  fn renames_c_keywords() {
    assert_eq!(c_name("int"), "int_");
    assert_eq!(c_name("width"), "width");
  }
}
