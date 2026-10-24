//! Generics (docs/spec/05-interfaces-generics.md#generics, ADR-0032): substituting type arguments
//! for type parameters, the bounds of parameters, the interfaces a type implements with their
//! arguments, and the inference of the type arguments of calls, literals and `new`.
//!
//! Generic declarations are checked once, with their parameters as `Type::Param`; lowering then
//! instantiates them with the arguments recorded for each use (monomorphization).

use sagitario_ast::{Expr, Path};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{ClassId, InterfaceId, ParamOwner, TypeItem};
use sagitario_span::Span;

use crate::{Checker, ClassRef, EnumRef, FnSig, InterfaceRef, ParamRef, SELF_PARAM, StructRef, Type, TypedModule};

/// Type arguments for the parameters of generic declarations, and what `Self` stands for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Subst {
  pub params: Vec<(ParamOwner, Vec<Type>)>,
  pub self_ty: Option<Type>,
}

impl Subst {
  /// The arguments `args` for the parameters of `owner`.
  pub fn of(owner: ParamOwner, args: Vec<Type>) -> Subst {
    Subst { params: vec![(owner, args)], self_ty: None }
  }

  pub fn with(mut self, owner: ParamOwner, args: Vec<Type>) -> Subst {
    self.params.push((owner, args));
    self
  }

  pub fn with_self(mut self, self_ty: Type) -> Subst {
    self.self_ty = Some(self_ty);
    self
  }

  pub fn is_empty(&self) -> bool {
    self.params.is_empty() && self.self_ty.is_none()
  }

  fn get(&self, param: &ParamRef) -> Option<&Type> {
    if param.index == SELF_PARAM {
      return self.self_ty.as_ref();
    }

    self.params.iter().rev().find(|(owner, _)| *owner == param.owner).and_then(|(_, args)| args.get(param.index as usize))
  }

  /// This substitution applied after `inner`: `outer.compose(inner)` maps each parameter as
  /// `inner` does, then substitutes the result with `outer`.
  pub fn after(&self, inner: &Subst) -> Subst {
    let mut params: Vec<(ParamOwner, Vec<Type>)> =
      inner.params.iter().map(|(owner, args)| (*owner, args.iter().map(|arg| arg.substitute(self)).collect())).collect();

    for (owner, args) in &self.params {
      if !params.iter().any(|(existing, _)| existing == owner) {
        params.push((*owner, args.clone()));
      }
    }

    let self_ty = match &inner.self_ty {
      Some(ty) => Some(ty.substitute(self)),
      None => self.self_ty.clone(),
    };

    Subst { params, self_ty }
  }
}

impl Type {
  /// The type with each parameter replaced by its argument in `subst`, if it has one.
  pub fn substitute(&self, subst: &Subst) -> Type {
    if subst.is_empty() {
      return self.clone();
    }

    let args = |args: &[Type]| args.iter().map(|arg| arg.substitute(subst)).collect::<Vec<_>>();

    match self {
      Type::Param(param) => subst.get(param).cloned().unwrap_or_else(|| self.clone()),
      Type::Struct(inner) => Type::Struct(StructRef { id: inner.id, name: inner.name.clone(), args: args(&inner.args) }),
      Type::Enum(inner) => Type::Enum(EnumRef { id: inner.id, name: inner.name.clone(), args: args(&inner.args) }),
      Type::Class(inner) => Type::Class(ClassRef { id: inner.id, name: inner.name.clone(), args: args(&inner.args) }),
      Type::Interface(inner) => Type::Interface(InterfaceRef { id: inner.id, name: inner.name.clone(), args: args(&inner.args) }),
      Type::Array(elem, len) => Type::Array(Box::new(elem.substitute(subst)), *len),
      Type::Option(value) => Type::Option(Box::new(value.substitute(subst))),
      Type::Result(value, error) => Type::Result(Box::new(value.substitute(subst)), Box::new(error.substitute(subst))),
      Type::Ptr { mutable, pointee } => Type::Ptr { mutable: *mutable, pointee: Box::new(pointee.substitute(subst)) },
      Type::Function { params, ret } => Type::Function { params: args(params), ret: Box::new(ret.substitute(subst)) },
      other => other.clone(),
    }
  }

