//! Interface values (docs/spec/05-interfaces-generics.md#interfaces-as-types, ADR-0031): a value
//! of an interface type is a reference to an object (a class instance, or a box holding a copy of
//! a struct or enum) paired with the itable of the interface for the object's type. Converting a
//! value to an interface picks that itable; method calls and field accesses through the
//! interface call the itable's entries.

use sagitario_resolve::{ClassId, FnId, InterfaceId, TypeItem};
use sagitario_types::{ClassRef, InterfaceRef, Type, TypedModule};

use crate::{Callee, FnLowering, LResult, Operand, Rvalue, Stmt, error};

/// The methods and fields of one interface for one type.
#[derive(Debug, Clone, PartialEq)]
pub struct Itable {
  pub interface: InterfaceId,
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
  /// The interface method `signature`, implemented by the type's `function`; through the
  /// instance's vtable when `slot` is set.
  Method { signature: FnId, function: FnId, slot: Option<usize> },
  /// A default method of the interface, which receives the interface value itself.
  Default(FnId),
  /// Reads (`set: false`) or writes a field of the value.
  Field { field: FieldAt, ty: Type, set: bool },
}

/// Where a field lives in a value implementing an interface.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FieldAt {
  /// The field at `index` among those `class` declares.
  Class { class: ClassId, index: usize },
  /// The field at `index` of a boxed struct.
  Struct(usize),
}

/// The itables and boxes the program uses, built while lowering.
#[derive(Debug, Default)]
pub(crate) struct Registry {
  pub(crate) itables: Vec<Itable>,
  /// The struct and enum types copied into boxes to become interface values.
  pub(crate) boxes: Vec<Type>,
}

/// The slot of the getter of a field, after the interface's methods; the setter follows it.
pub(crate) fn field_slot(typed: &TypedModule, interface: InterfaceId, index: usize) -> usize {
  typed.interface_def(interface).map_or(0, |def| def.methods.len()) + 2 * index
}

impl FnLowering<'_> {
  fn interface_type(&self, id: InterfaceId) -> Type {
    let name = self.typed.interface_def(id).map(|def| def.name.clone()).unwrap_or_default();

    Type::Interface(InterfaceRef { id, name })
  }

  /// The index of the itable of `interface` for values of `concrete`, built if needed.
  pub(crate) fn itable(&mut self, interface: InterfaceId, concrete: &Type) -> usize {
    if let Some(index) = self.registry.itables.iter().position(|itable| itable.interface == interface && itable.concrete == *concrete) {
      return index;
    }

    if matches!(concrete, Type::Struct(_) | Type::Enum(_)) && !self.registry.boxes.contains(concrete) {
      self.registry.boxes.push(concrete.clone());
    }

    let index = self.registry.itables.len();
    let def = self.typed.interface_def(interface).cloned();

    // Reserved first, so cycles through parents cannot recurse forever.
    self.registry.itables.push(Itable { interface, concrete: concrete.clone(), parents: Vec::new(), entries: Vec::new() });

    let Some(def) = def else { return index };
    let mut entries = Vec::new();

    for method in &def.methods {
      let name = self.resolution.fn_decl(self.package, *method).map(|decl| decl.name.name.clone()).unwrap_or_default();

      entries.push(match self.implementation(concrete, &name) {
        Some(function) => ItableEntry::Method { signature: *method, function, slot: self.typed.slots.get(&function).copied() },
        None => ItableEntry::Default(*method),
      });
    }

    for field in &def.fields {
      let at = match concrete {
        Type::Class(class) => self.typed.class_field(class.id, &field.name).map(|(class, index, _)| FieldAt::Class { class, index }),
        Type::Struct(owner) => self.typed.field_index(owner.id, &field.name).map(FieldAt::Struct),
        _ => None,
      };

      if let Some(at) = at {
        entries.push(ItableEntry::Field { field: at, ty: field.ty.clone(), set: false });
        entries.push(ItableEntry::Field { field: at, ty: field.ty.clone(), set: true });
      }
    }

    let parents = def.parents.iter().map(|parent| self.itable(*parent, concrete)).collect();

    if let Some(itable) = self.registry.itables.get_mut(index) {
      itable.parents = parents;
      itable.entries = entries;
    }

    index
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
        let itable = self.itable(target.id, from);

        self.new_value(to.clone(), Rvalue::ToDyn { value, itable })
      }
      _ => value,
    }
  }

  /// The call of the interface method `function` on a value of interface type `receiver`.
  pub(crate) fn dyn_method(&self, receiver: InterfaceId, function: FnId) -> LResult<Callee> {
    let Some(TypeItem::Interface(owner)) = self.resolution.owner(function) else {
      return error("dynamic call of a method outside an interface");
    };
    let path = self.typed.interface_path(receiver, owner).unwrap_or_default();
    let slot = self.typed.interface_def(owner).and_then(|def| def.methods.iter().position(|method| *method == function));
    let sig = self.typed.sig(function);
    let (Some(slot), Some(sig)) = (slot, sig) else {
      return error("interface method without a slot");
    };

    Ok(Callee::Dyn { path, slot, params: sig.params.clone(), ret: sig.ret.clone() })
  }

  /// The getter (`set: false`) or setter of a field reached through interface `receiver`.
  pub(crate) fn dyn_field(&self, receiver: InterfaceId, name: &str, set: bool) -> LResult<Callee> {
    let Some((owner, index, field)) = self.typed.interface_field(receiver, name) else {
      return error(format!("unknown interface field `{name}`"));
    };
    let path = self.typed.interface_path(receiver, owner).unwrap_or_default();
    let slot = field_slot(self.typed, owner, index) + usize::from(set);
    let (params, ret) = if set { (vec![field.ty.clone()], Type::Unit) } else { (Vec::new(), field.ty.clone()) };

    Ok(Callee::Dyn { path, slot, params, ret })
  }

  /// `Error(text)`: a `MessageError` holding `text`, as an `Error`.
  pub(crate) fn message_error(&mut self, text: Operand) -> LResult<Operand> {
    let Some((error, class)) = self.typed.error_types else {
      return error("`Error(...)` without the prelude");
    };
    let name = self.typed.class_def(class).map(|def| def.name.clone()).unwrap_or_default();
    let class_ty = Type::Class(ClassRef { id: class, name });
    let instance = self.new_value(class_ty.clone(), Rvalue::Alloc(class));

    if let Some(initializer) = self.typed.class_def(class).and_then(|def| def.initializer) {
      self.call(Callee::Fn(initializer), vec![instance.clone(), text], Type::Unit);
    }

    let to = self.interface_type(error);

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

  /// Calls a default method of an interface on a value of a type implementing it.
  pub(crate) fn default_receiver(&mut self, function: FnId, receiver: Operand, receiver_ty: &Type) -> Operand {
    match self.resolution.owner(function) {
      Some(TypeItem::Interface(owner)) if !matches!(receiver_ty, Type::Interface(_)) => {
        let to = self.interface_type(owner);

        self.convert(receiver, receiver_ty, &to)
      }
      _ => receiver,
    }
  }

  /// `value.name = new` where `value` has an interface type: calls the field's setter.
  pub(crate) fn set_dyn_field(&mut self, receiver: Operand, interface: InterfaceId, name: &str, value: Operand) -> LResult<()> {
    let callee = self.dyn_field(interface, name, true)?;

    self.emit(Stmt::Eval(Rvalue::Call { callee, args: vec![receiver, value] }));

    Ok(())
  }
}
