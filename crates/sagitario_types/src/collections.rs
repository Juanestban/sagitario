//! Collections and iteration (docs/spec/02-types.md#lists, docs/spec/08-closures.md#iterators,
//! ADR-0034): the built-in `List<T>`, the prelude's `Iter<T>` and `Map<K, V>`, ranges used as
//! values, and the prelude interfaces `Eq`, `Ord` and `Hash`, which integers, `bool`, `char` and
//! `String` implement. Several methods are prelude functions that receive the receiver first.

use sagitario_ast::{Expr, ExprKind, Ident, Path};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, EnumId, InterfaceId, ParamOwner, StructId, TypeItem};

use crate::generic::Subst;
use crate::{BuiltinMethod, Checker, ClassRef, EnumRef, IntTy, InterfaceRef, ListMethod, StructRef, Type, TypedModule};

/// The prelude items the compiler relies on.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PreludeItems {
  pub ordering: Option<EnumId>,
  pub eq: Option<InterfaceId>,
  pub ord: Option<InterfaceId>,
  pub hash: Option<InterfaceId>,
  pub entry: Option<StructId>,
  pub iter: Option<ClassId>,
  pub map: Option<ClassId>,
}

/// The methods of `Iter` that lists and arrays also have, as shorthands for `.iter().method()`.
pub const ITER_METHODS: &[&str] = &["map", "filter", "take", "skip", "step_by", "collect", "count", "fold", "for_each", "any", "all", "find"];

/// The methods of `List` implemented by the compiler.
const LIST_METHODS: &[&str] = &["len", "is_empty", "push", "pop", "get", "first", "last", "insert", "remove", "clear", "reverse"];

/// The methods of `List` implemented by prelude functions, with the function.
const LIST_PRELUDE_METHODS: &[(&str, &str)] = &[
  ("iter", "list_iter"),
  ("contains", "list_contains"),
  ("index_of", "list_index_of"),
  ("sort", "list_sort"),
  ("sort_by", "list_sort_by"),
  ("join", "list_join"),
];

impl TypedModule {
  /// `Iter<elem>`.
  pub fn iter_type(&self, elem: Type) -> Type {
    match self.prelude.iter {
      Some(id) => Type::Class(ClassRef { id, name: "Iter".to_string(), args: vec![elem] }),
      None => Type::Error,
    }
  }

  /// The element type of an `Iter<T>`.
  pub fn iter_elem(&self, ty: &Type) -> Option<Type> {
    match ty {
      Type::Class(class) if Some(class.id) == self.prelude.iter => class.args.first().cloned(),
      _ => None,
    }
  }

  /// The key and value types of a `Map<K, V>`.
  pub fn map_args(&self, ty: &Type) -> Option<(Type, Type)> {
    match ty {
      Type::Class(class) if Some(class.id) == self.prelude.map => match class.args.as_slice() {
        [key, value] => Some((key.clone(), value.clone())),
        _ => None,
      },
      _ => None,
    }
  }

  /// `Entry<K, V>`.
  pub fn entry_type(&self, key: Type, value: Type) -> Type {
    match self.prelude.entry {
      Some(id) => Type::Struct(StructRef { id, name: "Entry".to_string(), args: vec![key, value] }),
      None => Type::Error,
    }
  }

  /// The prelude's `Ordering`.
  pub fn ordering_type(&self) -> Type {
    match self.prelude.ordering {
      Some(id) => Type::Enum(EnumRef { id, name: "Ordering".to_string(), args: Vec::new() }),
      None => Type::Error,
    }
  }

  /// The element type `for` iterates over in a list, an iterator or a map (its entries).
  pub fn collection_elem(&self, ty: &Type) -> Option<Type> {
    match ty {
      Type::List(elem) => Some(elem.as_ref().clone()),
      _ => self.iter_elem(ty).or_else(|| self.map_args(ty).map(|(key, value)| self.entry_type(key, value))),
    }
  }

