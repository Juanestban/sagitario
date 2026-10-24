//! Classes (docs/spec/04-classes.md): their fields, the initializer `new`, `new C(...)` and field
//! access, with inherited fields looked up through the ancestors (`inheritance` checks the
//! hierarchy itself). Also which places can be modified (docs/spec/03-memory.md), including the fields of
//! class instances, and which struct and enum methods modify `self` (ADR-0028): a method does if
//! it assigns to `self` or a part of it, or calls such a method on it. Calls to those methods on
//! values that cannot be modified are reported once every method is known.

use sagitario_ast::{Expr, ExprKind, Ident, Path, Stmt, Visibility};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, FnId, LocalId, LocalKind, ParamOwner, TypeItem};

use crate::generic::Unknowns;
use crate::{Checker, ClassDef, FieldDef, PlaceUse, Receiver, Subst, Type};

/// What a place expression belongs to.
enum PlaceBase<'e> {
  /// A local binding.
  Local(LocalId),
  /// A field of a class instance: it can be modified through any reference if it is `var`.
  ClassField { class: ClassId, name: &'e Ident, base: &'e Expr },
  /// A field reached through an interface value: it can be modified if the interface says `var`.
  InterfaceField { interface: sagitario_resolve::InterfaceId, name: &'e Ident },
  /// An element of a list, `whole` or a part of one: only whole elements can be assigned,
  /// since reading an element gives a copy.
  ListElement { whole: bool },
  /// A value that is not stored anywhere.
  Temporary,
  /// A name that is not a binding: already reported.
  Unknown,
}

/// Whether a place can be modified.
enum PlaceAccess {
  Mutable,
  /// A part of the `self` of a struct or enum method, which modifying makes the method modify
  /// `self`.
  OfSelf,
  Immutable(Diagnostic),
}