  /// Whether the type mentions a type parameter.
  pub fn has_params(&self) -> bool {
    match self {
      Type::Param(_) => true,
      Type::Struct(StructRef { args, .. })
      | Type::Enum(EnumRef { args, .. })
      | Type::Class(ClassRef { args, .. })
      | Type::Interface(InterfaceRef { args, .. }) => args.iter().any(Type::has_params),
      Type::Array(elem, _) | Type::Option(elem) | Type::Ptr { pointee: elem, .. } => elem.has_params(),
      Type::Result(value, error) => value.has_params() || error.has_params(),
      Type::Function { params, ret } => params.iter().any(Type::has_params) || ret.has_params(),
      _ => false,
    }
  }

  /// Whether the type mentions the `Self` of an interface.
  pub fn has_self(&self) -> bool {
    match self {
      Type::Param(param) => param.index == SELF_PARAM,
      Type::Struct(StructRef { args, .. })
      | Type::Enum(EnumRef { args, .. })
      | Type::Class(ClassRef { args, .. })
      | Type::Interface(InterfaceRef { args, .. }) => args.iter().any(Type::has_self),
      Type::Array(elem, _) | Type::Option(elem) | Type::Ptr { pointee: elem, .. } => elem.has_self(),
      Type::Result(value, error) => value.has_self() || error.has_self(),
      Type::Function { params, ret } => params.iter().any(Type::has_self) || ret.has_self(),
      _ => false,
    }
  }

  /// How deeply generic types nest in the type, to stop instantiations that never end.
  pub fn depth(&self) -> usize {
    match self {
      Type::Struct(StructRef { args, .. })
      | Type::Enum(EnumRef { args, .. })
      | Type::Class(ClassRef { args, .. })
      | Type::Interface(InterfaceRef { args, .. }) => 1 + args.iter().map(Type::depth).max().unwrap_or(0),
      Type::Array(elem, _) | Type::Option(elem) | Type::Ptr { pointee: elem, .. } => 1 + elem.depth(),
      Type::Result(value, error) => 1 + value.depth().max(error.depth()),
      Type::Function { params, ret } => 1 + params.iter().chain([ret.as_ref()]).map(Type::depth).max().unwrap_or(0),
      _ => 0,
    }
  }
}

impl FnSig {
  pub fn substitute(&self, subst: &Subst) -> FnSig {
    FnSig { receiver: self.receiver, params: self.params.iter().map(|param| param.substitute(subst)).collect(), ret: self.ret.substitute(subst) }
  }
}

/// The type arguments being inferred for some generic declarations: one slot per parameter.
pub(crate) struct Unknowns {
  groups: Vec<(ParamOwner, Vec<Option<Type>>)>,
}

impl Unknowns {
  pub(crate) fn new() -> Unknowns {
    Unknowns { groups: Vec::new() }
  }

  /// Adds the parameters of `owner`, with the arguments given explicitly, if any.
  pub(crate) fn add(&mut self, owner: ParamOwner, count: usize, explicit: Option<Vec<Type>>) {
    let slots = match explicit {
      Some(args) => args.into_iter().map(Some).collect(),
      None => vec![None; count],
    };

    self.groups.push((owner, slots));
  }

  pub(crate) fn is_empty(&self) -> bool {
    self.groups.is_empty()
  }

  /// The arguments known so far; unknown ones stay parameters.
  pub(crate) fn subst(&self) -> Subst {
    Subst {
      params: self
        .groups
        .iter()
        .map(|(owner, slots)| {
          let args = slots
            .iter()
            .enumerate()
            .map(|(index, slot)| slot.clone().unwrap_or(Type::Param(ParamRef { owner: *owner, index: index as u32, name: String::new() })))
            .collect();

          (*owner, args)
        })
        .collect(),
      self_ty: None,
    }
  }

