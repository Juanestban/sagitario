//! Closures and function values (docs/spec/08-closures.md, ADR-0033): lambdas with parameter
//! types from the expected function type, results inferred from the body, functions used as
//! values, calls of function values, captures that cannot be assigned, and the methods of
//! `Option` and `Result` that take closures.

use sagitario_ast::{Expr, ExprKind, Ident, LambdaParam, Path};
use sagitario_diagnostics::{Diagnostic, codes};
use sagitario_resolve::{FnId, LocalId, ParamOwner};

use crate::generic::Unknowns;
use crate::{BuiltinMethod, Checker, Receiver, Type};

impl Checker<'_> {
  /// `(params) => body`. Parameter types come from the annotations or the expected function
  /// type; the result from the expected type or else from the body and its `return`s.
  pub(crate) fn lambda(&mut self, expr: &Expr, params: &[LambdaParam], body: &Expr, expected: Option<&Type>) -> Type {
    let Some(info) = self.resolution.lambdas.get(&expr.id).cloned() else { return Type::Error };
    let (mut hint_params, hint_ret) = match expected {
      Some(Type::Function { params, ret }) => (Some(params.clone()), Some(ret.as_ref().clone()).filter(|ret| *ret != Type::Error)),
      _ => (None, None),
    };

    let wrong_arity = hint_params.as_ref().is_some_and(|hint| hint.len() != params.len());

    if let Some(hint) = hint_params.as_ref().filter(|hint| hint.len() != params.len()) {
      self.error(
        codes::MISMATCHED_TYPES,
        expr.span,
        format!("expected a function with {} parameter{}, found a lambda with {}", hint.len(), if hint.len() == 1 { "" } else { "s" }, params.len()),
      );
      hint_params = None;
    }

    let mut param_types = Vec::new();

    for (index, (param, local)) in params.iter().zip(&info.params).enumerate() {
      let hint = hint_params.as_ref().and_then(|hint| hint.get(index)).cloned();
      let ty = match (&param.ty, hint) {
        (Some(written), hint) => {
          let ty = self.type_expr(written);

          if let Some(hint) = hint.filter(|hint| *hint != ty && !matches!(hint, Type::Error | Type::Param(_))) {
            self.mismatch(written.span, &hint, &ty);
          }

          ty
        }
        (None, Some(hint)) => hint,
        // Already reported as a wrong number of parameters.
        (None, None) if wrong_arity => Type::Error,
        (None, None) => {
          self.diagnostics.push(
            Diagnostic::error(codes::UNKNOWN_TYPE, param.name.span, format!("cannot infer the type of the parameter `{}`", param.name.name))
              .with_help(format!("write its type: `({}: Type) => ...`", param.name.name)),
          );

          Type::Error
        }
      };

      self.set_local(*local, ty.clone());
      param_types.push(ty);
    }

    // The body is a function of its own: `return` leaves the lambda, loops do not cross it.
    let inferring = hint_ret.is_none();
    let saved_ret = std::mem::replace(&mut self.ret, hint_ret.clone().unwrap_or(Type::Never));
    let saved_span = self.ret_span.take();
    let saved_inferring = std::mem::replace(&mut self.inferring_ret, inferring);
    let saved_loops = std::mem::take(&mut self.loops);

    self.lambda_stack.push(expr.id);

    let body_ty = self.expr(body, hint_ret.as_ref());
    let ret = if inferring && self.ret == Type::Never {
      if body_ty == Type::Never { Type::Unit } else { body_ty }
    } else {
      let ret = self.ret.clone();

      if !self.fits(&body_ty, &ret) {
        self.mismatch(body.span, &ret, &body_ty);
      }

      ret
    };

    self.lambda_stack.pop();
    self.ret = saved_ret;
    self.ret_span = saved_span;
    self.inferring_ret = saved_inferring;
    self.loops = saved_loops;

    if wrong_arity {
      return Type::Error;
    }

    Type::Function { params: param_types, ret: Box::new(ret) }
  }

  /// A module or static function used as a value; a generic one takes its type arguments from
  /// the path or the expected function type.
  pub(crate) fn fn_value(&mut self, expr: &Expr, path: &Path, function: FnId, expected: Option<&Type>) -> Type {
    let Some(sig) = self.typed.sig(function).cloned() else { return Type::Error };
    let name = self.resolution.fn_decl(self.package, function).map(|decl| decl.name.name.clone()).unwrap_or_default();

    if sig.receiver.is_some() {
      let what = if sig.receiver == Some(Receiver::Init) { "an initializer" } else { "a method" };

      self.diagnostics.push(
        Diagnostic::error(codes::WRONG_CALL_FORM, expr.span, format!("`{name}` is {what}, which cannot be used as a value"))
          .with_help("wrap the call in a lambda: `(x) => value.method(x)`"),
      );

      return Type::Error;
    }

    let value = Type::Function { params: sig.params.clone(), ret: Box::new(sig.ret.clone()) };
    let owner = self.resolution.owner(function);
    let owner_count = owner.map_or(0, |owner| self.generic_count(ParamOwner::Item(owner)));
    let own_count = self.generic_count(ParamOwner::Fn(function));

    if owner_count == 0 && own_count == 0 {
      return value;
    }

    let explicit = self.explicit_args(path);
    let last = path.segments.len().saturating_sub(1);
    let mut unknowns = Unknowns::new();

    if let Some(owner) = owner.filter(|_| owner_count > 0) {
      let given = explicit.as_ref().filter(|(segment, _)| *segment != last).map(|(_, args)| args.clone());

      unknowns.add(ParamOwner::Item(owner), owner_count, given);
    }

    unknowns.add(ParamOwner::Fn(function), own_count, explicit.filter(|(segment, _)| *segment == last).map(|(_, args)| args));

    if let Some(expected) = expected {
      unknowns.unify(&value, expected);
    }

    let subst = self.finish_inference(expr.span, &format!("`{name}`"), unknowns);
    let own = subst.params.iter().find(|(owner, _)| *owner == ParamOwner::Fn(function)).map(|(_, args)| args.clone()).unwrap_or_default();

    self.record_call_args(expr.id, &subst, function, own);

    value.substitute(&subst)
  }

  /// `callee(args)` where the callee is a value of a function type.
  pub(crate) fn call_value(&mut self, expr: &Expr, callee: &Expr, args: &[Expr]) -> Type {
    match self.expr(callee, None) {
      Type::Function { params, ret } => {
        self.args(expr.span, "function", &params, args);

        *ret
      }
      Type::Error | Type::Never => {
        self.unchecked_args(args);

        Type::Error
      }
      other => {
        self.error(codes::WRONG_CALL_FORM, callee.span, format!("cannot call a value of type `{other}`"));
        self.unchecked_args(args);

        Type::Error
      }
    }
  }

  /// `value.name(args)` where `name` is a field of a function type and the type has no method
  /// with that name.
  pub(crate) fn field_call(&mut self, expr: &Expr, field_ty: Type, args: &[Expr]) -> Type {
    let Type::Function { params, ret } = field_ty else { return Type::Error };

    self.args(expr.span, "function", &params, args);
    self.typed.field_calls.insert(expr.id);

    *ret
  }

  /// A closure argument taking `params`, whose result is `ret` when known: returns its result
  /// type.
  fn closure_arg(&mut self, arg: &Expr, params: Vec<Type>, ret: Option<Type>) -> Type {
    let expected = Type::Function { params: params.clone(), ret: Box::new(ret.clone().unwrap_or(Type::Error)) };

    match self.expr(arg, Some(&expected)) {
      Type::Function { params: actual, ret: actual_ret } if actual == params => {
        if let Some(ret) = ret.filter(|ret| !self.fits(&actual_ret, ret)) {
          self.mismatch(arg.span, &Type::Function { params, ret: Box::new(ret) }, &Type::Function { params: actual, ret: actual_ret.clone() });
        }

        *actual_ret
      }
      Type::Error | Type::Never => Type::Error,
      other => {
        self.mismatch(arg.span, &expected, &other);

        Type::Error
      }
    }
  }

  /// The methods of `Option` and `Result` that take a closure (Q66).
  pub(crate) fn closure_method(&mut self, expr: &Expr, receiver_ty: &Type, method: &Ident, args: &[Expr]) -> Option<Type> {
    let (builtin, takes) = match (receiver_ty, method.name.as_str()) {
      (Type::Option(_) | Type::Result(..), "map") => (BuiltinMethod::Map, 1),
      (Type::Result(..), "map_err") => (BuiltinMethod::MapErr, 1),
      (Type::Option(_) | Type::Result(..), "and_then") => (BuiltinMethod::AndThen, 1),
      (Type::Option(_) | Type::Result(..), "unwrap_or_else") => (BuiltinMethod::UnwrapOrElse, 1),
      (Type::Option(_), "filter") => (BuiltinMethod::Filter, 1),
      _ => return None,
    };
    let [arg] = args else {
      self.args(expr.span, "method", &vec![Type::Error; takes], args);

      return Some(Type::Error);
    };
    let ty = match (&builtin, receiver_ty) {
      (BuiltinMethod::Map, Type::Option(value)) => Type::Option(Box::new(self.closure_arg(arg, vec![*value.clone()], None))),
      (BuiltinMethod::Map, Type::Result(value, error)) => Type::Result(Box::new(self.closure_arg(arg, vec![*value.clone()], None)), error.clone()),
      (BuiltinMethod::MapErr, Type::Result(value, error)) => Type::Result(value.clone(), Box::new(self.closure_arg(arg, vec![*error.clone()], None))),
      (BuiltinMethod::AndThen, Type::Option(value)) => match self.closure_arg(arg, vec![*value.clone()], None) {
        ty @ (Type::Option(_) | Type::Error) => ty,
        other => {
          self.error(codes::MISMATCHED_TYPES, arg.span, format!("the closure of `and_then` must return an `Option`, not `{other}`"));

          Type::Error
        }
      },
      (BuiltinMethod::AndThen, Type::Result(value, error)) => match self.closure_arg(arg, vec![*value.clone()], None) {
        Type::Result(next, next_error) if next_error == *error => Type::Result(next, next_error),
        Type::Error => Type::Error,
        other => {
          self.error(
            codes::MISMATCHED_TYPES,
            arg.span,
            format!("the closure of `and_then` must return a `Result` with the error type `{error}`, not `{other}`"),
          );

          Type::Error
        }
      },
      (BuiltinMethod::UnwrapOrElse, Type::Option(value)) => {
        self.closure_arg(arg, Vec::new(), Some(*value.clone()));

        *value.clone()
      }
      (BuiltinMethod::UnwrapOrElse, Type::Result(value, error)) => {
        self.closure_arg(arg, vec![*error.clone()], Some(*value.clone()));

        *value.clone()
      }
      (BuiltinMethod::Filter, Type::Option(value)) => {
        self.closure_arg(arg, vec![*value.clone()], Some(Type::Bool));

        receiver_ty.clone()
      }
      _ => Type::Error,
    };

    self.typed.builtin_methods.insert(expr.id, builtin);

    Some(ty)
  }

  /// The type of a field of a function type of a struct or class type (inherited fields
  /// included).
  pub(crate) fn function_field(&self, receiver_ty: &Type, name: &str) -> Option<Type> {
    let ty = match receiver_ty {
      Type::Struct(inner) => {
        let index = self.typed.field_index(inner.id, name)?;

        self.typed.struct_field_types(inner).into_iter().nth(index)?
      }
      Type::Class(inner) => self.typed.class_field_type(inner, name)?.2,
      _ => return None,
    };

    matches!(ty, Type::Function { .. }).then_some(ty)
  }

  /// Whether a binding is captured by the lambda being checked (declared outside it).
  pub(crate) fn is_captured(&self, local: LocalId) -> bool {
    self.lambda_stack.last().and_then(|lambda| self.resolution.lambdas.get(lambda)).is_some_and(|info| !info.owns(local))
  }

  /// The error for modifying a captured binding.
  pub(crate) fn captured_error(&self, span: sagitario_span::Span, name: &str) -> Diagnostic {
    Diagnostic::error(codes::CAPTURED_ASSIGNMENT, span, format!("cannot modify `{name}`: closures capture bindings as read-only copies"))
      .with_help("compute the value with `fold`/`reduce`, or keep it in a `var` field of a class instance")
  }

  /// Warns when a closure capturing a binding is stored in a field of the object that binding
  /// refers to: the object and the closure keep each other alive (Q65).
  pub(crate) fn check_closure_cycle(&mut self, target: &Expr, value: &Expr) {
    let (ExprKind::Field { base, .. }, ExprKind::Lambda { .. }) = (&target.kind, &value.kind) else { return };
    let (Some(local), Some(Type::Class(_))) = (self.local(base), self.typed.expr_types.get(&base.id)) else { return };
    let Some(info) = self.resolution.lambdas.get(&value.id) else { return };

    if !info.captures.contains(&local) {
      return;
    }

    let name = self.resolution.locals(self.function).get(local.0 as usize).map(|info| info.name.clone()).unwrap_or_default();

    self.diagnostics.push(
      Diagnostic::warning(codes::CLOSURE_CYCLE, value.span, format!("this closure captures `{name}` and is stored in it, so neither is ever freed"))
        .with_note("the object keeps the closure alive, and the closure keeps the object alive")
        .with_help("pass what the closure needs as parameters instead of capturing the object"),
    );
  }
}