impl Checker<'_> {
  /// Resolves the parent and the field types of every class.
  pub(crate) fn class_defs(&mut self) {
    for index in 0..self.resolution.classes.len() {
      let id = ClassId(index as u32);
      let Some(decl) = self.resolution.class_decl(self.package, id) else { continue };

      self.owner = Some(TypeItem::Class(id));

      let parent = decl.parent.as_ref().and_then(|path| self.parent_class(path));

      let fields = decl
        .fields
        .iter()
        .map(|field| {
          let ty = self.type_expr(&field.ty);
          let weak = field.weak && self.check_weak_field(field, &ty);

          FieldDef {
            name: field.name.name.clone(),
            ty,
            public: field.visibility != Visibility::Private,
            mutable: field.mutable == Some(true),
            weak,
            span: field.span,
          }
        })
        .collect();
      let initializer = self.resolution.initializer(self.package, id);

      self.typed.classes.push(ClassDef {
        name: decl.name.name.clone(),
        parent,
        is_abstract: decl.is_abstract,
        fields,
        initializer,
        vtable: Vec::new(),
      });
    }

    self.owner = None;
    self.check_inheritance_cycles();
  }

  /// A `weak` field is `var` and holds an `Option` of a class; returns whether it is valid.
  fn check_weak_field(&mut self, field: &sagitario_ast::FieldDecl, ty: &Type) -> bool {
    let problem = match ty {
      _ if field.mutable == Some(false) => Some("a `weak` field must be `var`: it becomes `None` when its instance is destroyed".to_string()),
      Type::Option(inner) if matches!(**inner, Type::Class(_)) => None,
      Type::Error => return false,
      other => Some(format!("a `weak` field holds an `Option` of a class, not `{other}`")),
    };
    let Some(message) = problem else { return true };

    self.diagnostics.push(
      Diagnostic::error(codes::INVALID_WEAK, field.name.span, message)
        .with_help(format!("declare it as `weak var {}: Option<ClassName>;`", field.name.name)),
    );

    false
  }

  /// The initializer of each class: present if the class has fields or its parent's initializer
  /// takes arguments, starting with `zod.new(...)` when an ancestor has one, without a return
  /// type, and never a method. Then the rules of inheritance.
  pub(crate) fn check_classes(&mut self) {
    for index in 0..self.resolution.classes.len() {
      let id = ClassId(index as u32);
      let Some(decl) = self.resolution.class_decl(self.package, id) else { continue };
      let name = &decl.name.name;
      let inherited = self.typed.inherited_initializer(id);
      let own = self.typed.class_def(id).and_then(|def| def.initializer);

      for function in decl.functions.iter().filter(|function| function.name.name == "new") {
        if function.self_param.is_none() {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_INITIALIZER, function.name.span, "the initializer `new` must take `self` first")
              .with_note("`self` is the instance being initialized")
              .with_help("declare it as `pub fn new(self, ...)`"),
          );
        } else if let Some(ret) = &function.return_type {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_INITIALIZER, ret.span, "the initializer `new` has no return type")
              .with_help(format!("remove the return type: `new {name}(...)` returns the new instance")),
          );
        }
      }

      match (own, inherited) {
        (None, _) if !decl.fields.is_empty() => {
          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_INITIALIZER, decl.name.span, format!("the class `{name}` has fields but no initializer"))
              .with_help("declare `pub fn new(self, ...) { ... }` and assign every field in it"),
          );
        }
        (None, Some(inherited)) if self.typed.sig(inherited).is_some_and(|sig| !sig.params.is_empty()) => {
          let parent = self.resolution.owner(inherited).map(|owner| self.resolution.type_name(self.package, owner)).unwrap_or_default();

          self.diagnostics.push(
            Diagnostic::error(codes::INVALID_INITIALIZER, decl.name.span, format!("the class `{name}` must declare an initializer"))
              .with_note(format!("the initializer of `{parent}` takes arguments, which `new {name}()` cannot pass"))
              .with_help("declare `pub fn new(self, ...) { zod.new(...); }`"),
          );
        }
        (Some(own), Some(inherited)) => self.require_zod_new(own, inherited),
        _ => {}
      }
    }

    self.check_members();
    self.check_abstract_methods();
    self.build_vtables();
  }

  /// The initializer of a class whose ancestor has one must start with `zod.new(...)`.
  fn require_zod_new(&mut self, own: FnId, inherited: FnId) {
    let Some(decl) = self.resolution.fn_decl(self.package, own) else { return };
    let first = match decl.body.stmts.first() {
      Some(Stmt::Expr(expr)) => Some(expr),
      Some(_) => None,
      None => decl.body.tail.as_deref(),
    };

    match first {
      Some(expr @ Expr { kind: ExprKind::MethodCall { receiver, method, .. }, .. }) if receiver.kind == ExprKind::Zod && method.name == "new" => {
        self.zod_new_calls.push(expr.id);
      }
      _ => {
        let parent = self.resolution.owner(inherited).map(|owner| self.resolution.type_name(self.package, owner)).unwrap_or_default();

        self.diagnostics.push(
          Diagnostic::error(codes::INVALID_INITIALIZER, decl.name.span, "this initializer must start with `zod.new(...)`")
            .with_note(format!("`{parent}` has an initializer, which runs before the fields of the subclass are assigned"))
            .with_help("make `zod.new(...)` the first statement"),
        );
      }
    }
  }

  /// `new C(args)`: runs the initializer of `C`, if it has one, or else the one it inherits.
  pub(crate) fn new_instance(&mut self, expr: &Expr, path: &Path, id: ClassId, args: &[Expr], expected: Option<&Type>) -> Type {
    let name = self.resolution.type_name(self.package, TypeItem::Class(id));

    if self.typed.class_def(id).is_some_and(|def| def.is_abstract) {
      self.diagnostics.push(
        Diagnostic::error(codes::INVALID_INHERITANCE, path.span, format!("cannot create an instance of the abstract class `{name}`"))
          .with_help(format!("create an instance of a concrete subclass of `{name}`")),
      );
    }

    // The type arguments of a generic class come from `new C<...>`, the expected type and the
    // initializer's arguments.
    let owner = ParamOwner::Item(TypeItem::Class(id));
    let count = self.generic_count(owner);
    let explicit =
      self.explicit_args(path).map(|(_, args)| args).filter(|args| self.check_arg_count(path.span, &format!("`{name}`"), count, args.len()));
    let mut unknowns = Unknowns::new();

    unknowns.add(owner, count, explicit);

    if let Some(expected) = expected {
      unknowns.unify(&self.type_ref(TypeItem::Class(id)), expected);
    }

    let initializer = self.typed.class_def(id).and_then(|def| def.initializer);

    if let Some(initializer) = initializer
      && let Some(decl) = self.resolution.fn_decl(self.package, initializer)
      && decl.visibility == Visibility::Private
      && self.owner != Some(TypeItem::Class(id))
    {
      self.diagnostics.push(
        Diagnostic::error(codes::PRIVATE_MEMBER, path.span, format!("the initializer of `{name}` is private"))
          .with_label(decl.name.span, "declared `prv` here")
          .with_help(format!("create instances with a static function of `{name}`")),
      );
    }

    let params = initializer.and_then(|initializer| self.typed.sig(initializer)).map(|sig| sig.params.clone()).unwrap_or_default();

    self.generic_args(path.span, "initializer", &params, args, &Subst::default(), &mut unknowns);

    let subst = if count == 0 { Subst::default() } else { self.finish_inference(path.span, &format!("`{name}`"), unknowns) };
    let class_args = subst.params.first().map(|(_, args)| args.clone()).unwrap_or_default();

    if let Some(initializer) = initializer {
      self.record_owner_args(expr.id, &subst, initializer);
    }

    self.named_type(TypeItem::Class(id), class_args)
  }

  /// `instance.name`, a field of a class instance or one it inherits.
  pub(crate) fn class_field(&mut self, id: ClassId, base_ty: &Type, name: &Ident) -> Type {
    let field_name = name.name.as_str();
    let Some((declaring, _, field)) = self.typed.class_field(id, field_name).map(|(class, index, field)| (class, index, field.clone())) else {
      let mut diagnostic = Diagnostic::error(codes::UNKNOWN_MEMBER, name.span, format!("no field `{field_name}` on type `{base_ty}`"));
      let fields: Vec<String> = self
        .typed
        .class_chain(id)
        .into_iter()
        .filter_map(|class| self.typed.class_def(class))
        .flat_map(|def| def.fields.iter().map(|field| field.name.clone()))
        .collect();

      if self.class_method(id, field_name).is_some() {
        diagnostic =
          diagnostic.with_help(format!("`{field_name}` is a method; call it with `{field_name}(...)`, or wrap it in a lambda to use it as a value"));
      } else if let Some(similar) = sagitario_diagnostics::closest_match(field_name, fields.iter().map(String::as_str)) {
        diagnostic = diagnostic.with_help(format!("a field with a similar name exists: `{similar}`"));
      }

      self.diagnostics.push(diagnostic);

      return Type::Error;
    };

    // An inherited field is private to the class that declares it.
    if !field.public && self.owner != Some(TypeItem::Class(declaring)) {
      let owner = self.resolution.type_name(self.package, TypeItem::Class(declaring));

      self.diagnostics.push(
        Diagnostic::error(codes::PRIVATE_MEMBER, name.span, format!("the field `{field_name}` of `{owner}` is private"))
          .with_label(field.span, "declared `prv` here"),
      );
    }

    // The field's type with the type arguments of the class declaring it.
    match base_ty {
      Type::Class(class) => self.typed.class_field_type(class, field_name).map_or(field.ty.clone(), |(_, _, ty)| ty),
      _ => field.ty.clone(),
    }
  }

  /// Records that the method being checked modifies `self`.
  pub(crate) fn mark_self_modified(&mut self) {
    if let Some(flag) = self.mutates_self.get_mut(self.function.0 as usize) {
      *flag = true;
    }
  }

  fn place_base<'e>(&self, expr: &'e Expr) -> PlaceBase<'e> {
    match &expr.kind {
      ExprKind::Path(_) => self.local(expr).map_or(PlaceBase::Unknown, PlaceBase::Local),
      ExprKind::Field { base, name } => match self.typed.expr_types.get(&base.id) {
        Some(Type::Class(class)) => PlaceBase::ClassField { class: class.id, name, base },
        Some(Type::Interface(interface)) => PlaceBase::InterfaceField { interface: interface.id, name },
        _ => self.part_of(base),
      },
      ExprKind::Index { base, .. } if matches!(self.typed.expr_types.get(&base.id), Some(Type::List(_))) => PlaceBase::ListElement { whole: true },
      ExprKind::Index { base, .. } => self.part_of(base),
      _ => PlaceBase::Temporary,
    }
  }

  /// The base of a place inside `base`.
  fn part_of<'e>(&self, base: &'e Expr) -> PlaceBase<'e> {
    match self.place_base(base) {
      PlaceBase::ListElement { .. } => PlaceBase::ListElement { whole: false },
      other => other,
    }
  }

  fn is_self(&self, expr: &Expr) -> bool {
    self.local(expr).is_some_and(|id| self.resolution.locals(self.function).get(id.0 as usize).is_some_and(|info| info.kind == LocalKind::SelfParam))
  }

  /// Whether `place` can be modified: it belongs to a `var` binding, to `self`, or to a `var`
  /// field of a class instance (or any field, while the class's initializer assigns `self`).
  fn place_access(&self, place: &Expr, place_use: PlaceUse) -> PlaceAccess {
    let code = match place_use {
      PlaceUse::Assign => codes::IMMUTABLE_PLACE,
      PlaceUse::Receiver(_) => codes::IMMUTABLE_RECEIVER,
    };

    match self.place_base(place) {
      PlaceBase::Unknown => PlaceAccess::Mutable,
      PlaceBase::Temporary => {
        let (code, message) = match place_use {
          PlaceUse::Assign => (codes::INVALID_ASSIGNMENT_TARGET, "cannot assign to a part of a temporary value".to_string()),
          PlaceUse::Receiver(method) => (code, format!("cannot call the method `{method}`, which modifies `self`, on a temporary value")),
        };

        PlaceAccess::Immutable(Diagnostic::error(code, place.span, message).with_help("store the value in a `var` binding first"))
      }
      PlaceBase::ListElement { whole: true } if matches!(place_use, PlaceUse::Assign) => PlaceAccess::Mutable,
      PlaceBase::ListElement { .. } => {
        let (code, message) = match place_use {
          PlaceUse::Assign => (codes::INVALID_ASSIGNMENT_TARGET, "cannot modify a part of a list element".to_string()),
          PlaceUse::Receiver(method) => (code, format!("cannot call the method `{method}`, which modifies `self`, on a list element")),
        };

        PlaceAccess::Immutable(
          Diagnostic::error(code, place.span, message)
            .with_note("reading `list[i]` gives a copy of the element")
            .with_help("modify a copy and store it back: `var item = list[i]; ...; list[i] = item;`"),
        )
      }
      PlaceBase::InterfaceField { interface, name } => {
        let Some((owner, _, field)) = self.typed.interface_field(interface, &name.name) else { return PlaceAccess::Mutable };

        if field.mutable {
          return PlaceAccess::Mutable;
        }

        let owner = self.typed.interface_def(owner).map_or("", |def| def.name.as_str());

        PlaceAccess::Immutable(
          Diagnostic::error(code, place.span, format!("cannot modify the field `{}`: it is a `const` field of the interface `{owner}`", name.name))
            .with_label(field.span, "declared `const` here"),
        )
      }
      PlaceBase::ClassField { class, name, base } => {
        let Some((declaring, _, field)) = self.typed.class_field(class, &name.name) else { return PlaceAccess::Mutable };
        let def = self.typed.class_def(declaring);
        // Only the initializer of the declaring class assigns a `const` field.
        let initializing = def.and_then(|def| def.initializer) == Some(self.function) && self.is_self(base);

        if field.mutable || initializing {
          return PlaceAccess::Mutable;
        }

        let class_name = def.map_or("", |def| def.name.as_str());
        let action = match place_use {
          PlaceUse::Assign => format!("cannot modify the field `{}`", name.name),
          PlaceUse::Receiver(method) => format!("cannot call the method `{method}`, which modifies `self`, on the field `{}`", name.name),
        };

        PlaceAccess::Immutable(
          Diagnostic::error(code, place.span, format!("{action}: it is a `const` field of `{class_name}`"))
            .with_label(field.span, "declared `const` here")
            .with_help(format!("declare it `var {}` to modify it after `new`", name.name)),
        )
      }
      PlaceBase::Local(root) if self.is_captured(root) => {
        let name = self.resolution.locals(self.function).get(root.0 as usize).map(|info| info.name.clone()).unwrap_or_default();

        PlaceAccess::Immutable(self.captured_error(place.span, &name))
      }
      PlaceBase::Local(root) => {
        let Some(info) = self.resolution.locals(self.function).get(root.0 as usize) else { return PlaceAccess::Mutable };
        let name = &info.name;
        let action = match place_use {
          PlaceUse::Assign => format!("cannot modify `{name}`"),
          PlaceUse::Receiver(method) => format!("cannot call the method `{method}`, which modifies `self`, on `{name}`"),
        };
        let copy = format!("copy it into a `var` binding: `var {name} = {name};`");
        let (reason, label, help) = match info.kind {
          LocalKind::Var => return PlaceAccess::Mutable,
          // A class's `self` is a reference: its fields are checked as `ClassField`s.
          LocalKind::SelfParam if matches!(self.owner, Some(TypeItem::Class(_) | TypeItem::Interface(_))) => return PlaceAccess::Mutable,
          LocalKind::SelfParam => return PlaceAccess::OfSelf,
          LocalKind::Const => ("it is not declared with `var`", "declared with `const` here", format!("declare `{name}` with `var` to modify it")),
          LocalKind::Param => ("parameters are immutable", "parameter declared here", copy),
          LocalKind::ForVar => ("loop variables are immutable", "loop variable declared here", copy),
          LocalKind::Pattern => ("pattern bindings are immutable", "bound by the pattern here", copy),
        };

        PlaceAccess::Immutable(Diagnostic::error(code, place.span, format!("{action}: {reason}")).with_label(info.span, label).with_help(help))
      }
    }
  }

  /// A place that is modified right away: an assignment target or the receiver of a built-in
  /// method that writes to it.
  pub(crate) fn check_mutable_place(&mut self, place: &Expr, place_use: PlaceUse) {
    if let Some(pointer) = self.deref_pointer(place) {
      self.check_pointer_write(place, pointer);

      return;
    }

    match self.place_access(place, place_use) {
      PlaceAccess::Mutable => {}
      PlaceAccess::OfSelf => self.mark_self_modified(),
      PlaceAccess::Immutable(diagnostic) => self.diagnostics.push(diagnostic),
    }
  }

  /// The receiver of a struct or enum method, which must be mutable if the method modifies
  /// `self`; that is decided by `infer_mutating_methods`.
  pub(crate) fn receiver_use(&mut self, receiver: &Expr, function: FnId, name: &str) {
    let access = match self.deref_pointer(receiver) {
      Some(pointer) => match self.typed.expr_types.get(&pointer.id) {
        Some(Type::Ptr { mutable: false, .. }) => PlaceAccess::Immutable(
          Diagnostic::error(
            codes::IMMUTABLE_RECEIVER,
            receiver.span,
            format!("cannot call the method `{name}`, which modifies `self`, through a `*const` pointer"),
          )
          .with_help("use a `*mut` pointer, for example with `as *mut T`"),
        ),
        _ => PlaceAccess::Mutable,
      },
      None => self.place_access(receiver, PlaceUse::Receiver(name)),
    };

    match access {
      PlaceAccess::Mutable => {}
      PlaceAccess::OfSelf => self.self_calls.push((self.function, function)),
      PlaceAccess::Immutable(diagnostic) => self.receiver_checks.push((function, diagnostic)),
    }
  }

  /// Decides which struct and enum methods modify `self` (also through the methods they call on
  /// it), makes them receive the caller's value, and reports calls to them on values that
  /// cannot be modified.
  pub(crate) fn infer_mutating_methods(&mut self) {
    let mut mutating = std::mem::take(&mut self.mutates_self);
    let calls = std::mem::take(&mut self.self_calls);
    let modifies = |mutating: &[bool], id: FnId| mutating.get(id.0 as usize).copied().unwrap_or(false);

    loop {
      let mut changed = false;

      for (caller, callee) in &calls {
        if modifies(&mutating, *callee) && !modifies(&mutating, *caller) {
          if let Some(flag) = mutating.get_mut(caller.0 as usize) {
            *flag = true;
          }

          changed = true;
        }
      }

      if !changed {
        break;
      }
    }

    for (sig, mutates) in self.typed.sigs.iter_mut().zip(&mutating) {
      if *mutates && sig.receiver == Some(Receiver::Value) {
        sig.receiver = Some(Receiver::Var);
      }
    }

    for (callee, diagnostic) in std::mem::take(&mut self.receiver_checks) {
      if modifies(&mutating, callee) {
        self.diagnostics.push(diagnostic);
      }
    }
  }
}
