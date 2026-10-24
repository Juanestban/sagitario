//! Monomorphization (docs/spec/05-interfaces-generics.md#generics, ADR-0032): every generic
//! function, method of a generic type and default method of an interface is a template, lowered
//! once per combination of type arguments it is used with. Instances are numbered after the
//! program's functions and lowered from a worklist. Each class type with its arguments is a class
//! instance with its own fields, parent and vtable.

use sagitario_ast::NodeId;
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{FnId, ParamOwner, Resolution, TypeItem};
use sagitario_types::{ClassRef, Subst, Type, TypedModule};

use crate::FnLowering;

/// A class type with its type arguments, as the program uses it.
#[derive(Debug, Clone, PartialEq)]
pub struct ClassInstance {
  pub class: ClassRef,
  /// The class instance of its parent.
  pub parent: Option<usize>,
  /// The types of the class's own fields.
  pub fields: Vec<Type>,
  /// Which of those fields are `weak`: stored as a bare reference instead of an `Option`.
  pub weak: Vec<bool>,
  /// The methods called through the vtable, by slot.
  pub vtable: Vec<FnId>,
}

/// A template with the type arguments of one instance.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Instance {
  pub(crate) template: FnId,
  pub(crate) subst: Subst,
}

/// How deeply type arguments may nest before instances are considered endless.
const MAX_DEPTH: usize = 64;

/// Whether a function is lowered only through instances: it has type parameters, belongs to a
/// generic type, or belongs to an interface (its `Self` is the implementing type).
pub(crate) fn is_template(resolution: &Resolution, typed: &TypedModule, function: FnId) -> bool {
  match resolution.owner(function) {
    Some(TypeItem::Interface(_)) => true,
    Some(owner) if typed.generics.contains_key(&ParamOwner::Item(owner)) => true,
    _ => typed.generics.contains_key(&ParamOwner::Fn(function)),
  }
}

impl FnLowering<'_> {
  /// The function to call for `function` with the type arguments in `subst`: itself, or the
  /// instance of a template, created if needed.
  pub(crate) fn instance(&mut self, function: FnId, subst: Subst) -> FnId {
    if !is_template(self.resolution, self.typed, function) {
      return function;
    }

    if let Some(index) = self.registry.instances.iter().position(|instance| instance.template == function && instance.subst == subst) {
      return FnId(self.registry.first_instance + index as u32);
    }

    let depth = subst.params.iter().flat_map(|(_, args)| args).chain(subst.self_ty.iter()).map(Type::depth).max().unwrap_or(0);

    if depth > MAX_DEPTH {
      if !self.registry.endless {
        self.registry.endless = true;
        self.registry.diagnostics.push(
          Diagnostic::error(codes::UNSATISFIED_BOUND, self.span, "this generic code creates an endless series of instances")
            .with_note(format!("type arguments nest more than {MAX_DEPTH} levels deep, each instance requiring a larger one")),
        );
      }

      return function;
    }

    let index = self.registry.instances.len();

    self.registry.instances.push(Instance { template: function, subst });
    self.registry.pending.push(index);

    FnId(self.registry.first_instance + index as u32)
  }

  /// The type arguments a call recorded by the type checker gives `function`, in the function
  /// being lowered; `receiver` is the type of a method's receiver, the `Self` of interface
  /// methods.
  pub(crate) fn call_subst(&self, id: NodeId, function: FnId, receiver: Option<&Type>) -> Subst {
    let args: Vec<Type> = self.typed.call_args.get(&id).map(|args| args.iter().map(|arg| arg.substitute(&self.subst)).collect()).unwrap_or_default();
    let mut subst = Subst::default();
    let mut rest = args.as_slice();

    if let Some(owner) = self.resolution.owner(function) {
      let count = self.typed.generics.get(&ParamOwner::Item(owner)).map_or(0, Vec::len).min(rest.len());
      let (head, tail) = rest.split_at(count);

      if count > 0 {
        subst = subst.with(ParamOwner::Item(owner), head.to_vec());
      }

      rest = tail;

      if let (TypeItem::Interface(_), Some(receiver)) = (owner, receiver) {
        subst = subst.with_self(receiver.clone());
      }
    }

    if self.typed.generics.contains_key(&ParamOwner::Fn(function)) {
      subst = subst.with(ParamOwner::Fn(function), rest.to_vec());
    }

    subst
  }

  /// The index of the class instance of a class type, created with its parent, fields and
  /// vtable if needed.
  pub(crate) fn class_instance(&mut self, class: &ClassRef) -> usize {
    if let Some(index) = self.registry.classes.iter().position(|instance| instance.class == *class) {
      return index;
    }

    let index = self.registry.classes.len();
    let def = self.typed.class_def(class.id).cloned();
    let fields = self.typed.class_field_types(class);
    let weak = def.as_ref().map(|def| def.fields.iter().map(|field| field.weak).collect()).unwrap_or_default();

    // Reserved first: a class may refer to itself through its fields.
    self.registry.classes.push(ClassInstance { class: class.clone(), parent: None, fields: fields.clone(), weak, vtable: Vec::new() });

    let parent = self.typed.class_parent(class).map(|parent| self.class_instance(&parent));
    let mut vtable = Vec::new();

    for method in def.map(|def| def.vtable).unwrap_or_default() {
      let subst = match self.resolution.owner(method) {
        Some(TypeItem::Class(owner)) => {
          let args = self.typed.class_ancestor(class, owner).map(|ancestor| ancestor.args).unwrap_or_default();

          Subst::of(ParamOwner::Item(TypeItem::Class(owner)), args)
        }
        _ => Subst::default(),
      };

      vtable.push(self.instance(method, subst));
    }

    if let Some(instance) = self.registry.classes.get_mut(index) {
      instance.parent = parent;
      instance.vtable = vtable;
    }

    for field in &fields {
      self.register_types(field);
    }

    index
  }

  /// Registers the class instances a type needs, inside structs, enums, arrays and the like.
  pub(crate) fn register_types(&mut self, ty: &Type) {
    if !self.registry.seen.insert(ty.clone()) {
      return;
    }

    match ty {
      Type::Class(class) => {
        self.class_instance(class);
      }
      Type::Struct(inner) => {
        for field in self.typed.struct_field_types(inner) {
          self.register_types(&field);
        }
      }
      Type::Enum(_) | Type::Option(_) | Type::Result(..) => {
        for field in self.typed.variants(ty).unwrap_or_default().iter().flat_map(|variant| variant.field_types()) {
          self.register_types(&field);
        }
      }
      Type::Array(elem, _) | Type::Ptr { pointee: elem, .. } => self.register_types(elem),
      _ => {}
    }
  }
}
