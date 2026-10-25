//! `Send` (docs/spec/05-interfaces-generics.md#send, ADR-0045): the types whose values can
//! cross to another thread, which the compiler infers, and the check of `thread::spawn`, whose
//! lambda may only capture such values.

use sagitario_ast::{Expr, ExprKind};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, FnId, InterfaceId, ItemRef, ParamOwner, Res, TypeItem};
use sagitario_span::Span;

use crate::{Checker, ClassRef, Subst, Type, TypedModule};

impl TypedModule {
  /// Why values of `ty` cannot be sent to another thread, or `None` if `ty` is `Send`.
  pub fn not_send_reason(&self, ty: &Type) -> Option<String> {
    self.not_send(ty, &mut Vec::new())
  }

  /// `visiting` holds the types being checked: a type reached again through itself (a class
  /// holding an `Option` of itself) adds no new reason.
  fn not_send(&self, ty: &Type, visiting: &mut Vec<Type>) -> Option<String> {
    if visiting.contains(ty) {
      return None;
    }

    match ty {
      Type::Unit | Type::Bool | Type::Char | Type::Int(_) | Type::Float(_) | Type::String | Type::CString | Type::Never | Type::Error => None,
      Type::StringBuilder => Some("a `StringBuilder` can be changed through any reference to it".to_string()),
      Type::List(_) => Some(format!("a `{ty}` can be changed through any reference to it")),
      Type::Function { .. } => Some("a function value can capture values that are not `Send`".to_string()),
      Type::Ptr { .. } | Type::CVoid => Some("raw pointers are not `Send`".to_string()),
      Type::Interface(interface) if self.prelude.send == Some(interface.id) => None,
      Type::Interface(interface) => Some(format!("a value of the interface `{}` can hold a type that is not `Send`", interface.name)),
      Type::Param(param) => {
        let bounded = self.param_bounds(param).iter().any(|bound| Some(bound.id) == self.prelude.send);

        (!bounded).then(|| format!("the type parameter `{}` is not bounded by `Send`", param.name))
      }
      Type::Array(elem, _) | Type::Option(elem) => self.not_send(elem, visiting),
      Type::Result(value, error) => self.not_send(value, visiting).or_else(|| self.not_send(error, visiting)),
      Type::Struct(inner) => {
        visiting.push(ty.clone());

        let reason = self.struct_field_types(inner).iter().find_map(|field| self.not_send(field, visiting));

        visiting.pop();
        reason
      }
      Type::Enum(inner) => {
        visiting.push(ty.clone());

        let subst = Subst::of(ParamOwner::Item(TypeItem::Enum(inner.id)), inner.args.clone());
        let fields: Vec<Type> = self
          .enums
          .get(inner.id.0 as usize)
          .map(|def| def.variants.iter().flat_map(|variant| variant.field_types()).map(|field| field.substitute(&subst)).collect())
          .unwrap_or_default();
        let reason = fields.iter().find_map(|field| self.not_send(field, visiting));

        visiting.pop();
        reason
      }
      Type::Class(inner) => {
        visiting.push(ty.clone());

        let reason = self.class_not_send(inner, visiting);

        visiting.pop();
        reason
      }
    }
  }

  /// A class is `Send` when every field, its own and inherited, is `const` and `Send`, and so is
  /// every subclass: a reference to the class may refer to an instance of any of them.
  fn class_not_send(&self, class: &ClassRef, visiting: &mut Vec<Type>) -> Option<String> {
    for ancestor in self.class_ancestors(class) {
      if let Some(reason) = self.own_fields_not_send(&ancestor, visiting) {
        return Some(reason);
      }
    }

    let subclasses: Vec<ClassId> =
      (0..self.classes.len() as u32).map(ClassId).filter(|id| *id != class.id && self.is_subclass(*id, class.id)).collect();

    for id in subclasses {
      let name = self.class_def(id).map(|def| def.name.clone()).unwrap_or_default();
      let subclass = ClassRef { id, name: name.clone(), args: self.identity_args(ParamOwner::Item(TypeItem::Class(id))) };

      if let Some(reason) = self.own_fields_not_send(&subclass, visiting) {
        return Some(format!("its subclass `{name}` is not `Send`: {reason}"));
      }
    }

    None
  }

