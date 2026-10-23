//! Interfaces (docs/spec/05-interfaces-generics.md, ADR-0031): their parents, fields and methods,
//! the types that implement them, interfaces used as types, and the default methods that the
//! implementing types receive.

use sagitario_ast::{Ident, Path, Visibility};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, EnumId, FnId, InterfaceId, StructId, TypeItem, TypeRes};

use crate::{Checker, FieldDef, FnSig, InterfaceDef, Receiver, Type, TypedModule};

impl Checker<'_> {
  /// Resolves the parents, fields and methods of every interface.
  pub(crate) fn interface_defs(&mut self) {
    for index in 0..self.resolution.interfaces.len() {
      let id = InterfaceId(index as u32);
      let Some(decl) = self.resolution.interface_decl(self.package, id) else { continue };

      self.owner = Some(TypeItem::Interface(id));

      let parents = decl.parents.iter().filter_map(|path| self.interface_path(path)).collect();
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

  /// The interface named after `extends` in an interface, or after `impl`.
  fn interface_path(&mut self, path: &Path) -> Option<InterfaceId> {
    match self.resolution.type_paths.get(&path.id) {
      Some(TypeRes::Item(TypeItem::Interface(id))) => Some(*id),
      Some(TypeRes::Error) | None => None,
      Some(_) => {
        let name = path.segments.last().map_or("", |segment| segment.name.as_str());

        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_IMPL, path.span, format!("`{name}` is not an interface"))
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

        parents.iter().any(|parent| self.typed.interface_ancestors(*parent).contains(&id))
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
      let mentions_self = self.typed.interface_def(id).is_some_and(|def| {
        def.fields.iter().any(|field| mentions_self(&field.ty))
          || def.methods.iter().filter_map(|method| self.typed.sig(*method)).any(|sig| sig.params.iter().chain([&sig.ret]).any(mentions_self))
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
          Diagnostic::error(codes::INVALID_IMPL, span, format!("the interface `{name}` mentions `Self`, so it cannot be used as a type"))
            .with_note("a value of the interface would not know which type `Self` is; use it as a bound of a generic type (F2) instead"),
        );
      }
    }

    self.check_interface_members();
    self.collect_impls();
    self.check_implementations();
  }

  /// The methods of interfaces take `self`, have unique names among their ancestors, and only
  /// interfaces that can be types have default methods in the MVP.
  fn check_interface_members(&mut self) {
    for index in 0..self.typed.interfaces.len() {
      let id = InterfaceId(index as u32);
      let Some(decl) = self.resolution.interface_decl(self.package, id) else { continue };
      let uses_self = self.typed.interface_def(id).is_some_and(|def| def.uses_self);
      let methods = self.typed.interface_def(id).map(|def| def.methods.clone()).unwrap_or_default();

      for (method, function) in methods.iter().zip(&decl.functions) {
        if function.self_param.is_none() {
          self.error(codes::INVALID_IMPL, function.name.span, format!("the interface method `{}` must take `self`", function.name.name));
        } else if !function.is_abstract && uses_self {
          self.error(codes::NOT_IMPLEMENTED, function.name.span, "default methods in interfaces that mention `Self` are not implemented yet");
        }

        // A method reached through two parents is the same method; two with one name are not.
        let parents = self.typed.interface_def(id).map(|def| def.parents.clone()).unwrap_or_default();

        for parent in parents {
          if let Some((owner, inherited)) = self.typed.interface_method(parent, &function.name.name, self.resolution, self.package)
            && inherited != *method
          {
            let owner = self.typed.interface_def(owner).map_or_else(String::new, |def| def.name.clone());

            self.error(codes::INVALID_IMPL, function.name.span, format!("`{}` is already a method of the interface `{owner}`", function.name.name));
          }
        }
      }

      for field in &decl.fields {
        let parents = self.typed.interface_def(id).map(|def| def.parents.clone()).unwrap_or_default();

        if parents.into_iter().any(|parent| self.typed.interface_field(parent, &field.name.name).is_some()) {
          self.error(codes::INVALID_IMPL, field.name.span, format!("`{}` is already a field of a parent interface", field.name.name));
        }
      }
    }
  }

  /// The interfaces each struct, enum and class names after `impl`.
  fn collect_impls(&mut self) {
    let mut found: Vec<(TypeItem, Vec<Path>)> = Vec::new();

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

    for (item, paths) in found {
      let interfaces: Vec<InterfaceId> = paths.iter().filter_map(|path| self.interface_path(path)).collect();

      if !interfaces.is_empty() {
        self.typed.impls.insert(item, interfaces);
      }
    }
  }

  /// Every type implements the fields and methods of its interfaces (Q43, Q44).
  fn check_implementations(&mut self) {
    let mut items: Vec<TypeItem> = Vec::new();

    items.extend((0..self.resolution.structs.len() as u32).map(|index| TypeItem::Struct(StructId(index))));
    items.extend((0..self.resolution.enums.len() as u32).map(|index| TypeItem::Enum(EnumId(index))));
    items.extend((0..self.resolution.classes.len() as u32).map(|index| TypeItem::Class(ClassId(index))));

    for item in items {
      let interfaces = self.typed.implemented(item);

      if interfaces.is_empty() {
        continue;
      }

      // A class checks what it declares itself; what it inherits was checked in its ancestors,
      // except that a concrete class must end up with every required method.
      let own: Vec<InterfaceId> =
        self.typed.impls.get(&item).map(|own| own.iter().flat_map(|id| self.typed.interface_ancestors(*id)).collect()).unwrap_or_default();

      for interface in interfaces {
        if own.contains(&interface) {
          self.check_fields(item, interface);
        }

        self.check_methods(item, interface, own.contains(&interface));
      }
    }
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
  fn check_fields(&mut self, item: TypeItem, interface: InterfaceId) {
    let Some((type_name, span)) = self.type_span(item) else { return };
    let fields = self.typed.interface_def(interface).map(|def| def.fields.clone()).unwrap_or_default();
    let interface_name = self.typed.interface_def(interface).map_or_else(String::new, |def| def.name.clone());
    let self_ty = self.type_ref(item);

    for field in fields {
      let found: Option<FieldDef> = match item {
        TypeItem::Struct(id) => self.typed.struct_def(id).and_then(|def| def.fields.iter().find(|own| own.name == field.name).cloned()),
        TypeItem::Class(id) => self.typed.class_field(id, &field.name).map(|(_, _, own)| own.clone()),
        TypeItem::Enum(_) | TypeItem::Interface(_) => None,
      };
      let expected = substitute_self(&field.ty, &self_ty);
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
  fn check_methods(&mut self, item: TypeItem, interface: InterfaceId, own: bool) {
    let Some((type_name, span)) = self.type_span(item) else { return };
    let methods = self.typed.interface_def(interface).map(|def| def.methods.clone()).unwrap_or_default();
    let interface_name = self.typed.interface_def(interface).map_or_else(String::new, |def| def.name.clone());
    let self_ty = self.type_ref(item);
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
      let expected = self.typed.sig(method).map(|sig| substitute_sig(sig, &self_ty));

      if implementation.visibility == Visibility::Private {
        self.error(codes::INVALID_IMPL, implementation.name.span, format!("`{name}` must be `pub` to implement `{interface_name}`"));
      } else if let Some(expected) = expected
        && self.typed.sig(function).is_some_and(|sig| sig.params != expected.params || sig.ret != expected.ret)
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

  /// `value.name` where `value` has an interface type.
  pub(crate) fn interface_field_type(&mut self, id: InterfaceId, base_ty: &Type, name: &Ident) -> Type {
    match self.typed.interface_field(id, &name.name) {
      Some((_, _, field)) => field.ty.clone(),
      None => {
        let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, name.span, format!("no field `{}` on type `{base_ty}`", name.name));

        if self.typed.interface_method(id, &name.name, self.resolution, self.package).is_some() {
          diagnostic = diagnostic.with_help(format!("`{0}` is a method; call it with `{0}(...)`", name.name));
        }

        self.diagnostics.push(diagnostic);

        Type::Error
      }
    }
  }

  /// Whether `ty` is the prelude's `Error` or implements it.
  pub(crate) fn implements_error(&self, ty: &Type) -> bool {
    let Some((error, _)) = self.typed.error_types else { return false };

    TypedModule::type_item(ty).is_some_and(|item| self.typed.implemented(item).contains(&error))
  }
}

fn mentions_self(ty: &Type) -> bool {
  match ty {
    Type::SelfType => true,
    Type::Array(elem, _) | Type::Option(elem) | Type::Ptr { pointee: elem, .. } => mentions_self(elem),
    Type::Result(value, error) => mentions_self(value) || mentions_self(error),
    _ => false,
  }
}

/// `ty` with `Self` replaced by the implementing type.
fn substitute_self(ty: &Type, self_ty: &Type) -> Type {
  match ty {
    Type::SelfType => self_ty.clone(),
    Type::Array(elem, len) => Type::Array(Box::new(substitute_self(elem, self_ty)), *len),
    Type::Option(value) => Type::Option(Box::new(substitute_self(value, self_ty))),
    Type::Result(value, error) => Type::Result(Box::new(substitute_self(value, self_ty)), Box::new(substitute_self(error, self_ty))),
    Type::Ptr { mutable, pointee } => Type::Ptr { mutable: *mutable, pointee: Box::new(substitute_self(pointee, self_ty)) },
    other => other.clone(),
  }
}

fn substitute_sig(sig: &FnSig, self_ty: &Type) -> FnSig {
  FnSig {
    receiver: sig.receiver.map(|_| Receiver::Value),
    params: sig.params.iter().map(|param| substitute_self(param, self_ty)).collect(),
    ret: substitute_self(&sig.ret, self_ty),
  }
}

/// `fn name(self, A) -> R`, for messages.
fn signature_text(name: &str, sig: &FnSig) -> String {
  let params: Vec<String> = std::iter::once("self".to_string()).chain(sig.params.iter().map(Type::to_string)).collect();
  let ret = if sig.ret == Type::Unit { String::new() } else { format!(" -> {}", sig.ret) };

  format!("fn {name}({}){ret}", params.join(", "))
}
