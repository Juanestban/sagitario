//! The AST as text, for the differential tests of the self-hosted compiler (`sag check
//! --dump=ast`, docs/adr/0051-parser-port-and-ast-dumps.md). One node per line, indented two
//! spaces per level: `<field>: <Node> <start>..<end> [#<id>] [values]`. A list repeats its field,
//! an absent option is left out, and names are written `name@start..end`.

use sagitario_span::Span;

use crate::{
  ArrayLen, Block, Expr, ExprKind, FieldDecl, FnDecl, GenericParam, Ident, Item, MatchArm, Module, Param, Path, Pattern, PatternKind, Stmt,
  TemplatePart, TypeExpr, TypeExprKind, UseTree, UseTreeKind, VariantFields, Visibility,
};

pub fn dump(module: &Module) -> String {
  let mut dumper = Dumper { out: String::new(), depth: 0 };

  dumper.line("module", "Module", None, None, "");
  dumper.depth += 1;

  for decl in &module.uses {
    dumper.line("use", "Use", Some(decl.span), None, "");
    dumper.nested(|dumper| dumper.use_tree("tree", &decl.tree));
  }

  for item in &module.items {
    dumper.item(item);
  }

  dumper.out
}

struct Dumper {
  out: String,
  depth: usize,
}

fn span_text(span: Span) -> String {
  format!("{}..{}", span.start, span.end)
}

fn ident(name: &Ident) -> String {
  format!("{}@{}", name.name, span_text(name.span))
}

fn visibility(visibility: Visibility) -> &'static str {
  match visibility {
    Visibility::Module => "module",
    Visibility::Public => "pub",
    Visibility::Private => "prv",
  }
}

/// `text` between double quotes, as in the token dump.
fn quote(text: &str) -> String {
  let mut out = String::from("\"");

  for c in text.chars() {
    match c {
      '"' => out.push_str("\\\""),
      '\\' => out.push_str("\\\\"),
      '\n' => out.push_str("\\n"),
      '\r' => out.push_str("\\r"),
      '\t' => out.push_str("\\t"),
      ' '..='~' => out.push(c),
      other => out.push_str(&format!("\\u{{{:x}}}", u32::from(other))),
    }
  }

  out.push('"');
  out
}

fn suffixed(value: String, suffix: &Option<String>) -> String {
  match suffix {
    Some(suffix) => format!("{value} {suffix}"),
    None => value,
  }
}

impl Dumper {
  /// One node: its field in the parent, its kind, span, id and other values.
  fn line(&mut self, label: &str, kind: &str, span: Option<Span>, id: Option<u32>, values: &str) {
    let mut text = format!("{}{label}: {kind}", "  ".repeat(self.depth));

    if let Some(span) = span {
      text.push_str(&format!(" {}", span_text(span)));
    }

    if let Some(id) = id {
      text.push_str(&format!(" #{id}"));
    }

    if !values.is_empty() {
      text.push(' ');
      text.push_str(values);
    }

    text.push('\n');
    self.out.push_str(&text);
  }

  fn nested(&mut self, children: impl FnOnce(&mut Dumper)) {
    self.depth += 1;
    children(self);
    self.depth -= 1;
  }

  fn use_tree(&mut self, label: &str, tree: &UseTree) {
    let prefix: Vec<String> = tree.prefix.iter().map(ident).collect();

    match &tree.kind {
      UseTreeKind::Simple { alias } => {
        let alias = alias.as_ref().map_or(String::new(), |alias| format!(" as {}", ident(alias)));

        self.line(label, "UseTree", Some(tree.span), None, &format!("{}{alias}", prefix.join("::")));
      }
      UseTreeKind::Group(trees) => {
        self.line(label, "UseGroup", Some(tree.span), None, &prefix.join("::"));
        self.nested(|dumper| {
          for tree in trees {
            dumper.use_tree("tree", tree);
          }
        });
      }
    }
  }