  fn own_fields_not_send(&self, class: &ClassRef, visiting: &mut Vec<Type>) -> Option<String> {
    let def = self.class_def(class.id)?;
    let types = self.class_field_types(class);

    for (field, ty) in def.fields.iter().zip(&types) {
      if field.weak {
        return Some(format!("the field `{}` of `{}` is `weak`", field.name, def.name));
      }

      if field.mutable {
        return Some(format!("the field `{}` of `{}` is `var`", field.name, def.name));
      }

      if let Some(reason) = self.not_send(ty, visiting) {
        return Some(reason);
      }
    }

    None
  }
}

impl Checker<'_> {
  /// The prelude's `Send` interface.
  pub(crate) fn is_send_interface(&self, id: InterfaceId) -> bool {
    self.resolution.prelude_type("Send") == Some(TypeItem::Interface(id))
  }

  pub(crate) fn send_used_as_type(&mut self, span: Span) {
    self.diagnostics.push(
      Diagnostic::error(codes::INVALID_IMPL, span, "`Send` cannot be used as a type").with_help("use it as the bound of a type parameter: `T: Send`"),
    );
  }

  /// `Send` written as a type inside function bodies, which are checked after the interfaces.
  pub(crate) fn check_send_types(&mut self) {
    for (span, id) in std::mem::take(&mut self.interface_types) {
      if self.is_send_interface(id) {
        self.send_used_as_type(span);
      }
    }
  }

  /// Whether `id` starts a thread: `std::thread::spawn`, or `JoinHandle::spawn`, which it calls.
  /// Both are checked where code outside the standard library calls them.
  pub(crate) fn is_spawn(&self, id: FnId) -> bool {
    let Some(module) = self.resolution.modules.iter().find(|module| module.in_std && module.path == ["std", "thread"]) else { return false };
    let spawn = match module.items.get("spawn").map(|declared| declared.item) {
      Some(ItemRef::Fn(spawn)) => Some(spawn),
      _ => None,
    };
    let handle_spawn = match module.items.get("JoinHandle").map(|declared| declared.item) {
      Some(ItemRef::Type(TypeItem::Class(class))) => self.resolution.classes.get(class.0 as usize).and_then(|info| {
        info.functions.iter().copied().find(|function| self.resolution.fn_decl(self.package, *function).is_some_and(|decl| decl.name.name == "spawn"))
      }),
      _ => None,
    };

    (spawn == Some(id) || handle_spawn == Some(id)) && !self.in_std()
  }

  /// Whether the function being checked belongs to the standard library.
  fn in_std(&self) -> bool {
    self.resolution.functions.get(self.function.0 as usize).is_some_and(|info| self.resolution.is_std_source(info.module))
  }

  /// The argument of `thread::spawn` is a lambda whose captures are all `Send`, or a function
  /// named by its path, which captures nothing.
  pub(crate) fn check_spawn_argument(&mut self, args: &[Expr]) {
    let Some(arg) = args.first() else { return };

    match &arg.kind {
      ExprKind::Lambda { .. } => {
        let captures = self.resolution.lambdas.get(&arg.id).map(|info| info.captures.clone()).unwrap_or_default();

        for local in captures {
          let Some(ty) = self.locals.get(local.0 as usize).cloned().flatten() else { continue };
          let Some(reason) = self.typed.not_send_reason(&ty) else { continue };
          let name = self.resolution.locals(self.function).get(local.0 as usize).map(|info| info.name.clone()).unwrap_or_default();

          self.diagnostics.push(
            Diagnostic::error(codes::NOT_SEND, arg.span, format!("this lambda captures `{name}`, of type `{ty}`, which is not `Send`"))
              .with_note(reason)
              .with_note("a thread may only capture values that no other thread can change: `Send` values")
              .with_help("pass the thread a `Send` copy of what it needs, or build the value inside the thread"),
          );
        }
      }
      ExprKind::Path(_) if matches!(self.resolution.paths.get(&arg.id), Some(Res::Fn(_))) => {}
      _ => self.diagnostics.push(
        Diagnostic::error(codes::INVALID_SPAWN, arg.span, "`thread::spawn` takes a lambda or the name of a function")
          .with_note("the compiler checks that what the thread captures is `Send`, which it cannot do for other function values"),
      ),
    }
  }
}
