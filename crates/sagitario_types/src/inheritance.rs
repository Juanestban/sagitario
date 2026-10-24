//! Inheritance between classes (docs/spec/04-classes.md#inheritance, ADR-0029): the parent of
//! each class, members inherited through the ancestors, `override` and `abstract`, the vtables
//! of methods dispatched dynamically, and calls through `zod`.

use std::collections::HashSet;

use sagitario_ast::{Expr, Ident, TypeExpr, Visibility};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, FnId, TypeItem};

use crate::{Checker, ClassRef, Type};

impl Checker<'_> {
  /// The class named after `extends`, with its type arguments.
  pub(crate) fn parent_class(&mut self, ty: &TypeExpr) -> Option<ClassRef> {
    match self.type_expr(ty) {
      Type::Class(class) => Some(class),
      Type::Error => None,
      other => {
        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_INHERITANCE, ty.span, format!("only classes can be extended, and `{other}` is not a class"))
            .with_note("structs and enums have no inheritance"),
        );

        None
      }
    }
  }

  /// Reports classes that inherit from themselves and removes their parent.
  pub(crate) fn check_inheritance_cycles(&mut self) {
    let count = self.typed.classes.len();
    let parent = |checker: &Self, id: ClassId| checker.typed.class_def(id).and_then(|def| def.parent.as_ref().map(|parent| parent.id));
    let mut cyclic = Vec::new();

    for index in 0..count {
      let start = ClassId(index as u32);
      let mut current = parent(self, start);
      let mut steps = 0;

      while let Some(class) = current {
        if class == start {
          cyclic.push(start);

          break;
        }

        steps += 1;

        if steps > count {
          break;
        }

        current = parent(self, class);
      }
    }

    for id in cyclic {
      if let Some(decl) = self.resolution.class_decl(self.package, id) {
        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_INHERITANCE, decl.name.span, format!("the class `{}` inherits from itself", decl.name.name))
            .with_note("the chain of `extends` must end in a class without a parent"),
        );
      }

      if let Some(def) = self.typed.classes.get_mut(id.0 as usize) {
        def.parent = None;
      }
    }
  }

  /// A function of a class or, if it is a method, of one of its ancestors. Initializers and
  /// static functions are not inherited.
  pub(crate) fn class_method(&self, id: ClassId, name: &str) -> Option<FnId> {
    for (depth, class) in self.typed.class_chain(id).into_iter().enumerate() {
      if let Some(function) = self.resolution.type_function(self.package, TypeItem::Class(class), name) {
        let method = self.resolution.is_method(self.package, function);

        return (depth == 0 || method).then_some(function);
      }
    }

    None
  }

  fn class_name(&self, id: ClassId) -> String {
    self.resolution.type_name(self.package, TypeItem::Class(id))
  }

  /// The members a class declares against those it inherits: no field or function may reuse an
  /// inherited name, except an `override` of a public method with the same signature.
  pub(crate) fn check_members(&mut self) {
    for index in 0..self.resolution.classes.len() {
      let id = ClassId(index as u32);
      let (Some(decl), Some(parent)) =
        (self.resolution.class_decl(self.package, id), self.typed.class_def(id).and_then(|def| def.parent.as_ref().map(|parent| parent.id)))
      else {
        continue;
      };

      for field in &decl.fields {
        self.inherited_name_clash(parent, &field.name);
      }

      let functions = self.resolution.classes.get(index).map(|info| info.functions.clone()).unwrap_or_default();

      for (function, fn_decl) in functions.into_iter().zip(&decl.functions) {
        if self.resolution.is_initializer(self.package, function) || self.inherited_name_clash(parent, &fn_decl.name) {
          continue;
        }

        let name = &fn_decl.name;
        let inherited = self.class_method(parent, &name.name);
        let Some(inherited) = inherited else {
          if fn_decl.is_override {
            self.diagnostics.push(
              Diagnostic::error(codes::INVALID_OVERRIDE, name.span, format!("`{}` overrides nothing", name.name))
                .with_note(format!("no ancestor of `{}` declares a method `{}`", decl.name.name, name.name))
                .with_help("remove `override`"),
            );
          }

          continue;
        };
        let Some(inherited_decl) = self.resolution.fn_decl(self.package, inherited) else { continue };

        // Overrides are called through the vtable, which holds one function per slot.
        if fn_decl.is_override && (!fn_decl.generics.is_empty() || !inherited_decl.generics.is_empty()) {
          self.error(codes::INVALID_OVERRIDE, name.span, format!("`{}` has type parameters, so it cannot be overridden", name.name));

          continue;
        }
        let owner = self.resolution.owner(inherited).map(|owner| self.resolution.type_name(self.package, owner)).unwrap_or_default();
        let private = inherited_decl.visibility == Visibility::Private;

        if !self.resolution.is_method(self.package, function) || (private && !fn_decl.is_override) {
          let note = if private { "private methods cannot be overridden" } else { "static functions cannot reuse the name of an inherited method" };

          self.diagnostics.push(
            Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("`{}` is already a method of `{owner}`", name.name))
              .with_label(inherited_decl.name.span, "declared here")
              .with_note(note),
          );
        } else if !fn_decl.is_override {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_OVERRIDE, name.span, format!("`{}` replaces the method of `{owner}` without `override`", name.name))
              .with_label(inherited_decl.name.span, "overridden method declared here")
              .with_help(format!("write `pub override fn {}`", name.name)),
          );
        } else if private {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_OVERRIDE, name.span, format!("cannot override `{}`: it is private to `{owner}`", name.name))
              .with_label(inherited_decl.name.span, "declared `prv` here"),
          );
        } else if fn_decl.visibility == Visibility::Private {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_OVERRIDE, name.span, "an `override` must be `pub`, like the method it overrides")
              .with_help("write `pub override fn`"),
          );
        } else if !self.same_override_sig(id, function, inherited) {
          let expected = self.signature_text(inherited);

          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_OVERRIDE, name.span, format!("`{}` must have the same signature as the method it overrides", name.name))
              .with_label(inherited_decl.name.span, format!("overridden method declared here in `{owner}`"))
              .with_note(format!("expected `{expected}`")),
          );
        }
      }
    }
  }

  /// Whether `function` of class `id` has the signature of the inherited method it overrides,
  /// with the type arguments the class gives its ancestor.
  fn same_override_sig(&self, id: ClassId, function: FnId, inherited: FnId) -> bool {
    let class_ty = self.type_ref(TypeItem::Class(id));
    let expected = self.typed.sig(inherited).map(|sig| sig.substitute(&self.inherited_subst(&class_ty, inherited)));

    match (self.typed.sig(function), expected) {
      (Some(actual), Some(expected)) => actual.params == expected.params && actual.ret == expected.ret,
      _ => true,
    }
  }

  /// Reports a member named like an inherited field, and returns whether it did.
  fn inherited_name_clash(&mut self, parent: ClassId, name: &Ident) -> bool {
    let Some((owner, _, field)) = self.typed.class_field(parent, &name.name).map(|(owner, index, field)| (owner, index, field.span)) else {
      return false;
    };
    let owner = self.class_name(owner);

    self.diagnostics.push(
      Diagnostic::error(codes::DUPLICATE_DEFINITION, name.span, format!("`{}` is already a field of `{owner}`", name.name))
        .with_label(field, "declared here")
        .with_note("a class inherits every field of its ancestors; fields cannot be redeclared"),
    );

    true
  }

  /// `fn name(self, A, B) -> R`, for messages.
  fn signature_text(&self, function: FnId) -> String {
    let name = self.resolution.fn_decl(self.package, function).map_or("", |decl| decl.name.name.as_str());
    let Some(sig) = self.typed.sig(function) else { return String::new() };
    let params: Vec<String> = std::iter::once("self".to_string()).chain(sig.params.iter().map(Type::to_string)).collect();
    let ret = if sig.ret == Type::Unit { String::new() } else { format!(" -> {}", sig.ret) };

    format!("fn {name}({}){ret}", params.join(", "))
  }

  /// Abstract methods only in abstract classes, as public methods; concrete classes implement
  /// every abstract method they inherit.
  pub(crate) fn check_abstract_methods(&mut self) {
    for index in 0..self.resolution.classes.len() {
      let id = ClassId(index as u32);
      let Some(decl) = self.resolution.class_decl(self.package, id) else { continue };

      for function in decl.functions.iter().filter(|function| function.is_abstract) {
        let name = &function.name.name;
        let problem = if function.self_param.is_none() {
          Some((codes::INVALID_OVERRIDE, format!("the abstract function `{name}` must be a method that takes `self`")))
        } else if function.visibility == Visibility::Private {
          Some((codes::INVALID_OVERRIDE, format!("the abstract method `{name}` cannot be `prv`: subclasses must be able to override it")))
        } else if function.is_override {
          Some((codes::INVALID_OVERRIDE, format!("the method `{name}` cannot be both `abstract` and `override`")))
        } else if !function.generics.is_empty() {
          Some((codes::INVALID_OVERRIDE, format!("the abstract method `{name}` cannot have type parameters: it must be overridden")))
        } else if !decl.is_abstract {
          Some((codes::INVALID_INHERITANCE, format!("the class `{}` is not abstract, so its method `{name}` needs a body", decl.name.name)))
        } else {
          None
        };

        if let Some((code, message)) = problem {
          self.diagnostics.push(Diagnostic::error(code, function.name.span, message));
        }
      }

      if decl.is_abstract {
        continue;
      }

      for function in self.implementations(id) {
        // Malformed abstract methods were reported where they are declared.
        let Some(abstract_decl) = self
          .resolution
          .fn_decl(self.package, function)
          .filter(|decl| decl.is_abstract && decl.self_param.is_some() && decl.name.name != "new" && decl.visibility != Visibility::Private)
        else {
          continue;
        };

        // An abstract method of this class was reported above.
        if self.resolution.owner(function) == Some(TypeItem::Class(id)) {
          continue;
        }

        let owner = self.resolution.owner(function).map(|owner| self.resolution.type_name(self.package, owner)).unwrap_or_default();
        let name = &abstract_decl.name.name;

        self.diagnostics.push(
          Diagnostic::error(
            codes::INVALID_INHERITANCE,
            decl.name.span,
            format!("the class `{}` does not implement the abstract method `{name}` of `{owner}`", decl.name.name),
          )
          .with_label(abstract_decl.name.span, "declared `abstract` here")
          .with_help(format!("add `pub override fn {name}(...)`, or declare `abstract class {}`", decl.name.name)),
        );
      }
    }
  }

  /// The method each name runs on instances of a class: its own, or the nearest ancestor's.
  fn implementations(&self, id: ClassId) -> Vec<FnId> {
    let mut methods: Vec<(String, FnId)> = Vec::new();

    for class in self.typed.class_chain(id).into_iter().rev() {
      let functions = self.resolution.classes.get(class.0 as usize).map(|info| info.functions.clone()).unwrap_or_default();

      for function in functions {
        let Some(decl) = self.resolution.fn_decl(self.package, function).filter(|_| self.resolution.is_method(self.package, function)) else {
          continue;
        };

        match methods.iter_mut().find(|(name, _)| *name == decl.name.name) {
          Some(entry) => entry.1 = function,
          None => methods.push((decl.name.name.clone(), function)),
        }
      }
    }

    methods.into_iter().map(|(_, function)| function).collect()
  }

  /// The method an override ultimately overrides: the first declaration of its name.
  fn method_root(&self, function: FnId) -> FnId {
    let mut current = function;

    for _ in 0..=self.typed.classes.len() {
      let Some(decl) = self.resolution.fn_decl(self.package, current).filter(|decl| decl.is_override) else { break };
      let parent = match self.resolution.owner(current) {
        Some(TypeItem::Class(owner)) => self.typed.class_def(owner).and_then(|def| def.parent.as_ref().map(|parent| parent.id)),
        _ => None,
      };

      match parent.and_then(|parent| self.class_method(parent, &decl.name.name)) {
        Some(overridden) if overridden != current => current = overridden,
        _ => break,
      }
    }

    current
  }

  /// Gives a vtable slot to every abstract or overridden method and to its overrides, and builds
  /// the vtable of each class from its parent's. Calls to other methods are direct.
  pub(crate) fn build_vtables(&mut self) {
    let methods: Vec<(ClassId, FnId)> = (0..self.resolution.classes.len())
      .flat_map(|index| {
        let functions = self.resolution.classes.get(index).map(|info| info.functions.clone()).unwrap_or_default();

        functions.into_iter().map(move |function| (ClassId(index as u32), function))
      })
      .filter(|(_, function)| self.resolution.is_method(self.package, *function))
      .collect();
    let mut dynamic = HashSet::new();

    for (_, function) in &methods {
      let Some(decl) = self.resolution.fn_decl(self.package, *function) else { continue };

      if decl.is_abstract {
        dynamic.insert(*function);
      } else if decl.is_override {
        dynamic.insert(self.method_root(*function));
      }
    }

    // Parents come before their subclasses.
    let mut order: Vec<ClassId> = (0..self.typed.classes.len() as u32).map(ClassId).collect();

    order.sort_by_key(|id| self.typed.class_chain(*id).len());

    for id in order {
      let mut vtable = self
        .typed
        .class_def(id)
        .and_then(|def| def.parent.as_ref().map(|parent| parent.id))
        .and_then(|parent| self.typed.class_def(parent))
        .map(|def| def.vtable.clone())
        .unwrap_or_default();

      for (_, function) in methods.iter().filter(|(class, _)| *class == id) {
        let root = self.method_root(*function);

        if !dynamic.contains(&root) {
          continue;
        }

        let slot = if root == *function {
          vtable.push(*function);

          vtable.len() - 1
        } else {
          let Some(slot) = self.typed.slots.get(&root).copied() else { continue };

          if let Some(entry) = vtable.get_mut(slot) {
            *entry = *function;
          }

          slot
        };

        self.typed.slots.insert(*function, slot);
      }

      if let Some(def) = self.typed.classes.get_mut(id.0 as usize) {
        def.vtable = vtable;
      }
    }
  }

  /// `zod.method(args)` or `zod.new(args)`: the parent's implementation, without dynamic
  /// dispatch.
  pub(crate) fn zod_call(&mut self, expr: &Expr, receiver: &Expr, method: &Ident, args: &[Expr]) -> Type {
    let class = match self.owner {
      Some(TypeItem::Class(class)) if self.resolution.paths.contains_key(&receiver.id) => class,
      // Outside a class's methods: already reported.
      _ => {
        self.unchecked_args(args);

        return Type::Error;
      }
    };
    let Some(parent_ref) = self.typed.class_def(class).and_then(|def| def.parent.clone()) else {
      self.diagnostics.push(
        Diagnostic::error(
          codes::INVALID_INHERITANCE,
          receiver.span,
          format!("`zod` refers to the parent class, and `{}` has none", self.class_name(class)),
        )
        .with_help("use `self` to call the methods of this class"),
      );
      self.unchecked_args(args);

      return Type::Error;
    };
    let parent = parent_ref.id;
    let parent_ty = Type::Class(parent_ref);

    self.typed.expr_types.insert(receiver.id, parent_ty.clone());

    if method.name == "new" {
      return self.zod_new(expr, parent_ty, class, args);
    }

    let Some(function) =
      self.class_method(parent, &method.name).filter(|function| self.typed.sig(*function).is_some_and(|sig| sig.receiver.is_some()))
    else {
      self.error(codes::UNKNOWN_MEMBER, method.span, format!("no method `{}` in the parent class `{parent_ty}`", method.name));
      self.unchecked_args(args);

      return Type::Error;
    };

    if let Some(decl) = self.resolution.fn_decl(self.package, function) {
      if decl.is_abstract {
        self.diagnostics.push(
          Diagnostic::error(
            codes::INVALID_OVERRIDE,
            method.span,
            format!("`{}` is abstract in `{parent_ty}`: there is no implementation to call", method.name),
          )
          .with_label(decl.name.span, "declared `abstract` here"),
        );
      } else if decl.visibility == Visibility::Private && self.owner != self.resolution.owner(function) {
        self.diagnostics.push(
          Diagnostic::error(codes::PRIVATE_MEMBER, method.span, format!("the method `{}` of `{parent_ty}` is private", method.name))
            .with_label(decl.name.span, "declared `prv` here"),
        );
      }
    }

    let subst = self.inherited_subst(&parent_ty, function);
    let Some(sig) = self.typed.sig(function).map(|sig| sig.substitute(&subst)) else { return Type::Error };

    self.args(expr.span, "method", &sig.params, args);
    self.typed.method_calls.insert(expr.id, function);
    self.typed.zod_calls.insert(expr.id);
    self.record_owner_args(expr.id, &subst, function);

    sig.ret
  }

  /// `zod.new(args)`, the first statement of the initializer of a class whose ancestor has one.
  fn zod_new(&mut self, expr: &Expr, parent_ty: Type, class: ClassId, args: &[Expr]) -> Type {
    let Some(initializer) = self.typed.inherited_initializer(class) else {
      self.diagnostics.push(
        Diagnostic::error(codes::INVALID_INITIALIZER, expr.span, format!("`{parent_ty}` has no initializer to call"))
          .with_help("remove `zod.new(...)`"),
      );
      self.unchecked_args(args);

      return Type::Unit;
    };

    if !self.zod_new_calls.contains(&expr.id) {
      self.diagnostics.push(
        Diagnostic::error(codes::INVALID_INITIALIZER, expr.span, "`zod.new(...)` can only be the first statement of an initializer")
          .with_note("the parent part of an instance is initialized once, before the subclass's fields"),
      );
    }

    let subst = self.inherited_subst(&parent_ty, initializer);
    let params = self.typed.sig(initializer).map(|sig| sig.substitute(&subst).params).unwrap_or_default();

    self.args(expr.span, "initializer", &params, args);
    self.typed.method_calls.insert(expr.id, initializer);
    self.typed.zod_calls.insert(expr.id);
    self.record_owner_args(expr.id, &subst, initializer);

    Type::Unit
  }
}