  /// The interfaces integers, `bool`, `char` and `String` implement: `Ord` and `Hash`, and
  /// through them `Eq`.
  pub(crate) fn primitive_impls(&self, ty: &Type) -> Vec<InterfaceRef> {
    if !matches!(ty, Type::Int(_) | Type::Bool | Type::Char | Type::String) {
      return Vec::new();
    }

    [(self.prelude.ord, "Ord"), (self.prelude.hash, "Hash")]
      .into_iter()
      .filter_map(|(id, name)| id.map(|id| InterfaceRef { id, name: name.to_string(), args: Vec::new() }))
      .collect()
  }
}

impl Checker<'_> {
  /// Finds the prelude items the compiler relies on.
  pub(crate) fn prelude_items(&mut self) {
    let resolution = self.resolution;
    let interface = |name: &str| match resolution.prelude_type(name) {
      Some(TypeItem::Interface(id)) => Some(id),
      _ => None,
    };
    let class = |name: &str| match resolution.prelude_type(name) {
      Some(TypeItem::Class(id)) => Some(id),
      _ => None,
    };

    self.typed.prelude = PreludeItems {
      ordering: match resolution.prelude_type("Ordering") {
        Some(TypeItem::Enum(id)) => Some(id),
        _ => None,
      },
      eq: interface("Eq"),
      ord: interface("Ord"),
      hash: interface("Hash"),
      entry: match resolution.prelude_type("Entry") {
        Some(TypeItem::Struct(id)) => Some(id),
        _ => None,
      },
      iter: class("Iter"),
      map: class("Map"),
    };
  }

  /// `new List<T>()`: the element type is written or comes from the expected type.
  pub(crate) fn new_list(&mut self, path: &Path, args: &[Expr], expected: Option<&Type>) -> Type {
    let explicit = self.explicit_args(path).map(|(_, types)| types);

    self.args(path.span, "constructor", &[], args);

    let elem = match explicit {
      Some(types) => {
        if !self.check_arg_count(path.span, "`List`", 1, types.len()) {
          return Type::Error;
        }

        types.into_iter().next().unwrap_or(Type::Error)
      }
      None => match expected {
        Some(Type::List(elem)) => elem.as_ref().clone(),
        _ => {
          self.diagnostics.push(
            Diagnostic::error(codes::UNKNOWN_TYPE, path.span, "cannot infer the element type of this list")
              .with_help("write it, as in `new List<i32>()`, or annotate the binding: `const list: List<i32> = new List();`"),
          );

          return Type::Error;
        }
      },
    };

    Type::List(Box::new(elem))
  }

  /// `List::from(array)`: a list with copies of the elements of an array.
  pub(crate) fn list_from(&mut self, expr: &Expr, callee: &Expr, args: &[Expr], expected: Option<&Type>) -> Type {
    let explicit = match &callee.kind {
      ExprKind::Path(path) => self.explicit_args(path).map(|(_, types)| types),
      _ => None,
    };
    let written = match explicit {
      Some(types) if self.check_arg_count(callee.span, "`List`", 1, types.len()) => types.into_iter().next(),
      Some(_) => Some(Type::Error),
      None => None,
    };
    let hint = written.clone().or_else(|| match expected {
      Some(Type::List(elem)) => Some(elem.as_ref().clone()),
      _ => None,
    });
    let [array] = args else {
      self.args(expr.span, "function", &[Type::Array(Box::new(hint.unwrap_or(Type::Error)), 0)], args);

      return Type::Error;
    };
    let elem = match self.expr(array, hint.map(|elem| Type::Array(Box::new(elem), 0)).as_ref()) {
      Type::Array(elem, _) => *elem,
      Type::Error | Type::Never => return Type::Error,
      other => {
        self.error(codes::MISMATCHED_TYPES, array.span, format!("expected an array, found `{other}`"));

        return Type::Error;
      }
    };

    if let Some(written) = written.filter(|written| !self.fits(&elem, written)) {
      self.mismatch(array.span, &Type::Array(Box::new(written), 0), &Type::Array(Box::new(elem.clone()), 0));
    }

    Type::List(Box::new(elem))
  }

  /// A method call on a `List<T>`.
  pub(crate) fn list_method(&mut self, expr: &Expr, receiver: &Expr, list_ty: &Type, method: &Ident, args: &[Expr]) -> Type {
    let Type::List(elem) = list_ty else { return Type::Error };
    let elem = elem.as_ref().clone();
    let name = method.name.as_str();
    let index = Type::Int(IntTy::Usize);
    let option = Type::Option(Box::new(elem.clone()));
    let core = match name {
      "len" => Some((ListMethod::Len, Vec::new(), index.clone())),
      "is_empty" => Some((ListMethod::IsEmpty, Vec::new(), Type::Bool)),
      "push" => Some((ListMethod::Push, vec![elem.clone()], Type::Unit)),
      "pop" => Some((ListMethod::Pop, Vec::new(), option)),
      "get" => Some((ListMethod::Get, vec![index.clone()], option)),
      "first" => Some((ListMethod::First, Vec::new(), option)),
      "last" => Some((ListMethod::Last, Vec::new(), option)),
      "insert" => Some((ListMethod::Insert, vec![index.clone(), elem.clone()], Type::Unit)),
      "remove" => Some((ListMethod::Remove, vec![index.clone()], elem.clone())),
      "clear" => Some((ListMethod::Clear, Vec::new(), Type::Unit)),
      "reverse" => Some((ListMethod::Reverse, Vec::new(), Type::Unit)),
      _ => None,
    };

    if let Some((op, params, ret)) = core {
      self.args(expr.span, "method", &params, args);
      self.typed.builtin_methods.insert(expr.id, BuiltinMethod::List(op));

      return ret;
    }

    if let Some((_, function)) = LIST_PRELUDE_METHODS.iter().find(|(method, _)| *method == name) {
      if name == "join" && !matches!(elem, Type::String | Type::Error) {
        self.diagnostics.push(
          Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `join` on type `{list_ty}`"))
            .with_note("only lists of strings (`List<String>`) can be joined"),
        );
        self.unchecked_args(args);

        return Type::Error;
      }

      return self.prelude_method(expr, function, vec![elem], args);
    }

    if ITER_METHODS.contains(&name) {
      return self.iter_shorthand(expr, receiver, elem, method, args);
    }

    let names = LIST_METHODS.iter().chain(LIST_PRELUDE_METHODS.iter().map(|(method, _)| method)).chain(ITER_METHODS).copied();
    let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{list_ty}`"));

    if let Some(similar) = sagitario_diagnostics::closest_match(name, names) {
      diagnostic = diagnostic.with_help(format!("a method with a similar name exists: `{similar}`"));
    }

    self.diagnostics.push(diagnostic);
    self.unchecked_args(args);

    Type::Error
  }

  /// `array.iter()` and the iterator methods of arrays, which work on a list with copies of the
  /// elements; `None` for other methods.
  pub(crate) fn array_method(&mut self, expr: &Expr, receiver: &Expr, elem: &Type, method: &Ident, args: &[Expr]) -> Option<Type> {
    match method.name.as_str() {
      "iter" => Some(self.prelude_method(expr, "list_iter", vec![elem.clone()], args)),
      name if ITER_METHODS.contains(&name) => Some(self.iter_shorthand(expr, receiver, elem.clone(), method, args)),
      _ => None,
    }
  }

  /// A method implemented by a prelude function, which receives the receiver first and has
  /// the type arguments `type_args` if it is generic.
  pub(crate) fn prelude_method(&mut self, expr: &Expr, name: &str, type_args: Vec<Type>, args: &[Expr]) -> Type {
    let Some((function, sig)) = self.resolution.prelude_fn(name).and_then(|function| Some((function, self.typed.sig(function)?.clone()))) else {
      self.unchecked_args(args);

      return Type::Error;
    };
    let generic = self.generic_count(ParamOwner::Fn(function)) > 0;
    let subst = if generic { Subst::of(ParamOwner::Fn(function), type_args.clone()) } else { Subst::default() };
    let params: Vec<Type> = sig.params.iter().skip(1).map(|param| param.substitute(&subst)).collect();

    self.args(expr.span, "method", &params, args);

    if generic {
      self.check_bounds(expr.span, &subst);
      self.typed.call_args.insert(expr.id, type_args);
    }

    self.typed.builtin_methods.insert(expr.id, BuiltinMethod::Prelude(function));

    sig.ret.substitute(&subst)
  }

  /// `receiver.method(args)` on a list or array, for a method of `Iter`: the receiver becomes
  /// an iterator first.
  fn iter_shorthand(&mut self, expr: &Expr, receiver: &Expr, elem: Type, method: &Ident, args: &[Expr]) -> Type {
    let found = self.typed.prelude.iter.and_then(|iter| self.class_method(iter, &method.name));
    let (Some(function), Some(to_iter)) = (found, self.resolution.prelude_fn("list_iter")) else {
      self.unchecked_args(args);

      return Type::Error;
    };
    let iter_ty = self.typed.iter_type(elem.clone());

    self.typed.to_iter.insert(receiver.id, (to_iter, vec![elem]));
    self.typed.method_calls.insert(expr.id, function);

    self.generic_method_call(expr, &iter_ty, function, args)
  }

  /// `equals`, `compare` and `hash` on integers, `bool`, `char` and `String`, and
  /// `total_compare` on floats; `None` for other methods.
  pub(crate) fn primitive_method(&mut self, expr: &Expr, receiver_ty: &Type, method: &Ident, args: &[Expr]) -> Option<Type> {
    let name = method.name.as_str();

    if let Type::Float(_) = receiver_ty {
      if name == "total_compare" {
        self.args(expr.span, "method", std::slice::from_ref(receiver_ty), args);
        self.typed.builtin_methods.insert(expr.id, BuiltinMethod::TotalCompare);

        return Some(self.typed.ordering_type());
      }

      if matches!(name, "equals" | "compare" | "hash") {
        self.diagnostics.push(
          Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{name}` on type `{receiver_ty}`"))
            .with_note("floats do not implement `Eq`, `Ord` or `Hash`, since `NaN` is not equal to itself")
            .with_help("compare floats with `==` and `<`, or order them with `total_compare`"),
        );
        self.unchecked_args(args);

        return Some(Type::Error);
      }

      return None;
    }

    if !matches!(receiver_ty, Type::Int(_) | Type::Bool | Type::Char | Type::String) {
      return None;
    }

    let interface = match name {
      "equals" => self.typed.prelude.eq,
      "compare" => self.typed.prelude.ord,
      "hash" => self.typed.prelude.hash,
      _ => return None,
    }?;
    let function = self.typed.interface_def(interface)?.methods.first().copied()?;

    self.typed.method_calls.insert(expr.id, function);

    Some(self.generic_method_call(expr, receiver_ty, function, args))
  }

  /// `start..end` outside of `for`: an `Iter` over the integers in it.
  pub(crate) fn range_value(&mut self, expr: &Expr, start: &Expr, end: &Expr, expected: Option<&Type>) -> Type {
    let hint = expected.and_then(|expected| self.typed.iter_elem(expected)).filter(|elem| matches!(elem, Type::Int(_)));
    let int = match self.same_type_operands(start, end, hint.as_ref()) {
      Type::Int(int) => int,
      Type::Error | Type::Never => return Type::Error,
      other => {
        self.error(codes::INVALID_OPERAND, expr.span, format!("range bounds must be integers, found `{other}`"));

        return Type::Error;
      }
    };
    let Some(function) = self.resolution.prelude_fn(&format!("range_{}", int.name())) else { return Type::Error };

    self.typed.ranges.insert(expr.id, function);

    self.typed.iter_type(Type::Int(int))
  }

  /// `(start..end).rev()`: the range's integers from the last one down.
  pub(crate) fn range_rev(&mut self, expr: &Expr, receiver: &Expr, receiver_ty: &Type, method: &Ident, args: &[Expr]) -> Option<Type> {
    if method.name != "rev" || self.typed.iter_elem(receiver_ty).is_none() {
      return None;
    }

    if !matches!(receiver.kind, ExprKind::Range { .. }) {
      self.diagnostics.push(
        Diagnostic::error(codes::UNKNOWN_MEMBER, method.span, format!("no method `rev` on type `{receiver_ty}`"))
          .with_note("only ranges can be reversed, as in `(0..10).rev()`; collect other iterators into a list and call `reverse`"),
      );
      self.unchecked_args(args);

      return Some(Type::Error);
    }

    self.args(expr.span, "method", &[], args);
    self.typed.builtin_methods.insert(expr.id, BuiltinMethod::RangeRev);

    Some(receiver_ty.clone())
  }
}
