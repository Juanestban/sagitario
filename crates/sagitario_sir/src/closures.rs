//! Closures (docs/spec/08-closures.md, ADR-0033). A function value is a function pointer and an
//! environment: an ARC object holding copies of the captured bindings, or none when nothing is
//! captured. Each lambda becomes a function of its own that receives the environment first,
//! copies the captures into its locals and then runs its body. Named functions used as values get
//! a wrapper with the same convention.

use std::collections::HashMap;

use sagitario_ast::Expr;
use sagitario_resolve::{FnId, LocalId};
use sagitario_types::{BuiltinMethod, Type};

use crate::mono::Instance;
use crate::{Callee, Context, FnLowering, Function, LResult, Local, LowerError, Operand, Place, Projection, Rvalue, Stmt, error, is_valueless};

impl FnLowering<'_> {
  /// The SIR local of a binding of the source function; inside a lambda, its own numbering.
  pub(crate) fn local_id(&self, id: LocalId) -> Local {
    match &self.local_map {
      Some(map) => map.get(&id.0).copied().unwrap_or(Local(id.0)),
      None => Local(id.0),
    }
  }

  /// `(params) => body`: lowers the body as a function of its own and creates the closure value.
  pub(crate) fn lambda(&mut self, expr: &Expr, body: &Expr, ty: &Type) -> LResult<Operand> {
    let info = self.resolution.lambdas.get(&expr.id).cloned().ok_or_else(|| LowerError("lambda without resolution".to_string()))?;
    let Type::Function { params, ret } = ty else {
      return error("lambda without a function type");
    };
    let captures: Vec<(LocalId, Local, Type)> = info
      .captures
      .iter()
      .map(|id| {
        let local = self.local_id(*id);

        (*id, local, self.local_type(local))
      })
      .collect();
    let env = if captures.is_empty() {
      None
    } else {
      self.registry.envs.push(captures.iter().map(|(_, _, ty)| ty.clone()).collect());

      Some(self.registry.envs.len() - 1)
    };

    // The lambda's locals: the environment, its parameters, the captures, then its bindings.
    let mut map = HashMap::new();
    let mut locals = vec![Type::Unit];

    for (id, param) in info.params.iter().zip(params) {
      map.insert(id.0, Local(locals.len() as u32));
      locals.push(param.clone());
    }

    for (id, _, capture_ty) in &captures {
      map.insert(id.0, Local(locals.len() as u32));
      locals.push(capture_ty.clone());
    }

    for id in info.first_local..info.end_local {
      if map.contains_key(&id) {
        continue;
      }

      let local_ty = self.typed.local_type(self.source_fn, LocalId(id)).map_or(Type::Error, |local_ty| local_ty.substitute(&self.subst));

      map.insert(id, Local(locals.len() as u32));
      locals.push(local_ty);
    }

    let slot = self.registry.instances.len();

    self.registry.instances.push(Instance { template: self.source_fn, subst: self.subst.clone(), lambda: true });

    let function = FnId(self.registry.first_instance + slot as u32);
    let param_count = 1 + params.len();
    let lowered = {
      let mut cx = Context {
        package: self.package,
        resolution: self.resolution,
        typed: self.typed,
        layouts: self.layouts,
        sources: self.sources,
        registry: &mut *self.registry,
      };
      let mut lowering = FnLowering::new(&mut cx, ret.as_ref().clone(), locals, self.subst.clone(), body.span);

      lowering.source_fn = self.source_fn;
      lowering.local_map = Some(map.clone());
      lowering.lambda_body(body, ret, env, &captures, &map)?;

      Function {
        name: "lambda".to_string(),
        template: false,
        closure: true,
        param_count,
        self_by_ref: false,
        export: None,
        ret: ret.as_ref().clone(),
        locals: lowering.locals,
        body: lowering.buffers.pop().unwrap_or_default(),
      }
    };

    self.registry.lambdas.push((slot, lowered));

    let values = captures.iter().map(|(_, local, _)| Operand::Local(*local)).collect();

    Ok(self.new_value(ty.clone(), Rvalue::Closure { function, env, captures: values }))
  }

  /// The body of a lambda: copies the captures out of the environment (local 0), runs the body
  /// and returns its value.
  fn lambda_body(
    &mut self,
    body: &Expr,
    ret: &Type,
    env: Option<usize>,
    captures: &[(LocalId, Local, Type)],
    map: &HashMap<u32, Local>,
  ) -> LResult<()> {
    let mut owned = Vec::new();

    if let Some(env) = env {
      for (index, (id, _, capture_ty)) in captures.iter().enumerate() {
        let Some(local) = map.get(&id.0).copied() else { continue };
        let place = Place { local: Local(0), projections: vec![Projection::EnvField { env, index }], ty: capture_ty.clone() };

        self.store(local, Operand::Place(place));

        if self.managed(capture_ty) {
          owned.push(local);
        }
      }
    }

    // Released when the lambda returns, like the bindings of its outermost block.
    self.scopes.push(owned);

    let result = (!is_valueless(ret)).then(|| self.temp(ret.clone()));
    let mark = self.pending.len();
    let value = self.operand_as(body, ret)?;

    if let Some(result) = result
      && !is_valueless(&self.type_of(body)?)
    {
      self.store(result, value);
    }

    self.release_pending_from(mark);

    for local in self.scopes.pop().unwrap_or_default().into_iter().rev() {
      self.emit(Stmt::Release(local));
    }

    self.emit(Stmt::Return(result.map(Operand::Local)));

    Ok(())
  }

  /// A named function used as a value: a closure without environment calling it.
  pub(crate) fn fn_value(&mut self, expr: &Expr, function: FnId, ty: &Type) -> Operand {
    let subst = self.call_subst(expr.id, function, None);
    let target = self.instance(function, subst);

    if !self.registry.wrapped.contains(&target) {
      self.registry.wrapped.push(target);
    }

    self.new_value(ty.clone(), Rvalue::FnValue(target))
  }

  /// Calls a function value with arguments (already converted to its parameter types).
  pub(crate) fn call_closure(&mut self, value: Operand, args: Vec<Operand>, params: Vec<Type>, ret: Type) -> Operand {
    let mut operands = vec![value];

    operands.extend(args);

    self.call(Callee::Closure { params, ret: ret.clone() }, operands, ret)
  }

  /// `callee(args)` where `callee` is a value of a function type.
  pub(crate) fn call_value(&mut self, callee: &Expr, args: &[Expr]) -> LResult<Operand> {
    let Type::Function { params, ret } = self.type_of(callee)? else {
      return error("call of a value that is not a function");
    };
    let value = self.expr(callee)?;
    // The function value must stay alive while it runs, even if the call replaces its source.
    let value = self.owned_copy(Type::Function { params: params.clone(), ret: ret.clone() }, value);
    let mut operands = Vec::new();

    for (arg, param) in args.iter().zip(&params) {
      operands.push(self.operand_as(arg, param)?);
    }

    Ok(self.call_closure(value, operands, params, *ret))
  }

  /// `value.name(args)` calling a field of a function type.
  pub(crate) fn field_call(&mut self, receiver: &Expr, name: &str, args: &[Expr]) -> LResult<Operand> {
    let receiver_ty = self.type_of(receiver)?;
    let (projection, field_ty) = match &receiver_ty {
      Type::Struct(owner) => {
        let index = self.typed.field_index(owner.id, name).ok_or_else(|| LowerError(format!("unknown field `{name}`")))?;
        let field_ty = self.typed.struct_field_types(owner).into_iter().nth(index).unwrap_or(Type::Error);

        (Projection::Field(index), field_ty)
      }
      Type::Class(owner) => {
        let (declaring, index, field_ty) = self.typed.class_field_type(owner, name).ok_or_else(|| LowerError(format!("unknown field `{name}`")))?;

        (Projection::ClassField { class: self.class_instance(&declaring), index }, field_ty)
      }
      _ => return error("field call on a value without fields"),
    };
    let Type::Function { params, ret } = field_ty.clone() else {
      return error("field call of a field that is not a function");
    };
    let mut place = self.base_place(receiver)?;

    place.projections.push(projection);
    place.ty = field_ty.clone();

    let value = self.owned_copy(field_ty, Operand::Place(place));
    let mut operands = Vec::new();

    for (arg, param) in args.iter().zip(&params) {
      operands.push(self.operand_as(arg, param)?);
    }

    Ok(self.call_closure(value, operands, params, *ret))
  }

  /// `map`, `map_err`, `and_then`, `unwrap_or_else` and `filter` of `Option` and `Result`.
  pub(crate) fn closure_method(&mut self, method: &BuiltinMethod, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let receiver_ty = self.type_of(receiver)?;
    let place = self.value_place(receiver)?;
    let arg = args.first().ok_or_else(|| LowerError("closure method without a closure".to_string()))?;
    let Type::Function { params, ret } = self.type_of(arg)? else {
      return error("closure method without a function");
    };
    let closure = self.expr(arg)?;
    let closure = self.owned_copy(Type::Function { params: params.clone(), ret: ret.clone() }, closure);
    let is_option = matches!(receiver_ty, Type::Option(_));
    let success = if is_option { 1 } else { 0 };
    let (value_ty, error_ty) = match &receiver_ty {
      Type::Option(value) => (value.as_ref().clone(), Type::Unit),
      Type::Result(value, error) => (value.as_ref().clone(), error.as_ref().clone()),
      _ => return error("closure method on a value that is not an `Option` or a `Result`"),
    };
    let dest = self.temp(ty.clone());
    let cond = self.tag_is(&place, success)?;
    let call = |lowering: &mut Self, args: Vec<Operand>| lowering.call_closure(closure.clone(), args, params.clone(), ret.as_ref().clone());
    let field = |variant: u32, field_ty: Type| {
      let mut projections = place.projections.clone();

      projections.push(Projection::VariantField { variant, field: 0 });

      Place { local: place.local, projections, ty: field_ty }
    };
    let (ok_place, err_place) = (field(success, value_ty.clone()), field(1, error_ty.clone()));
    let receiver_copy = Operand::Place(place.clone());
    let method = method.clone();
    let else_method = method.clone();
    let then_body = self.buffer(|lowering| {
      let value = lowering.read(ok_place.clone());
      let result = match method {
        BuiltinMethod::Map => {
          let mapped = call(lowering, vec![value]);
          let index = if is_option { 1 } else { 0 };

          lowering.variant(ty.clone(), index, vec![mapped])
        }
        BuiltinMethod::MapErr => lowering.variant(ty.clone(), 0, vec![value]),
        BuiltinMethod::AndThen => call(lowering, vec![value]),
        BuiltinMethod::UnwrapOrElse => value,
        BuiltinMethod::Filter => {
          let keep = call(lowering, vec![value]);
          let kept = lowering.buffer(|lowering| {
            lowering.store(dest, receiver_copy.clone());

            Ok(())
          })?;
          let dropped = lowering.buffer(|lowering| {
            let none = lowering.variant(ty.clone(), 0, Vec::new());

            lowering.store(dest, none);

            Ok(())
          })?;

          lowering.emit(Stmt::If { cond: keep, then_body: kept, else_body: dropped });

          return Ok(());
        }
        other => return error(format!("`{other:?}` is not a closure method")),
      };

      lowering.store(dest, result);

      Ok(())
    })?;
    let else_body = self.buffer(|lowering| {
      let result = match else_method {
        BuiltinMethod::UnwrapOrElse if is_option => call(lowering, Vec::new()),
        BuiltinMethod::UnwrapOrElse => {
          let error_value = lowering.read(err_place.clone());

          call(lowering, vec![error_value])
        }
        BuiltinMethod::MapErr => {
          let error_value = lowering.read(err_place.clone());
          let mapped = call(lowering, vec![error_value]);

          lowering.variant(ty.clone(), 1, vec![mapped])
        }
        _ if is_option => lowering.variant(ty.clone(), 0, Vec::new()),
        _ => {
          let error_value = lowering.read(err_place.clone());

          lowering.variant(ty.clone(), 1, vec![error_value])
        }
      };

      lowering.store(dest, result);

      Ok(())
    })?;

    self.emit(Stmt::If { cond, then_body, else_body });

    Ok(self.owned_result(Some(dest)))
  }
}