  /// Learns arguments by matching the type `pattern`, which mentions the unknown parameters,
  /// against the type `actual` of a value.
  pub(crate) fn unify(&mut self, pattern: &Type, actual: &Type) {
    match (pattern, actual) {
      (_, Type::Never | Type::Error) => {}
      (Type::Param(param), _) => {
        if let Some((_, slots)) = self.groups.iter_mut().find(|(owner, _)| *owner == param.owner)
          && let Some(slot) = slots.get_mut(param.index as usize)
          && slot.is_none()
        {
          *slot = Some(actual.clone());
        }
      }
      (Type::Option(pattern), Type::Option(actual)) | (Type::Array(pattern, _), Type::Array(actual, _)) => self.unify(pattern, actual),
      (Type::Ptr { pointee: pattern, .. }, Type::Ptr { pointee: actual, .. }) => self.unify(pattern, actual),
      (Type::Result(value, error), Type::Result(actual_value, actual_error)) => {
        self.unify(value, actual_value);
        self.unify(error, actual_error);
      }
      (Type::Struct(pattern), Type::Struct(actual)) if pattern.id == actual.id => self.unify_all(&pattern.args, &actual.args),
      (Type::Enum(pattern), Type::Enum(actual)) if pattern.id == actual.id => self.unify_all(&pattern.args, &actual.args),
      (Type::Class(pattern), Type::Class(actual)) if pattern.id == actual.id => self.unify_all(&pattern.args, &actual.args),
      (Type::Interface(pattern), Type::Interface(actual)) if pattern.id == actual.id => self.unify_all(&pattern.args, &actual.args),
      (Type::Function { params, ret }, Type::Function { params: actual_params, ret: actual_ret }) if params.len() == actual_params.len() => {
        self.unify_all(params, actual_params);
        self.unify(ret, actual_ret);
      }
      _ => {}
    }
  }

  fn unify_all(&mut self, patterns: &[Type], actuals: &[Type]) {
    for (pattern, actual) in patterns.iter().zip(actuals) {
      self.unify(pattern, actual);
    }
  }

  /// The arguments of each declaration after inference, and the parameters still unknown (with
  /// their owner), which become `Type::Error`.
  pub(crate) fn finish(self) -> (Subst, Vec<(ParamOwner, usize)>) {
    let mut missing = Vec::new();
    let groups = self
      .groups
      .into_iter()
      .map(|(owner, slots)| {
        let args = slots
          .into_iter()
          .enumerate()
          .map(|(index, slot)| {
            slot.unwrap_or_else(|| {
              missing.push((owner, index));
              Type::Error
            })
          })
          .collect();

        (owner, args)
      })
      .collect();

    (Subst { params: groups, self_ty: None }, missing)
  }
}

impl TypedModule {
  /// The parameters of a declaration as types: the arguments of the declaration used inside
  /// itself.
  pub fn identity_args(&self, owner: ParamOwner) -> Vec<Type> {
    let names = self.generics.get(&owner).map(|params| params.iter().map(|param| param.name.clone()).collect::<Vec<_>>()).unwrap_or_default();

    names.into_iter().enumerate().map(|(index, name)| Type::Param(ParamRef { owner, index: index as u32, name })).collect()
  }

  /// The interfaces a parameter is bounded by; `Self` is bounded by its interface.
  pub fn param_bounds(&self, param: &ParamRef) -> Vec<InterfaceRef> {
    if param.index == SELF_PARAM {
      return match param.owner {
        ParamOwner::Item(TypeItem::Interface(id)) => {
          let name = self.interface_def(id).map(|def| def.name.clone()).unwrap_or_default();

          vec![InterfaceRef { id, name, args: self.identity_args(param.owner) }]
        }
        _ => Vec::new(),
      };
    }

    self.generics.get(&param.owner).and_then(|params| params.get(param.index as usize)).map(|param| param.bounds.clone()).unwrap_or_default()
  }

