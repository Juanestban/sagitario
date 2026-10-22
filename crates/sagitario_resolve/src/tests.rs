//! Unit tests of name resolution.

use super::*;
use sagitario_span::SourceMap;
use std::path::PathBuf;

fn resolve_text(text: &str) -> (Package, Resolution, Vec<&'static str>) {
  let mut sources = SourceMap::new();
  let file = sources.add(PathBuf::from("test.sag"), text.to_string());
  let mut diagnostics = Diagnostics::new();
  let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
  let package = Package::single(sagitario_parser::parse_module(file, tokens, &mut diagnostics));

  assert!(!diagnostics.has_errors(), "parse failed: {:?}", diagnostics.take());

  let resolution = resolve(&package, &mut diagnostics);
  let errors = diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect();

  (package, resolution, errors)
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
  let (package, resolution, errors) = resolve_text(program);

  assert!(errors.is_empty(), "{errors:?}");
  assert_eq!(resolution.functions.len(), 5);
  assert_eq!(resolution.type_function(&package, TypeItem::Struct(StructId(0)), "get"), Some(FnId(1)));
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

/// Resolves a package given as `(module path, source)` pairs; the first one is the root.
fn resolve_package(files: &[(&str, &str)]) -> Vec<&'static str> {
  let mut sources = SourceMap::new();
  let mut diagnostics = Diagnostics::new();
  let mut next_id = 0;
  let mut modules = Vec::new();

  for (path, text) in files {
    let file = sources.add(PathBuf::from(format!("{path}.sag")), text.to_string());
    let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
    let mut module = sagitario_parser::parse_module_from(file, tokens, &mut next_id, &mut diagnostics);

    module.path = path.split("::").filter(|segment| !segment.is_empty()).map(str::to_string).collect();
    modules.push(module);
  }

  assert!(!diagnostics.has_errors(), "parse failed: {:?}", diagnostics.take());

  resolve(&Package { modules }, &mut diagnostics);

  diagnostics.take().iter().map(|diagnostic| diagnostic.code.0).collect()
}

#[test]
fn private_items_are_visible_in_submodules_only() {
  let root = "use pkg::a::b; fn secret() {} fn main() { b::f(); pkg::a::open(); }";

  // `a::b` sees the private items of its ancestors, `pkg` and `a`.
  assert_eq!(
    resolve_package(&[("", root), ("a", "fn hidden() {} pub fn open() {}"), ("a::b", "pub fn f() { super::super::secret(); super::hidden(); }")]),
    Vec::<&str>::new()
  );
  // The root does not see the private items of `a`.
  assert_eq!(resolve_package(&[("", "fn main() { pkg::a::hidden(); }"), ("a", "fn hidden() {}")]), vec!["E0207"]);
  assert_eq!(resolve_package(&[("", "use pkg::a::P; fn main() { const _p = P {}; }"), ("a", "pub(pkg) struct P {}")]), Vec::<&str>::new());
}

#[test]
fn reports_import_errors_and_warnings() {
  assert_eq!(resolve_package(&[("", "use pkg::a::f; fn main() {}"), ("a", "pub fn f() {}")]), vec!["W0005"]);
  assert_eq!(resolve_package(&[("", "use pkg::a::f; fn f() {} fn main() {}"), ("a", "pub fn f() {}")]), vec!["E0201"]);
  assert_eq!(resolve_package(&[("", "use pkg::a::{self, f as g}; fn main() { a::f(); g(); }"), ("a", "pub fn f() {}")]), Vec::<&str>::new());
  assert_eq!(resolve_package(&[("", "use a::f; fn main() {}")]), vec!["E0200"]);
  assert_eq!(resolve_package(&[("", "fn main() { std::io::read(); }")]), vec!["E0900"]);
  assert_eq!(
    resolve_package(&[("", "use pkg::a; fn main() { const _x: a::K = a::make(); }"), ("a", "struct K {} pub fn make() -> K { K {} }")]),
    vec!["E0207", "W0006"]
  );
}

#[test]
fn finds_the_modules_a_file_refers_to() {
  let text = "use pkg::net::{http, tcp::Socket}; use super::util as u;
    fn main() { http::get(); u::x::y(); self::local::f(); const _p: pkg::geo::Point = pkg::geo::Point {}; }";
  let mut sources = SourceMap::new();
  let mut diagnostics = Diagnostics::new();
  let file = sources.add(PathBuf::from("app.sag"), text.to_string());
  let tokens = sagitario_lexer::lex(file, text, &mut diagnostics);
  let mut module = sagitario_parser::parse_module(file, tokens, &mut diagnostics);

  module.path = vec!["app".to_string()];

  let found: Vec<String> = module_references(&module).iter().map(|path| path.join("::")).collect();

  assert_eq!(
    found,
    vec![
      "app::local",
      "app::local::f",
      "geo",
      "geo::Point",
      "net",
      "net::http",
      "net::http::get",
      "net::tcp",
      "net::tcp::Socket",
      "util",
      "util::x",
      "util::x::y"
    ]
  );
}
