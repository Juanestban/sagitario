//! Interfaces (docs/spec/05-interfaces-generics.md, ADR-0031): their parents, fields and methods,
//! the types that implement them, interfaces used as types, and the default methods that the
//! implementing types receive.

use sagitario_ast::{Ident, TypeExpr, Visibility};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, EnumId, FnId, InterfaceId, ParamOwner, StructId, TypeItem};

use crate::{Checker, FieldDef, FnSig, InterfaceDef, InterfaceRef, Subst, Type};

impl Checker<'_> {
  /// Resolves the parents, fields and methods of every interface.
  pub(crate) fn interface_defs(&mut self) {
    for index in 0..self.resolution.interfaces.len() {
      let id = InterfaceId(index as u32);
      let Some(decl) = self.resolution.interface_decl(self.package, id) else { continue };

      self.owner = Some(TypeItem::Interface(id));

      let parents = decl.parents.iter().filter_map(|ty| self.supertype_interface(ty)).collect();
      let fields = decl
        .fields
        .iter()
        .map(|field| {
          if field.weak {
            self.error(codes::INVALID_WEAK, field.name.span, "only class fields can be `weak`");
          }

          FieldDef {
            name: field.name.name.clone(),
            ty: self.type_expr(&field.ty),
            public: true,
            mutable: field.mutable == Some(true),
            weak: false,
            span: field.span,
          }
        })
        .collect();
      let methods = self.resolution.interfaces.get(index).map(|info| info.functions.clone()).unwrap_or_default();

      self.typed.interfaces.push(InterfaceDef { name: decl.name.name.clone(), parents, fields, methods, uses_self: false });
    }

    self.owner = None;
    self.check_interface_cycles();

    let error = self.resolution.error_interface;
    let message_error = self.resolution.prelude.and_then(|_| self.prelude_class("MessageError"));

    self.typed.error_types = error.zip(message_error);
  }

  /// A class of the prelude module, by name.
  fn prelude_class(&self, name: &str) -> Option<ClassId> {
    let prelude = self.resolution.prelude?;

    (0..self.resolution.classes.len() as u32).map(ClassId).find(|id| {
      let info = self.resolution.classes.get(id.0 as usize);
      let in_prelude = info.is_some_and(|info| self.resolution.modules.get(prelude.0 as usize).and_then(|module| module.source) == Some(info.module));

      in_prelude && self.resolution.class_decl(self.package, *id).is_some_and(|decl| decl.name.name == name)
    })
  }

  /// The interface named after `extends` in an interface, or after `impl`, with its type
  /// arguments.
  fn supertype_interface(&mut self, ty: &TypeExpr) -> Option<InterfaceRef> {
    // Implementing an interface does not use it as a type, even if it mentions `Self`.
    let mark = self.interface_types.len();
    let named = self.type_expr(ty);

    self.interface_types.truncate(mark);

    match named {
      Type::Interface(interface) => Some(interface),
      Type::Error => None,
      other => {
        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_IMPL, ty.span, format!("`{other}` is not an interface"))
            .with_note("`impl` and the `extends` of an interface name interfaces; a class extends its parent class with `extends`"),
        );

        None
      }
    }
  }

  /// Reports interfaces that extend themselves and removes their parents.
  fn check_interface_cycles(&mut self) {
    let cyclic: Vec<usize> = (0..self.typed.interfaces.len())
      .filter(|index| {
        let id = InterfaceId(*index as u32);
        let parents = self.typed.interface_def(id).map(|def| def.parents.clone()).unwrap_or_default();

        parents.iter().any(|parent| self.typed.interface_ancestors(parent.id).contains(&id))
      })
      .collect();

    for index in cyclic {
      let id = InterfaceId(index as u32);

      if let Some(decl) = self.resolution.interface_decl(self.package, id) {
        self.error(codes::INVALID_IMPL, decl.name.span, format!("the interface `{}` extends itself", decl.name.name));
      }

      if let Some(def) = self.typed.interfaces.get_mut(index) {
        def.parents.clear();
      }
    }
  }

  /// Once every signature is known: which interfaces mention `Self`, where interfaces are used,
  /// and whether each type implements its interfaces.
  pub(crate) fn check_interfaces(&mut self) {
    for index in 0..self.typed.interfaces.len() {
      let id = InterfaceId(index as u32);
      // Methods with type parameters cannot be called through a value either: their instances
      // are not known.
      let mentions_self = self.typed.interface_def(id).is_some_and(|def| {
        def.fields.iter().any(|field| field.ty.has_self())
          || def.methods.iter().filter_map(|method| self.typed.sig(*method)).any(|sig| sig.params.iter().chain([&sig.ret]).any(Type::has_self))
          || def.methods.iter().any(|method| self.typed.generics.contains_key(&ParamOwner::Fn(*method)))
      });

      if let Some(def) = self.typed.interfaces.get_mut(index) {
        def.uses_self = mentions_self;
      }
    }

    // An interface whose ancestor mentions `Self` mentions it too.
    for index in 0..self.typed.interfaces.len() {
      let ancestors = self.typed.interface_ancestors(InterfaceId(index as u32));
      let uses_self = ancestors.iter().any(|ancestor| self.typed.interface_def(*ancestor).is_some_and(|def| def.uses_self));

      if let Some(def) = self.typed.interfaces.get_mut(index) {
        def.uses_self = uses_self;
      }
    }

    for (span, id) in std::mem::take(&mut self.interface_types) {
      if self.typed.interface_def(id).is_some_and(|def| def.uses_self) {
        let name = self.typed.interface_def(id).map_or_else(String::new, |def| def.name.clone());

        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_IMPL, span, format!("the interface `{name}` cannot be used as a type"))
            .with_note("it mentions `Self` or has methods with type parameters, which a value of the interface could not resolve")
            .with_help("use it as the bound of a type parameter instead"),
        );
      }
    }

    self.check_interface_members();
    self.collect_impls();
    self.check_implementations();
    self.check_type_bounds();
  }

  /// The methods of interfaces take `self` and have unique names among their ancestors.
  fn check_interface_members(&mut self) {
    for index in 0..self.typed.interfaces.len() {
      let id = InterfaceId(index as u32);
      let Some(decl) = self.resolution.interface_decl(self.package, id) else { continue };
      let methods = self.typed.interface_def(id).map(|def| def.methods.clone()).unwrap_or_default();

      for (method, function) in methods.iter().zip(&decl.functions) {
        if function.self_param.is_none() {
          self.error(codes::INVALID_IMPL, function.name.span, format!("the interface method `{}` must take `self`", function.name.name));
        }

        // A method reached through two parents is the same method; two with one name are not.
        let parents = self.typed.interface_def(id).map(|def| def.parents.clone()).unwrap_or_default();

        for parent in parents {
          if let Some((owner, inherited)) = self.typed.interface_method(parent.id, &function.name.name, self.resolution, self.package)
            && inherited != *method
          {
            let owner = self.typed.interface_def(owner).map_or_else(String::new, |def| def.name.clone());

            self.error(codes::INVALID_IMPL, function.name.span, format!("`{}` is already a method of the interface `{owner}`", function.name.name));
          }
        }
      }

      for field in &decl.fields {
        let parents = self.typed.interface_def(id).map(|def| def.parents.clone()).unwrap_or_default();

        if parents.into_iter().any(|parent| self.typed.interface_field(parent.id, &field.name.name).is_some()) {
          self.error(codes::INVALID_IMPL, field.name.span, format!("`{}` is already a field of a parent interface", field.name.name));
        }
      }
    }
  }

  /// The interfaces each struct, enum and class names after `impl`.
  fn collect_impls(&mut self) {
    let mut found: Vec<(TypeItem, Vec<TypeExpr>)> = Vec::new();

    for index in 0..self.resolution.structs.len() as u32 {
      if let Some(decl) = self.resolution.struct_decl(self.package, StructId(index)) {
        found.push((TypeItem::Struct(StructId(index)), decl.impls.clone()));
      }
    }

    for index in 0..self.resolution.enums.len() as u32 {
      if let Some(decl) = self.resolution.enum_decl(self.package, EnumId(index)) {
        found.push((TypeItem::Enum(EnumId(index)), decl.impls.clone()));
      }
    }

    for index in 0..self.resolution.classes.len() as u32 {
      if let Some(decl) = self.resolution.class_decl(self.package, ClassId(index)) {
        found.push((TypeItem::Class(ClassId(index)), decl.impls.clone()));
      }
    }

    for (item, types) in found {
      self.owner = Some(item);

      let interfaces: Vec<InterfaceRef> = types.iter().filter_map(|ty| self.supertype_interface(ty)).collect();

      if !interfaces.is_empty() {
        self.typed.impls.insert(item, interfaces);
      }
    }

    self.owner = None;
  }

  /// Every type implements the fields and methods of its interfaces (Q43, Q44).
  fn check_implementations(&mut self) {
    let mut items: Vec<TypeItem> = Vec::new();

    items.extend((0..self.resolution.structs.len() as u32).map(|index| TypeItem::Struct(StructId(index))));
    items.extend((0..self.resolution.enums.len() as u32).map(|index| TypeItem::Enum(EnumId(index))));
    items.extend((0..self.resolution.classes.len() as u32).map(|index| TypeItem::Class(ClassId(index))));

    for item in items {
      let item_ty = self.type_ref(item);
      let interfaces = self.typed.implemented_refs(&item_ty);

      if interfaces.is_empty() {
        continue;
      }

      // A class checks what it declares itself; what it inherits was checked in its ancestors,
      // except that a concrete class must end up with every required method.
      let own: Vec<InterfaceRef> = self
        .typed
        .impls
        .get(&item)
        .map(|own| own.iter().flat_map(|interface| self.typed.interface_ancestor_refs(interface)).collect())
        .unwrap_or_default();

      for interface in interfaces {
        if own.contains(&interface) {
          self.check_fields(item, &interface);
        }

        self.check_methods(item, &interface, own.contains(&interface));
      }
    }
  }

  /// Replaces the parameters of an interface (and `Self`) in its members for a type implementing
  /// it.
  fn interface_subst(interface: &InterfaceRef, self_ty: &Type) -> Subst {
    Subst::of(ParamOwner::Item(TypeItem::Interface(interface.id)), interface.args.clone()).with_self(self_ty.clone())
  }

  fn type_span(&self, item: TypeItem) -> Option<(String, sagitario_span::Span)> {
    let name: &Ident = match item {
      TypeItem::Struct(id) => &self.resolution.struct_decl(self.package, id)?.name,
      TypeItem::Enum(id) => &self.resolution.enum_decl(self.package, id)?.name,
      TypeItem::Class(id) => &self.resolution.class_decl(self.package, id)?.name,
      TypeItem::Interface(id) => &self.resolution.interface_decl(self.package, id)?.name,
    };

    Some((name.name.clone(), name.span))
  }

  /// The type has a public field for each field of the interface, of the same type, and `var`
  /// when the interface's is.
  fn check_fields(&mut self, item: TypeItem, interface: &InterfaceRef) {
    let Some((type_name, span)) = self.type_span(item) else { return };
    let fields = self.typed.interface_def(interface.id).map(|def| def.fields.clone()).unwrap_or_default();
    let interface_name = Type::Interface(interface.clone()).to_string();
    let self_ty = self.type_ref(item);
    let subst = Self::interface_subst(interface, &self_ty);

    for field in fields {
      let found: Option<FieldDef> = match (item, &self_ty) {
        (TypeItem::Struct(id), _) => self.typed.struct_def(id).and_then(|def| def.fields.iter().find(|own| own.name == field.name).cloned()),
        (TypeItem::Class(id), Type::Class(class)) => self.typed.class_field(id, &field.name).map(|(_, _, own)| own.clone()).map(|own| {
          let ty = self.typed.class_field_type(class, &field.name).map_or(own.ty.clone(), |(_, _, ty)| ty);

          FieldDef { ty, ..own }
        }),
        _ => None,
      };
      let expected = field.ty.substitute(&subst);
      let problem = match &found {
        None => Some(format!("`{type_name}` has no field `{}`, which the interface `{interface_name}` requires", field.name)),
        Some(own) if !own.public => Some(format!("the field `{}` of `{type_name}` must be `pub` to implement `{interface_name}`", field.name)),
        Some(own) if own.ty != expected => {
          Some(format!("the field `{}` of `{type_name}` must have type `{expected}`, as in `{interface_name}`", field.name))
        }
        Some(own) if field.mutable && !own.mutable => {
          Some(format!("the field `{}` of `{type_name}` must be `var`: the interface `{interface_name}` declares it `var`", field.name))
        }
        Some(own) if own.weak => Some(format!("the field `{}` of `{type_name}` cannot be `weak` to implement `{interface_name}`", field.name)),
        Some(_) => None,
      };

      if let Some(message) = problem {
        let at = found.as_ref().map_or(span, |own| own.span);

        self.diagnostics.push(Diagnostic::error(codes::INVALID_IMPL, at, message).with_label(field.span, "required here"));
      }
    }
  }

  /// The type's implementation of each method of the interface: public, with the same signature,
  /// or the interface's default. `own` is set when the type names the interface itself.
  fn check_methods(&mut self, item: TypeItem, interface: &InterfaceRef, own: bool) {
    let Some((type_name, span)) = self.type_span(item) else { return };
    let methods = self.typed.interface_def(interface.id).map(|def| def.methods.clone()).unwrap_or_default();
    let interface_name = Type::Interface(interface.clone()).to_string();
    let self_ty = self.type_ref(item);
    let subst = Self::interface_subst(interface, &self_ty);
    let is_abstract = matches!(item, TypeItem::Class(id) if self.typed.class_def(id).is_some_and(|def| def.is_abstract));

    for method in methods {
      let Some(decl) = self.resolution.fn_decl(self.package, method) else { continue };
      let name = decl.name.name.clone();
      let found = match item {
        TypeItem::Class(id) => self.class_method(id, &name),
        _ => self.resolution.type_function(self.package, item, &name),
      };
      let Some(function) = found.filter(|function| self.resolution.fn_decl(self.package, *function).is_some_and(|decl| decl.self_param.is_some()))
      else {
        if decl.is_abstract && !is_abstract {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_IMPL, span, format!("`{type_name}` does not implement the method `{name}` of `{interface_name}`"))
              .with_label(decl.name.span, "required here")
              .with_help(format!("add `pub fn {name}(...)`")),
          );
        }

        continue;
      };

      // Methods a class inherits were checked in the class declaring them.
      if !own && !matches!(item, TypeItem::Class(_)) {
        continue;
      }

      let Some(implementation) = self.resolution.fn_decl(self.package, function) else { continue };
      // The method's own type parameters correspond to the implementation's.
      let own_args = self.identity_args(ParamOwner::Fn(function));
      let expected = self.typed.sig(method).map(|sig| sig.substitute(&subst.clone().with(ParamOwner::Fn(method), own_args.clone())));
      let actual = self.typed.sig(function).map(|sig| sig.substitute(&self.inherited_subst(&self_ty, function)));
      let same_generics = self.generic_count(ParamOwner::Fn(method)) == own_args.len();

      if implementation.visibility == Visibility::Private {
        self.error(codes::INVALID_IMPL, implementation.name.span, format!("`{name}` must be `pub` to implement `{interface_name}`"));
      } else if let (Some(expected), Some(actual)) = (expected, actual)
        && (!same_generics || actual.params != expected.params || actual.ret != expected.ret)
      {
        self.diagnostics.push(
          Diagnostic::error(
            codes::INVALID_IMPL,
            implementation.name.span,
            format!("`{name}` must have the signature declared in `{interface_name}`"),
          )
          .with_label(decl.name.span, "declared here")
          .with_note(format!("expected `{}`", signature_text(&name, &expected))),
        );
      }
    }
  }

  /// A default method of an interface that a struct, enum or class implements.
  pub(crate) fn default_method(&self, owner: TypeItem, name: &str) -> Option<FnId> {
    if matches!(owner, TypeItem::Interface(_)) {
      return None;
    }

    self.typed.implemented(owner).into_iter().find_map(|interface| {
      let methods = self.typed.interface_def(interface)?.methods.clone();

      methods
        .into_iter()
        .find(|method| self.resolution.fn_decl(self.package, *method).is_some_and(|decl| decl.name.name == name && !decl.is_abstract))
    })
  }

  /// The substitution giving the type parameters of the type declaring `function` (one of the
  /// ancestors of `self_ty`, or an interface it implements) their arguments in `self_ty`.
  pub(crate) fn inherited_subst(&self, self_ty: &Type, function: FnId) -> Subst {
    match (self.resolution.owner(function), self_ty) {
      (Some(TypeItem::Class(owner)), Type::Class(class)) => {
        let args = self.typed.class_ancestor(class, owner).map(|ancestor| ancestor.args).unwrap_or_default();

        Subst::of(ParamOwner::Item(TypeItem::Class(owner)), args)
      }
      (Some(TypeItem::Interface(owner)), _) => {
        let args = self.typed.implemented_ref(self_ty, owner).map(|interface| interface.args).unwrap_or_default();

        Subst::of(ParamOwner::Item(TypeItem::Interface(owner)), args).with_self(self_ty.clone())
      }
      (
        Some(owner @ (TypeItem::Struct(_) | TypeItem::Enum(_))),
        Type::Struct(crate::StructRef { args, .. }) | Type::Enum(crate::EnumRef { args, .. }),
      ) => Subst::of(ParamOwner::Item(owner), args.clone()),
      _ => Subst::default(),
    }
  }

  /// `value.name` where `value` has an interface type or is a type parameter: a field of one of
  /// its interfaces.
  pub(crate) fn interface_field_type(&mut self, base_ty: &Type, name: &Ident) -> Type {
    for interface in self.typed.implemented_refs(base_ty) {
      let field = self.typed.interface_def(interface.id).and_then(|def| def.fields.iter().find(|field| field.name == name.name).cloned());

      if let Some(field) = field {
        return field.ty.substitute(&Self::interface_subst(&interface, base_ty));
      }
    }

    let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, name.span, format!("no field `{}` on type `{base_ty}`", name.name));

    if self.interface_method_of(base_ty, &name.name).is_some() {
      diagnostic =
        diagnostic.with_help(format!("`{0}` is a method; call it with `{0}(...)`, or wrap it in a lambda to use it as a value", name.name));
    }

    self.diagnostics.push(diagnostic);

    Type::Error
  }

  /// A method of the interfaces of an interface type or a type parameter.
  pub(crate) fn interface_method_of(&self, ty: &Type, name: &str) -> Option<FnId> {
    self.typed.implemented_refs(ty).into_iter().find_map(|interface| {
      let methods = self.typed.interface_def(interface.id)?.methods.clone();

      methods.into_iter().find(|method| self.resolution.fn_decl(self.package, *method).is_some_and(|decl| decl.name.name == name))
    })
  }

  /// Whether `ty` is the prelude's `Error` or implements it.
  pub(crate) fn implements_error(&self, ty: &Type) -> bool {
    let Some((error, _)) = self.typed.error_types else { return false };
    let name = self.typed.interface_def(error).map(|def| def.name.clone()).unwrap_or_default();

    self.typed.implements(ty, &InterfaceRef { id: error, name, args: Vec::new() })
  }
}

/// `fn name(self, A) -> R`, for messages.
fn signature_text(name: &str, sig: &FnSig) -> String {
  let params: Vec<String> = std::iter::once("self".to_string()).chain(sig.params.iter().map(Type::to_string)).collect();
  let ret = if sig.ret == Type::Unit { String::new() } else { format!(" -> {}", sig.ret) };

  format!("fn {name}({}){ret}", params.join(", "))
}