  /// An interface followed by its ancestors, with their type arguments.
  pub fn interface_ancestor_refs(&self, interface: &InterfaceRef) -> Vec<InterfaceRef> {
    let mut found = vec![interface.clone()];
    let mut next = 0;

    while let Some(current) = found.get(next).cloned() {
      next += 1;

      let subst = Subst::of(ParamOwner::Item(TypeItem::Interface(current.id)), current.args.clone());

      for parent in self.interface_def(current.id).map(|def| def.parents.clone()).unwrap_or_default() {
        let parent = InterfaceRef { args: parent.args.iter().map(|arg| arg.substitute(&subst)).collect(), ..parent };

        if !found.contains(&parent) && found.len() <= self.interfaces.len() * 4 {
          found.push(parent);
        }
      }
    }

    found
  }

  /// A class followed by its ancestors, with their type arguments.
  pub fn class_ancestors(&self, class: &ClassRef) -> Vec<ClassRef> {
    let mut chain = vec![class.clone()];

    while let Some(last) = chain.last().cloned() {
      let Some(parent) = self.class_def(last.id).and_then(|def| def.parent.clone()) else { break };

      if chain.len() > self.classes.len() {
        break;
      }

      let subst = Subst::of(ParamOwner::Item(TypeItem::Class(last.id)), last.args.clone());

      chain.push(ClassRef { args: parent.args.iter().map(|arg| arg.substitute(&subst)).collect(), ..parent });
    }

    chain
  }

  /// The interfaces a type implements, with their type arguments: those a struct, enum or class
  /// (and its ancestors) names after `impl`, the ancestors of an interface, or the bounds of a
  /// parameter, each with its ancestors.
  pub fn implemented_refs(&self, ty: &Type) -> Vec<InterfaceRef> {
    let direct: Vec<InterfaceRef> = match ty {
      Type::Struct(inner) => self.impls_of(TypeItem::Struct(inner.id), &inner.args),
      Type::Enum(inner) => self.impls_of(TypeItem::Enum(inner.id), &inner.args),
      Type::Class(inner) => self.class_ancestors(inner).iter().flat_map(|class| self.impls_of(TypeItem::Class(class.id), &class.args)).collect(),
      Type::Interface(inner) => vec![inner.clone()],
      Type::Param(param) => self.param_bounds(param),
      _ => Vec::new(),
    };
    let mut found: Vec<InterfaceRef> = Vec::new();

    for interface in direct {
      for ancestor in self.interface_ancestor_refs(&interface) {
        if !found.contains(&ancestor) {
          found.push(ancestor);
        }
      }
    }

    found
  }

  fn impls_of(&self, item: TypeItem, args: &[Type]) -> Vec<InterfaceRef> {
    let subst = Subst::of(ParamOwner::Item(item), args.to_vec());

    self
      .impls
      .get(&item)
      .map(|impls| {
        impls
          .iter()
          .map(|interface| InterfaceRef { args: interface.args.iter().map(|arg| arg.substitute(&subst)).collect(), ..interface.clone() })
          .collect()
      })
      .unwrap_or_default()
  }

  /// Whether values of `ty` implement `interface` (with the same type arguments).
  pub fn implements(&self, ty: &Type, interface: &InterfaceRef) -> bool {
    self.implemented_refs(ty).contains(interface)
  }

  /// The field types of a struct type, with its type arguments.
  pub fn struct_field_types(&self, ty: &StructRef) -> Vec<Type> {
    let subst = Subst::of(ParamOwner::Item(TypeItem::Struct(ty.id)), ty.args.clone());

    self.struct_def(ty.id).map(|def| def.fields.iter().map(|field| field.ty.substitute(&subst)).collect()).unwrap_or_default()
  }

  /// The type of a field of a class type or one of its ancestors: the declaring class with its
  /// arguments, the field's position there and its type.
  pub fn class_field_type(&self, class: &ClassRef, name: &str) -> Option<(ClassRef, usize, Type)> {
    self.class_ancestors(class).into_iter().find_map(|ancestor| {
      let def = self.class_def(ancestor.id)?;
      let index = def.fields.iter().position(|field| field.name == name)?;
      let subst = Subst::of(ParamOwner::Item(TypeItem::Class(ancestor.id)), ancestor.args.clone());
      let ty = def.fields.get(index)?.ty.substitute(&subst);

      Some((ancestor, index, ty))
    })
  }