  fn item(&mut self, item: &Item) {
    match item {
      Item::Fn(decl) => self.function("item", decl),
      Item::Struct(decl) => {
        let values = format!("{} {} repr_c={}", ident(&decl.name), visibility(decl.visibility), decl.repr_c);

        self.line("item", "Struct", Some(decl.span), None, &values);
        self.nested(|dumper| {
          dumper.generics(&decl.generics);
          dumper.types("impl", &decl.impls);
          dumper.fields(&decl.fields);
          dumper.functions(&decl.functions);
        });
      }
      Item::Enum(decl) => {
        self.line("item", "Enum", Some(decl.span), None, &format!("{} {}", ident(&decl.name), visibility(decl.visibility)));
        self.nested(|dumper| {
          dumper.generics(&decl.generics);
          dumper.types("impl", &decl.impls);

          for variant in &decl.variants {
            let shape = match &variant.fields {
              VariantFields::Unit => "unit",
              VariantFields::Tuple(_) => "tuple",
              VariantFields::Named(_) => "named",
            };

            dumper.line("variant", "Variant", Some(variant.span), None, &format!("{} {shape}", ident(&variant.name)));
            dumper.nested(|dumper| match &variant.fields {
              VariantFields::Unit => {}
              VariantFields::Tuple(types) => dumper.types("ty", types),
              VariantFields::Named(fields) => {
                for field in fields {
                  dumper.line("field", "VariantField", Some(field.span), None, &ident(&field.name));
                  dumper.nested(|dumper| dumper.type_expr("ty", &field.ty));
                }
              }
            });
          }

          dumper.functions(&decl.functions);
        });
      }
      Item::Class(decl) => {
        let values = format!("{} {} abstract={}", ident(&decl.name), visibility(decl.visibility), decl.is_abstract);

        self.line("item", "Class", Some(decl.span), None, &values);
        self.nested(|dumper| {
          dumper.generics(&decl.generics);

          if let Some(parent) = &decl.parent {
            dumper.type_expr("parent", parent);
          }

          dumper.types("impl", &decl.impls);
          dumper.fields(&decl.fields);
          dumper.functions(&decl.functions);
        });
      }
      Item::Interface(decl) => {
        self.line("item", "Interface", Some(decl.span), None, &format!("{} {}", ident(&decl.name), visibility(decl.visibility)));
        self.nested(|dumper| {
          dumper.generics(&decl.generics);
          dumper.types("parent", &decl.parents);
          dumper.fields(&decl.fields);
          dumper.functions(&decl.functions);
        });
      }
      Item::Extern(block) => {
        self.line("item", "Extern", Some(block.span), None, "");
        self.nested(|dumper| {
          for function in &block.functions {
            dumper.line("function", "ExternFn", Some(function.span), None, &format!("{} {}", ident(&function.name), visibility(function.visibility)));
            dumper.nested(|dumper| {
              dumper.params(&function.params);

              if let Some(ty) = &function.return_type {
                dumper.type_expr("return", ty);
              }
            });
          }
        });
      }
      Item::Const(decl) => {
        self.line("item", "Const", Some(decl.span), None, &format!("{} {}", ident(&decl.name), visibility(decl.visibility)));
        self.nested(|dumper| {
          dumper.type_expr("ty", &decl.ty);
          dumper.expr("value", &decl.value);
        });
      }
    }
  }

  fn generics(&mut self, generics: &[GenericParam]) {
    for param in generics {
      self.line("generic", "GenericParam", None, None, &ident(&param.name));
      self.nested(|dumper| dumper.types("bound", &param.bounds));
    }
  }

  fn types(&mut self, label: &str, types: &[TypeExpr]) {
    for ty in types {
      self.type_expr(label, ty);
    }
  }

  fn fields(&mut self, fields: &[FieldDecl]) {
    for field in fields {
      let mutable = match field.mutable {
        None => "none",
        Some(false) => "const",
        Some(true) => "var",
      };
      let values = format!("{} {} mutable={mutable} weak={}", ident(&field.name), visibility(field.visibility), field.weak);

      self.line("field", "Field", Some(field.span), None, &values);
      self.nested(|dumper| dumper.type_expr("ty", &field.ty));
    }
  }

  fn functions(&mut self, functions: &[FnDecl]) {
    for function in functions {
      self.function("function", function);
    }
  }

  fn params(&mut self, params: &[Param]) {
    for param in params {
      self.line("param", "Param", Some(param.span), None, &ident(&param.name));
      self.nested(|dumper| dumper.type_expr("ty", &param.ty));
    }
  }

