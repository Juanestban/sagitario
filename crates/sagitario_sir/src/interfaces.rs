//! Interface values (docs/spec/05-interfaces-generics.md#interfaces-as-types, ADR-0031): a value
//! of an interface type is a reference to an object (a class instance, or a box holding a copy of
//! a struct or enum) paired with the itable of the interface for the object's type. Converting a
//! value to an interface picks that itable; method calls and field accesses through the
//! interface call the itable's entries.
//!
//! Calls of interface methods on values of a known type (a type parameter replaced by a
//! concrete type, or a default method called directly) go straight to the type's method, or to
//! the default method instantiated for that type (ADR-0032).

use sagitario_ast::NodeId;
use sagitario_diagnostics::Diagnostic;
use sagitario_resolve::{FnId, InterfaceId, ParamOwner, TypeItem};
use sagitario_types::{ClassRef, InterfaceRef, Receiver, Subst, Type, TypedModule};

use crate::mono::{ClassInstance, Instance};
use crate::{Callee, FnLowering, LResult, Operand, Rvalue, Stmt, error};

/// The methods and fields of one interface for one type.
#[derive(Debug, Clone, PartialEq)]
pub struct Itable {
  pub interface: InterfaceRef,
  /// The class, struct or enum whose values the itable describes.
  pub concrete: Type,
  /// The itables of the interface's parents for the same type, in `extends` order.
  pub parents: Vec<usize>,
  /// The interface's own methods in declaration order, then a getter and a setter for each of
  /// its own fields.
  pub entries: Vec<ItableEntry>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ItableEntry {
  /// Calls `function` (a method of the type, or a default method instantiated for it) on the
  /// value, through the instance's vtable when `slot` is set. The entry receives the interface
  /// value followed by arguments of types `params`.
  Method { function: FnId, slot: Option<usize>, params: Vec<Type>, ret: Type },
  /// Reads (`set: false`) or writes a field of the value.
  Field { field: FieldAt, ty: Type, set: bool },
}

/// Where a field lives in a value implementing an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldAt {
  /// The field at `index` among those of class instance `class` (the class declaring it).
  Class { class: usize, index: usize },
  /// The field at `index` of a boxed struct.
  Struct(usize),
}

/// What lowering builds for the whole program: itables, boxes, instances of templates and class
/// instances.
#[derive(Debug, Default)]
pub(crate) struct Registry {
  pub(crate) itables: Vec<Itable>,
  /// The struct and enum types copied into boxes to become interface values.
  pub(crate) boxes: Vec<Type>,
  /// The number of the program's functions; instance `N` is the function `first_instance + N`.
  pub(crate) first_instance: u32,
  pub(crate) instances: Vec<Instance>,
  /// Instances not lowered yet.
  pub(crate) pending: Vec<usize>,
  pub(crate) classes: Vec<ClassInstance>,
  /// Types whose class instances are registered.
  pub(crate) seen: std::collections::HashSet<Type>,
  /// Errors found while instantiating.
  pub(crate) diagnostics: Vec<Diagnostic>,
  /// Whether an endless series of instances was reported.
  pub(crate) endless: bool,
}

/// The slot of the getter of a field, after the interface's methods; the setter follows it.
pub(crate) fn field_slot(typed: &TypedModule, interface: InterfaceId, index: usize) -> usize {
  typed.interface_def(interface).map_or(0, |def| def.methods.len()) + 2 * index
}