  /// The field types of a class type's own fields, with its type arguments.
  pub fn class_field_types(&self, class: &ClassRef) -> Vec<Type> {
    let subst = Subst::of(ParamOwner::Item(TypeItem::Class(class.id)), class.args.clone());

    self.class_def(class.id).map(|def| def.fields.iter().map(|field| field.ty.substitute(&subst)).collect()).unwrap_or_default()
  }

  /// The parent of a class type, with its type arguments.
  pub fn class_parent(&self, class: &ClassRef) -> Option<ClassRef> {
    self.class_ancestors(class).into_iter().nth(1)
  }

  /// The ancestor of `ty` (itself included) declaring the class `id`, with its arguments.
  pub fn class_ancestor(&self, ty: &ClassRef, id: ClassId) -> Option<ClassRef> {
    self.class_ancestors(ty).into_iter().find(|ancestor| ancestor.id == id)
  }

  /// The ancestor interface `id` of an interface type, or one of the interfaces a type
  /// implements, with its arguments.
  pub fn implemented_ref(&self, ty: &Type, id: InterfaceId) -> Option<InterfaceRef> {
    self.implemented_refs(ty).into_iter().find(|interface| interface.id == id)
  }
}

impl Checker<'_> {
  /// The arguments written explicitly in a path, as types.
  pub(crate) fn explicit_args(&mut self, path: &Path) -> Option<(usize, Vec<Type>)> {
    let args = path.type_args.as_ref()?;
    let types = args.args.iter().map(|arg| self.type_expr(arg)).collect();

    Some((args.segment, types))
  }

  /// Reports a wrong number of explicit type arguments, and returns whether it was right.
  pub(crate) fn check_arg_count(&mut self, span: Span, what: &str, expected: usize, found: usize) -> bool {
    if expected == found {
      return true;
    }

    let message = if expected == 0 {
      format!("{what} takes no type arguments")
    } else {
      format!("{what} takes {expected} type argument{} but {found} were supplied", if expected == 1 { "" } else { "s" })
    };

    self.error(codes::WRONG_TYPE_ARGUMENTS, span, message);

    false
  }

  /// Checks call arguments against parameter types that may mention the parameters being
  /// inferred: each argument teaches `unknowns` what it can, and is checked against its type
  /// once known.
  pub(crate) fn generic_args(&mut self, span: Span, what: &str, params: &[Type], args: &[Expr], fixed: &Subst, unknowns: &mut Unknowns) {
    if args.len() != params.len() {
      self.error(
        codes::WRONG_ARGUMENT_COUNT,
        span,
        format!("this {what} takes {} argument{} but {} were supplied", params.len(), if params.len() == 1 { "" } else { "s" }, args.len()),
      );
    }

    // Lambdas come last, so their parameter types can come from the other arguments.
    for lambdas in [false, true] {
      for (index, arg) in args.iter().enumerate().filter(|(_, arg)| matches!(arg.kind, sagitario_ast::ExprKind::Lambda { .. }) == lambdas) {
        match params.get(index) {
          Some(param) => self.generic_arg(arg, param, fixed, unknowns),
          None => {
            self.expr(arg, None);
          }
        }
      }
    }
  }

  /// Checks one value against a type that may mention parameters being inferred.
  pub(crate) fn generic_arg(&mut self, arg: &Expr, param: &Type, fixed: &Subst, unknowns: &mut Unknowns) {
    let pattern = param.substitute(fixed);
    let known = pattern.substitute(&unknowns.subst());

    if !mentions_unknown(&known, &unknowns.subst()) {
      self.expect(arg, &known);

      return;
    }

    let actual = self.expr(arg, None);

    unknowns.unify(&pattern, &actual);

    let expected = pattern.substitute(&unknowns.subst());

    if !mentions_unknown(&expected, &unknowns.subst()) && !self.fits(&actual, &expected) {
      self.mismatch(arg.span, &expected, &actual);
    }
  }

  /// A call of a module function or of a type's static function, inferring the type arguments
  /// of generic ones.
  pub(crate) fn fn_call(&mut self, expr: &Expr, path: &Path, function: sagitario_resolve::FnId, args: &[Expr], expected: Option<&Type>) -> Type {
    let Some(sig) = self.typed.sig(function).cloned() else {
      self.unchecked_args(args);

      return Type::Error;
    };
    let name = self.resolution.fn_decl(self.package, function).map(|decl| decl.name.name.clone()).unwrap_or_default();
    let owner = self.resolution.owner(function);
    let owner_count = owner.map_or(0, |owner| self.generic_count(ParamOwner::Item(owner)));
    let own_count = self.generic_count(ParamOwner::Fn(function));
    let explicit = self.explicit_args(path);
    let last = path.segments.len().saturating_sub(1);

    if owner_count == 0 && own_count == 0 {
      if let Some((_, args)) = &explicit {
        self.check_arg_count(path.span, &format!("`{name}`"), 0, args.len());
      }

      self.args(expr.span, "function", &sig.params, args);

      return sig.ret;
    }

    let mut unknowns = Unknowns::new();

    if let Some(owner) = owner.filter(|_| owner_count > 0) {
      let given = match &explicit {
        Some((segment, args)) if *segment != last => {
          let args = args.clone();

          self
            .check_arg_count(path.span, &format!("`{}`", self.resolution.type_name(self.package, owner)), owner_count, args.len())
            .then_some(args.clone())
            .or(Some(vec![Type::Error; owner_count]))
        }
        // `Self::f()` keeps the arguments of the type being declared.
        _ if path.segments.first().is_some_and(|segment| segment.name == "Self") => Some(self.identity_args(ParamOwner::Item(owner))),
        _ => None,
      };

      unknowns.add(ParamOwner::Item(owner), owner_count, given);
    }

    let own_given = match &explicit {
      Some((segment, args)) if *segment == last => {
        let args = args.clone();

        self.check_arg_count(path.span, &format!("`{name}`"), own_count, args.len()).then_some(args.clone()).or(Some(vec![Type::Error; own_count]))
      }
      _ => None,
    };

    unknowns.add(ParamOwner::Fn(function), own_count, own_given);

    if let Some(expected) = expected {
      unknowns.unify(&sig.ret, expected);
    }

    self.generic_args(expr.span, "function", &sig.params, args, &Subst::default(), &mut unknowns);

    let subst = self.finish_inference(expr.span, &format!("`{name}`"), unknowns);
    let own = subst.params.iter().find(|(owner, _)| *owner == ParamOwner::Fn(function)).map(|(_, args)| args.clone()).unwrap_or_default();

    self.record_call_args(expr.id, &subst, function, own);

    sig.ret.substitute(&subst)
  }

  /// The arguments and result of a call to `function` on a receiver of type `receiver_ty`: the
  /// parameters of the type declaring it take the receiver's arguments, and its own are inferred.
  pub(crate) fn generic_method_call(&mut self, expr: &Expr, receiver_ty: &Type, function: sagitario_resolve::FnId, args: &[Expr]) -> Type {
    let Some(sig) = self.typed.sig(function).cloned() else {
      self.unchecked_args(args);

      return Type::Error;
    };
    let fixed = self.inherited_subst(receiver_ty, function);
    let own_count = self.generic_count(ParamOwner::Fn(function));
    let mut unknowns = Unknowns::new();

    unknowns.add(ParamOwner::Fn(function), own_count, None);
    self.generic_args(expr.span, "method", &sig.params, args, &fixed, &mut unknowns);

    let name = self.resolution.fn_decl(self.package, function).map(|decl| decl.name.name.clone()).unwrap_or_default();
    let own = self.finish_inference(expr.span, &format!("`{name}`"), unknowns);
    let own_args = own.params.first().map(|(_, args)| args.clone()).unwrap_or_default();
    let subst = own.after(&fixed).with(ParamOwner::Fn(function), own_args.clone());
    let subst = match fixed.self_ty.clone() {
      Some(self_ty) => subst.with_self(self_ty),
      None => subst,
    };

    self.record_call_args(expr.id, &fixed, function, own_args);

    sig.ret.substitute(&subst)
  }

  /// Reports the parameters inference could not determine, and returns the substitution.
  pub(crate) fn finish_inference(&mut self, span: Span, what: &str, unknowns: Unknowns) -> Subst {
    let (subst, missing) = unknowns.finish();

    for (owner, index) in missing {
      let name = self.typed.generics.get(&owner).and_then(|params| params.get(index)).map(|param| param.name.clone()).unwrap_or_default();

      self.diagnostics.push(
        Diagnostic::error(codes::UNKNOWN_TYPE, span, format!("cannot infer the type parameter `{name}` of {what}"))
          .with_help("write the type arguments explicitly, such as `name<Type>(...)`, or annotate the expected type"),
      );
    }

    self.check_bounds(span, &subst);

    subst
  }

  /// Reports type arguments that do not implement the bounds of their parameters.
  pub(crate) fn check_bounds(&mut self, span: Span, subst: &Subst) {
    for (owner, args) in &subst.params {
      let params = self.typed.generics.get(owner).cloned().unwrap_or_default();

      for (param, arg) in params.iter().zip(args) {
        for bound in &param.bounds {
          let bound = InterfaceRef { args: bound.args.iter().map(|ty| ty.substitute(subst)).collect(), ..bound.clone() };
          let bound_ty = Type::Interface(bound.clone());

          if matches!(arg, Type::Error | Type::Never) || self.typed.implements(arg, &bound) {
            continue;
          }

          self.diagnostics.push(
            Diagnostic::error(codes::UNSATISFIED_BOUND, span, format!("`{arg}` does not implement `{bound_ty}`"))
              .with_note(format!("the type parameter `{}` requires `{bound_ty}`", param.name)),
          );
        }
      }
    }
  }
}