  fn function(&mut self, label: &str, decl: &FnDecl) {
    let mut values = format!(
      "{} {} extern_c={} abstract={} override={}",
      ident(&decl.name),
      visibility(decl.visibility),
      decl.extern_c,
      decl.is_abstract,
      decl.is_override
    );

    if let Some(export) = &decl.export {
      values.push_str(&format!(" export={}@{}", quote(&export.symbol), span_text(export.span)));
    }

    if let Some(self_param) = &decl.self_param {
      values.push_str(&format!(" self@{}", span_text(self_param.span)));
    }

    self.line(label, "Fn", Some(decl.span), Some(decl.id.0), &values);
    self.nested(|dumper| {
      dumper.generics(&decl.generics);
      dumper.params(&decl.params);

      if let Some(ty) = &decl.return_type {
        dumper.type_expr("return", ty);
      }

      dumper.block("body", &decl.body);
    });
  }

  fn path(&mut self, label: &str, path: &Path) {
    let segments: Vec<String> = path.segments.iter().map(ident).collect();

    self.line(label, "Path", Some(path.span), Some(path.id.0), &segments.join("::"));

    if let Some(args) = &path.type_args {
      self.nested(|dumper| {
        dumper.line("type_args", "TypeArgs", Some(args.span), None, &format!("segment={}", args.segment));
        dumper.nested(|dumper| dumper.types("arg", &args.args));
      });
    }
  }

  fn array_len(&mut self, len: &ArrayLen) {
    self.line("len", "ArrayLen", Some(len.span), None, &suffixed(len.value.to_string(), &len.suffix));
  }

  fn type_expr(&mut self, label: &str, ty: &TypeExpr) {
    match &ty.kind {
      TypeExprKind::Unit => self.line(label, "UnitType", Some(ty.span), None, ""),
      TypeExprKind::Path(path) => {
        self.line(label, "PathType", Some(ty.span), None, "");
        self.nested(|dumper| dumper.path("path", path));
      }
      TypeExprKind::Generic { path, args } => {
        self.line(label, "GenericType", Some(ty.span), None, "");
        self.nested(|dumper| {
          dumper.path("path", path);
          dumper.types("arg", args);
        });
      }
      TypeExprKind::Array { elem, len } => {
        self.line(label, "ArrayType", Some(ty.span), None, "");
        self.nested(|dumper| {
          dumper.type_expr("elem", elem);
          dumper.array_len(len);
        });
      }
      TypeExprKind::Ptr { mutable, pointee } => {
        self.line(label, "PtrType", Some(ty.span), None, &format!("mutable={mutable}"));
        self.nested(|dumper| dumper.type_expr("pointee", pointee));
      }
      TypeExprKind::Fn { params, ret } => {
        self.line(label, "FnType", Some(ty.span), None, "");
        self.nested(|dumper| {
          dumper.types("param", params);

          if let Some(ret) = ret {
            dumper.type_expr("ret", ret);
          }
        });
      }
    }
  }

  fn block(&mut self, label: &str, block: &Block) {
    self.line(label, "Block", Some(block.span), None, "");
    self.nested(|dumper| {
      for stmt in &block.stmts {
        dumper.stmt(stmt);
      }

      if let Some(tail) = &block.tail {
        dumper.expr("tail", tail);
      }
    });
  }

  fn stmt(&mut self, stmt: &Stmt) {
    match stmt {
      Stmt::Let(decl) => {
        self.line("stmt", "Let", Some(decl.span), Some(decl.id.0), &format!("{} mutable={}", ident(&decl.name), decl.mutable));
        self.nested(|dumper| {
          if let Some(ty) = &decl.ty {
            dumper.type_expr("ty", ty);
          }

          if let Some(init) = &decl.init {
            dumper.expr("init", init);
          }
        });
      }
      Stmt::Assign { target, op, value, span } => {
        let op = op.map_or("=", |op| op.symbol());

        self.line("stmt", "Assign", Some(*span), None, &format!("op={op}"));
        self.nested(|dumper| {
          dumper.expr("target", target);
          dumper.expr("value", value);
        });
      }
      Stmt::Expr(expr) => self.expr("stmt", expr),
    }
  }