impl FnLowering<'_> {
  /// The substitution of an interface's parameters and `Self` for one implementing type.
  fn interface_subst(interface: &InterfaceRef, concrete: &Type) -> Subst {
    Subst::of(ParamOwner::Item(TypeItem::Interface(interface.id)), interface.args.clone()).with_self(concrete.clone())
  }

  /// The index of the itable of `interface` for values of `concrete`, built if needed.
  pub(crate) fn itable(&mut self, interface: &InterfaceRef, concrete: &Type) -> usize {
    if let Some(index) = self.registry.itables.iter().position(|itable| itable.interface == *interface && itable.concrete == *concrete) {
      return index;
    }

    if matches!(concrete, Type::Struct(_) | Type::Enum(_)) && !self.registry.boxes.contains(concrete) {
      self.registry.boxes.push(concrete.clone());
    }

    self.register_types(concrete);

    let index = self.registry.itables.len();
    let def = self.typed.interface_def(interface.id).cloned();

    // Reserved first, so cycles through parents cannot recurse forever.
    self.registry.itables.push(Itable { interface: interface.clone(), concrete: concrete.clone(), parents: Vec::new(), entries: Vec::new() });

    let Some(def) = def else { return index };
    let subst = Self::interface_subst(interface, concrete);
    let mut entries = Vec::new();

    for method in &def.methods {
      let Some(sig) = self.typed.sig(*method).map(|sig| sig.substitute(&subst)) else { continue };
      let (function, slot) = self.method_for(*method, concrete, &subst, Vec::new());

      entries.push(ItableEntry::Method { function, slot, params: sig.params, ret: sig.ret });
    }

    for field in &def.fields {
      let at = match concrete {
        Type::Class(class) => self
          .typed
          .class_field_type(class, &field.name)
          .map(|(declaring, index, _)| FieldAt::Class { class: self.class_instance(&declaring), index }),
        Type::Struct(owner) => self.typed.field_index(owner.id, &field.name).map(FieldAt::Struct),
        _ => None,
      };
      let ty = field.ty.substitute(&subst);

      if let Some(at) = at {
        entries.push(ItableEntry::Field { field: at, ty: ty.clone(), set: false });
        entries.push(ItableEntry::Field { field: at, ty, set: true });
      }
    }

    let parents = self
      .typed
      .interface_ancestor_refs(interface)
      .into_iter()
      .filter(|ancestor| def.parents.iter().any(|parent| parent.id == ancestor.id))
      .collect::<Vec<_>>();
    let parents = def
      .parents
      .iter()
      .filter_map(|parent| parents.iter().find(|ancestor| ancestor.id == parent.id).cloned())
      .map(|parent| self.itable(&parent, concrete))
      .collect();

    if let Some(itable) = self.registry.itables.get_mut(index) {
      itable.parents = parents;
      itable.entries = entries;
    }

    index
  }

  /// The function implementing the interface method `method` for values of `concrete`: the
  /// type's own (or inherited) method, or else the default method instantiated for the type,
  /// with its vtable slot when the type's method is dispatched dynamically. `own` are the type
  /// arguments of the method's own parameters.
  pub(crate) fn method_for(&mut self, method: FnId, concrete: &Type, subst: &Subst, own: Vec<Type>) -> (FnId, Option<usize>) {
    let name = self.resolution.fn_decl(self.package, method).map(|decl| decl.name.name.clone()).unwrap_or_default();

    match self.implementation(concrete, &name) {
      Some(function) => {
        let owner_subst = self.owner_subst(concrete, function);
        let subst =
          if self.typed.generics.contains_key(&ParamOwner::Fn(function)) { owner_subst.with(ParamOwner::Fn(function), own) } else { owner_subst };
        let slot = if matches!(concrete, Type::Class(_)) { self.typed.slots.get(&function).copied() } else { None };

        (self.instance(function, subst), slot)
      }
      None => {
        let subst =
          if self.typed.generics.contains_key(&ParamOwner::Fn(method)) { subst.clone().with(ParamOwner::Fn(method), own) } else { subst.clone() };

        (self.instance(method, subst), None)
      }
    }
  }

  /// The type arguments that the type declaring `function` (`concrete` or one of its ancestors)
  /// receives in `concrete`.
  fn owner_subst(&self, concrete: &Type, function: FnId) -> Subst {
    match (self.resolution.owner(function), concrete) {
      (Some(TypeItem::Class(owner)), Type::Class(class)) => {
        let args = self.typed.class_ancestor(class, owner).map(|ancestor| ancestor.args).unwrap_or_default();

        Subst::of(ParamOwner::Item(TypeItem::Class(owner)), args)
      }
      (Some(owner), Type::Struct(sagitario_types::StructRef { args, .. }) | Type::Enum(sagitario_types::EnumRef { args, .. })) => {
        Subst::of(ParamOwner::Item(owner), args.clone())
      }
      _ => Subst::default(),
    }
  }

  /// The method of a concrete type with this name, inherited ones included.
  fn implementation(&self, concrete: &Type, name: &str) -> Option<FnId> {
    let owners: Vec<TypeItem> = match concrete {
      Type::Class(class) => self.typed.class_chain(class.id).into_iter().map(TypeItem::Class).collect(),
      other => TypedModule::type_item(other).into_iter().collect(),
    };

    owners.into_iter().find_map(|owner| {
      self
        .resolution
        .type_function(self.package, owner, name)
        .filter(|function| self.resolution.fn_decl(self.package, *function).is_some_and(|decl| decl.self_param.is_some()))
    })
  }

  /// The call of the interface method `function` on a value of a concrete type: its
  /// implementation (through the vtable when it varies) or the default instantiated for the
  /// type. Returns the callee and whether it receives a struct by reference.
  pub(crate) fn concrete_interface_call(&mut self, id: NodeId, function: FnId, concrete: &Type) -> (Callee, bool) {
    let subst = self.call_subst(id, function, Some(concrete));
    let own = subst.params.iter().find(|(owner, _)| *owner == ParamOwner::Fn(function)).map(|(_, args)| args.clone()).unwrap_or_default();
    let (target, slot) = self.method_for(function, concrete, &subst, own);
    let by_ref = self.typed.sig(self.template_of(target)).is_some_and(|sig| sig.receiver == Some(Receiver::Var));

    match slot {
      Some(slot) => (Callee::Virtual { slot, function: target }, false),
      None => (Callee::Fn(target), by_ref),
    }
  }

  /// The template an instance comes from, or the function itself.
  pub(crate) fn template_of(&self, function: FnId) -> FnId {
    let index = function.0.checked_sub(self.registry.first_instance).map(|index| index as usize);

    match index.and_then(|index| self.registry.instances.get(index)) {
      Some(instance) => instance.template,
      None => function,
    }
  }

  /// `value` of type `from` as a value of type `to`: converted when `to` is an interface and
  /// `from` is a type implementing it or a descendant interface.
  pub(crate) fn convert(&mut self, value: Operand, from: &Type, to: &Type) -> Operand {
    let Type::Interface(target) = to else { return value };

    match from {
      Type::Interface(source) if source.id == target.id => value,
      Type::Interface(source) => match self.typed.interface_path(source.id, target.id) {
        Some(path) => self.new_value(to.clone(), Rvalue::DynUpcast { value, path }),
        None => value,
      },
      Type::Class(_) | Type::Struct(_) | Type::Enum(_) => {
        let itable = self.itable(target, from);

        self.new_value(to.clone(), Rvalue::ToDyn { value, itable })
      }
      _ => value,
    }
  }

  /// The call of the interface method `function` on a value of interface type `receiver`.
  pub(crate) fn dyn_method(&self, receiver: &InterfaceRef, function: FnId) -> LResult<Callee> {
    let Some(TypeItem::Interface(owner)) = self.resolution.owner(function) else {
      return error("dynamic call of a method outside an interface");
    };
    let path = self.typed.interface_path(receiver.id, owner).unwrap_or_default();
    let slot = self.typed.interface_def(owner).and_then(|def| def.methods.iter().position(|method| *method == function));
    let declaring = self.typed.implemented_ref(&Type::Interface(receiver.clone()), owner).unwrap_or_else(|| receiver.clone());
    let subst = Self::interface_subst(&declaring, &Type::Interface(receiver.clone()));
    let sig = self.typed.sig(function).map(|sig| sig.substitute(&subst));
    let (Some(slot), Some(sig)) = (slot, sig) else {
      return error("interface method without a slot");
    };

    Ok(Callee::Dyn { path, slot, params: sig.params, ret: sig.ret })
  }

  /// The getter (`set: false`) or setter of a field reached through interface `receiver`.
  pub(crate) fn dyn_field(&self, receiver: &InterfaceRef, name: &str, set: bool) -> LResult<Callee> {
    let Some((owner, index, field)) = self.typed.interface_field(receiver.id, name) else {
      return error(format!("unknown interface field `{name}`"));
    };
    let path = self.typed.interface_path(receiver.id, owner).unwrap_or_default();
    let slot = field_slot(self.typed, owner, index) + usize::from(set);
    let declaring = self.typed.implemented_ref(&Type::Interface(receiver.clone()), owner).unwrap_or_else(|| receiver.clone());
    let ty = field.ty.substitute(&Self::interface_subst(&declaring, &Type::Interface(receiver.clone())));
    let (params, ret) = if set { (vec![ty], Type::Unit) } else { (Vec::new(), ty) };

    Ok(Callee::Dyn { path, slot, params, ret })
  }

  /// `Error(text)`: a `MessageError` holding `text`, as an `Error`.
  pub(crate) fn message_error(&mut self, text: Operand) -> LResult<Operand> {
    let Some((error, class)) = self.typed.error_types else {
      return error("`Error(...)` without the prelude");
    };
    let name = self.typed.class_def(class).map(|def| def.name.clone()).unwrap_or_default();
    let class_ref = ClassRef { id: class, name, args: Vec::new() };
    let class_ty = Type::Class(class_ref.clone());
    let instance_index = self.class_instance(&class_ref);
    let instance = self.new_value(class_ty.clone(), Rvalue::Alloc(instance_index));

    if let Some(initializer) = self.typed.class_def(class).and_then(|def| def.initializer) {
      self.call(Callee::Fn(initializer), vec![instance.clone(), text], Type::Unit);
    }

    let interface_name = self.typed.interface_def(error).map(|def| def.name.clone()).unwrap_or_default();
    let to = Type::Interface(InterfaceRef { id: error, name: interface_name, args: Vec::new() });

    Ok(self.convert(instance, &class_ty, &to))
  }

  /// An error value of type `from` for a function returning errors of type `to`: converted to
  /// an interface, or, from a `String`, made an `Error` (`?`, docs/spec/07-errors.md).
  pub(crate) fn convert_error(&mut self, value: Operand, from: &Type, to: &Type) -> LResult<Operand> {
    let into_error =
      *from == Type::String && matches!(to, Type::Interface(target) if self.typed.error_types.is_some_and(|(error, _)| error == target.id));

    if into_error {
      return self.message_error(value);
    }

    Ok(self.convert(value, from, to))
  }

  /// `value.name = new` where `value` has an interface type: calls the field's setter.
  pub(crate) fn set_dyn_field(&mut self, receiver: Operand, interface: &InterfaceRef, name: &str, value: Operand) -> LResult<()> {
    let callee = self.dyn_field(interface, name, true)?;

    self.emit(Stmt::Eval(Rvalue::Call { callee, args: vec![receiver, value] }));

    Ok(())
  }
}