/// Whether `ty` mentions a parameter that `subst` leaves unchanged.
fn mentions_unknown(ty: &Type, subst: &Subst) -> bool {
  match ty {
    Type::Param(param) => subst.params.iter().any(|(owner, args)| {
      *owner == param.owner && args.get(param.index as usize).is_some_and(|arg| matches!(arg, Type::Param(same) if same == param))
    }),
    Type::Struct(StructRef { args, .. })
    | Type::Enum(EnumRef { args, .. })
    | Type::Class(ClassRef { args, .. })
    | Type::Interface(InterfaceRef { args, .. }) => args.iter().any(|arg| mentions_unknown(arg, subst)),
    Type::Array(elem, _) | Type::Option(elem) | Type::Ptr { pointee: elem, .. } => mentions_unknown(elem, subst),
    Type::Result(value, error) => mentions_unknown(value, subst) || mentions_unknown(error, subst),
    Type::Function { params, ret } => params.iter().any(|param| mentions_unknown(param, subst)) || mentions_unknown(ret, subst),
    _ => false,
  }
}

impl Checker<'_> {
  /// The type parameters of every generic declaration, with their bounds.
  pub(crate) fn generic_defs(&mut self) {
    let mut owners: Vec<ParamOwner> = Vec::new();

    owners.extend((0..self.resolution.structs.len() as u32).map(|id| ParamOwner::Item(TypeItem::Struct(sagitario_resolve::StructId(id)))));
    owners.extend((0..self.resolution.enums.len() as u32).map(|id| ParamOwner::Item(TypeItem::Enum(sagitario_resolve::EnumId(id)))));
    owners.extend((0..self.resolution.classes.len() as u32).map(|id| ParamOwner::Item(TypeItem::Class(ClassId(id)))));
    owners.extend((0..self.resolution.interfaces.len() as u32).map(|id| ParamOwner::Item(TypeItem::Interface(InterfaceId(id)))));
    owners.extend((0..self.resolution.functions.len() as u32).map(|id| ParamOwner::Fn(sagitario_resolve::FnId(id))));

    for owner in owners {
      let params = self.resolution.generics(self.package, owner);

      if params.is_empty() {
        continue;
      }

      self.owner = match owner {
        ParamOwner::Item(item) => Some(item),
        ParamOwner::Fn(id) => self.resolution.owner(id),
      };

      // Bounds are not values of the interface, so interfaces mentioning `Self` are fine here.
      let interface_types = self.interface_types.len();
      let mut defs = Vec::new();

      for param in params {
        let mut bounds = Vec::new();

        for bound in &param.bounds {
          match self.type_expr(bound) {
            Type::Interface(interface) => bounds.push(interface),
            Type::Error => {}
            other => {
              self.error(codes::UNSATISFIED_BOUND, bound.span, format!("the bound of `{}` must be an interface, not `{other}`", param.name.name))
            }
          }
        }

        defs.push(crate::GenericDef { name: param.name.name.clone(), bounds });
      }

      self.interface_types.truncate(interface_types);
      self.typed.generics.insert(owner, defs);
    }

    self.owner = None;
  }

  /// The bounds of the type arguments written in types, once every type's interfaces are known.
  pub(crate) fn check_type_bounds(&mut self) {
    for (span, ty) in std::mem::take(&mut self.type_bounds) {
      let (item, args) = match &ty {
        Type::Struct(inner) => (TypeItem::Struct(inner.id), inner.args.clone()),
        Type::Enum(inner) => (TypeItem::Enum(inner.id), inner.args.clone()),
        Type::Class(inner) => (TypeItem::Class(inner.id), inner.args.clone()),
        Type::Interface(inner) => (TypeItem::Interface(inner.id), inner.args.clone()),
        _ => continue,
      };

      self.check_bounds(span, &Subst::of(ParamOwner::Item(item), args));
    }
  }
}