  fn exprs(&mut self, label: &str, exprs: &[Expr]) {
    for expr in exprs {
      self.expr(label, expr);
    }
  }

  fn expr(&mut self, label: &str, expr: &Expr) {
    let line = |dumper: &mut Dumper, kind: &str, values: &str| dumper.line(label, kind, Some(expr.span), Some(expr.id.0), values);

    match &expr.kind {
      ExprKind::Int { value, suffix } => line(self, "Int", &suffixed(value.to_string(), suffix)),
      ExprKind::Float { text, suffix } => line(self, "Float", &suffixed(text.clone(), suffix)),
      ExprKind::Bool(value) => line(self, "Bool", &value.to_string()),
      ExprKind::Char(value) => line(self, "Char", &quote(&value.to_string())),
      ExprKind::Str(value) => line(self, "Str", &quote(value)),
      ExprKind::Template(parts) => {
        line(self, "Template", "");
        self.nested(|dumper| {
          for part in parts {
            match part {
              TemplatePart::Text(text) => dumper.line("part", "Text", None, None, &quote(text)),
              TemplatePart::Expr(expr) => dumper.expr("part", expr),
            }
          }
        });
      }
      ExprKind::Unit => line(self, "Unit", ""),
      ExprKind::Path(path) => {
        line(self, "PathExpr", "");
        self.nested(|dumper| dumper.path("path", path));
      }
      ExprKind::Call { callee, args } => {
        line(self, "Call", "");
        self.nested(|dumper| {
          dumper.expr("callee", callee);
          dumper.exprs("arg", args);
        });
      }
      ExprKind::MethodCall { receiver, method, args } => {
        line(self, "MethodCall", &ident(method));
        self.nested(|dumper| {
          dumper.expr("receiver", receiver);
          dumper.exprs("arg", args);
        });
      }
      ExprKind::Field { base, name } => {
        line(self, "Field", &ident(name));
        self.nested(|dumper| dumper.expr("base", base));
      }
      ExprKind::Index { base, index } => {
        line(self, "Index", "");
        self.nested(|dumper| {
          dumper.expr("base", base);
          dumper.expr("index", index);
        });
      }
      ExprKind::StructLit { path, fields } => {
        line(self, "StructLit", "");
        self.nested(|dumper| {
          dumper.path("path", path);

          for field in fields {
            dumper.line("field", "FieldInit", Some(field.span), None, &ident(&field.name));
            dumper.nested(|dumper| dumper.expr("value", &field.value));
          }
        });
      }
      ExprKind::ArrayLit(elems) => {
        line(self, "ArrayLit", "");
        self.nested(|dumper| dumper.exprs("elem", elems));
      }
      ExprKind::ArrayRepeat { value, len } => {
        line(self, "ArrayRepeat", "");
        self.nested(|dumper| {
          dumper.expr("value", value);
          dumper.array_len(len);
        });
      }
      ExprKind::Unary { op, operand } => {
        let op = match op {
          crate::UnaryOp::Neg => "-",
          crate::UnaryOp::Not => "!",
          crate::UnaryOp::Deref => "*",
        };

        line(self, "Unary", &format!("op={op}"));
        self.nested(|dumper| dumper.expr("operand", operand));
      }
      ExprKind::Binary { op, lhs, rhs } => {
        line(self, "Binary", &format!("op={}", op.symbol()));
        self.nested(|dumper| {
          dumper.expr("lhs", lhs);
          dumper.expr("rhs", rhs);
        });
      }
      ExprKind::Cast { expr: inner, ty } => {
        line(self, "Cast", "");
        self.nested(|dumper| {
          dumper.expr("expr", inner);
          dumper.type_expr("ty", ty);
        });
      }
      ExprKind::Range { start, end, inclusive } => {
        line(self, "Range", &format!("inclusive={inclusive}"));
        self.nested(|dumper| {
          dumper.expr("start", start);
          dumper.expr("end", end);
        });
      }
      ExprKind::Try(inner) => {
        line(self, "Try", "");
        self.nested(|dumper| dumper.expr("expr", inner));
      }
      ExprKind::Zod => line(self, "Zod", ""),
      ExprKind::Lambda { params, body } => {
        line(self, "Lambda", "");
        self.nested(|dumper| {
          for param in params {
            dumper.line("param", "LambdaParam", Some(param.span), None, &ident(&param.name));

            if let Some(ty) = &param.ty {
              dumper.nested(|dumper| dumper.type_expr("ty", ty));
            }
          }

          dumper.expr("body", body);
        });
      }
      ExprKind::New { path, args } => {
        line(self, "New", "");
        self.nested(|dumper| {
          dumper.path("path", path);
          dumper.exprs("arg", args);
        });
      }
      ExprKind::Match { scrutinee, arms } => {
        line(self, "Match", "");
        self.nested(|dumper| {
          dumper.expr("scrutinee", scrutinee);

          for arm in arms {
            dumper.arm(arm);
          }
        });
      }
      ExprKind::Block(block) => {
        line(self, "BlockExpr", "");
        self.nested(|dumper| dumper.block("block", block));
      }
      ExprKind::Unsafe(block) => {
        line(self, "Unsafe", "");
        self.nested(|dumper| dumper.block("block", block));
      }
      ExprKind::If { cond, then_block, else_branch } => {
        line(self, "If", "");
        self.nested(|dumper| {
          dumper.expr("cond", cond);
          dumper.block("then", then_block);

          if let Some(branch) = else_branch {
            dumper.expr("else", branch);
          }
        });
      }
      ExprKind::While { cond, body } => {
        line(self, "While", "");
        self.nested(|dumper| {
          dumper.expr("cond", cond);
          dumper.block("body", body);
        });
      }
      ExprKind::Loop { body } => {
        line(self, "Loop", "");
        self.nested(|dumper| dumper.block("body", body));
      }
      ExprKind::For { var, iter, body } => {
        line(self, "For", &ident(var));
        self.nested(|dumper| {
          dumper.expr("iter", iter);
          dumper.block("body", body);
        });
      }
      ExprKind::Break { value } => {
        line(self, "Break", "");

        if let Some(value) = value {
          self.nested(|dumper| dumper.expr("value", value));
        }
      }
      ExprKind::Continue => line(self, "Continue", ""),
      ExprKind::Return { value } => {
        line(self, "Return", "");

        if let Some(value) = value {
          self.nested(|dumper| dumper.expr("value", value));
        }
      }
    }
  }