impl Checker<'_> {
  /// Records the type arguments of a call to `function`: those its type receives in `subst`,
  /// then `own` for its own parameters. Nothing is recorded for calls without arguments.
  pub(crate) fn record_call_args(&mut self, id: sagitario_ast::NodeId, subst: &Subst, function: sagitario_resolve::FnId, own: Vec<Type>) {
    let mut args = match self.resolution.owner(function) {
      Some(owner) => {
        subst.params.iter().find(|(param_owner, _)| *param_owner == ParamOwner::Item(owner)).map(|(_, args)| args.clone()).unwrap_or_default()
      }
      None => Vec::new(),
    };

    args.extend(own);

    if !args.is_empty() {
      self.typed.call_args.insert(id, args);
    }
  }

  /// Like `record_call_args`, for a function without own type parameters.
  pub(crate) fn record_owner_args(&mut self, id: sagitario_ast::NodeId, subst: &Subst, function: sagitario_resolve::FnId) {
    self.record_call_args(id, subst, function, Vec::new());
  }
}

impl Checker<'_> {
  /// The type arguments of a generic enum being inferred for one of its variants: from the
  /// path (`Tree<i32>::Leaf`) and the expected type.
  pub(crate) fn enum_unknowns(&mut self, id: sagitario_resolve::EnumId, expected: Option<&Type>, written: Option<&Path>) -> Unknowns {
    let owner = ParamOwner::Item(TypeItem::Enum(id));
    let count = self.generic_count(owner);
    let explicit = match written {
      Some(path) => {
        let found = self.explicit_args(path).map(|(_, args)| args);

        found.filter(|args| self.check_arg_count(path.span, "this enum", count, args.len()))
      }
      None => None,
    };
    let mut unknowns = Unknowns::new();

    unknowns.add(owner, count, explicit);

    if let Some(expected) = expected {
      unknowns.unify(&self.type_ref(TypeItem::Enum(id)), expected);
    }

    unknowns
  }

  /// The type of a generic enum once its arguments are inferred.
  pub(crate) fn finish_enum(&mut self, span: Span, id: sagitario_resolve::EnumId, unknowns: &mut Unknowns) -> Type {
    let name = self.resolution.type_name(self.package, TypeItem::Enum(id));
    let taken = std::mem::replace(unknowns, Unknowns::new());
    let subst = self.finish_inference(span, &format!("`{name}`"), taken);
    let args = subst.params.into_iter().next().map(|(_, args)| args).unwrap_or_default();

    self.named_type(TypeItem::Enum(id), args)
  }
}