  fn arm(&mut self, arm: &MatchArm) {
    self.line("arm", "MatchArm", Some(arm.span), None, "");
    self.nested(|dumper| {
      dumper.pattern("pattern", &arm.pattern);

      if let Some(guard) = &arm.guard {
        dumper.expr("guard", guard);
      }

      dumper.expr("body", &arm.body);
    });
  }

  fn pattern(&mut self, label: &str, pattern: &Pattern) {
    let line = |dumper: &mut Dumper, kind: &str, values: &str| dumper.line(label, kind, Some(pattern.span), Some(pattern.id.0), values);

    match &pattern.kind {
      PatternKind::Wildcard => line(self, "Wildcard", ""),
      PatternKind::Ident(name) => line(self, "Binding", &ident(name)),
      PatternKind::Literal(expr) => {
        line(self, "LiteralPattern", "");
        self.nested(|dumper| dumper.expr("expr", expr));
      }
      PatternKind::Path(path) => {
        line(self, "PathPattern", "");
        self.nested(|dumper| dumper.path("path", path));
      }
      PatternKind::Tuple { path, elems } => {
        line(self, "TuplePattern", "");
        self.nested(|dumper| {
          dumper.path("path", path);

          for elem in elems {
            dumper.pattern("elem", elem);
          }
        });
      }
      PatternKind::Named { path, fields, rest } => {
        line(self, "NamedPattern", &format!("rest={rest}"));
        self.nested(|dumper| {
          dumper.path("path", path);

          for field in fields {
            dumper.line("field", "FieldPattern", Some(field.span), None, &ident(&field.name));
            dumper.nested(|dumper| dumper.pattern("pattern", &field.pattern));
          }
        });
      }
      PatternKind::Or(alternatives) => {
        line(self, "OrPattern", "");
        self.nested(|dumper| {
          for alternative in alternatives {
            dumper.pattern("alt", alternative);
          }
        });
      }
    }
  }
}
