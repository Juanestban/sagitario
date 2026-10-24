//! Lowering of lists, iterators and the prelude interfaces on primitives
//! (docs/spec/02-types.md#arrays-and-lists, docs/spec/08-closures.md#iterators, ADR-0034).
//!
//! A `List<T>` is an ARC object holding its elements; an element is a place behind the list
//! (`ListElem`), read like a field of a class instance. `for` over a list reads its length at
//! every step, and over an iterator calls `next` until it returns `None`; the loop variable owns
//! its value. Methods implemented by prelude functions are calls of their instances.

use sagitario_ast::{BinaryOp, Block, Expr, ExprKind};
use sagitario_resolve::{FnId, ParamOwner, Res, TypeItem};
use sagitario_types::{BuiltinMethod, IntTy, ListMethod, Subst, Type};

use crate::{Callee, Const, FnLowering, LResult, Local, LowerError, Operand, Place, Projection, Rvalue, Stmt, error};

const USIZE: Type = Type::Int(IntTy::Usize);

fn usize_const(value: u128) -> Operand {
  Operand::Const(Const::Int { value, negative: false, ty: IntTy::Usize })
}

fn runtime(function: &'static str) -> Callee {
  Callee::Runtime { function, located: false }
}

impl FnLowering<'_> {
  /// A value that stays valid while the rest of the expression runs: a list or instance read
  /// from a field (which a call could replace) is retained for the statement.
  fn stable(&mut self, ty: Type, value: Operand) -> Operand {
    match value {
      Operand::Place(_) => self.owned_copy(ty, value),
      other => other,
    }
  }

  /// `List::from(array)`: the elements are retained for the new list.
  pub(crate) fn list_from(&mut self, ty: &Type, args: &[Expr]) -> LResult<Operand> {
    let [array] = args else { return error("`List::from` without one argument") };
    let array_ty = self.type_of(array)?;
    let value = self.expr(array)?;

    Ok(self.list_from_array(ty.clone(), &array_ty, value))
  }

  fn list_from_array(&mut self, list_ty: Type, array_ty: &Type, value: Operand) -> Operand {
    if self.managed(array_ty) {
      self.emit(Stmt::Retain(value.clone()));
    }

    self.new_value(list_ty, Rvalue::ListFromArray { array: value, array_ty: array_ty.clone() })
  }

  /// The place of `list[index]`.
  pub(crate) fn list_elem_place(&mut self, base: &Expr, index: &Expr, ty: Type) -> LResult<Place> {
    let list_ty = self.type_of(base)?;
    let list = self.expr(base)?;
    let list = self.stable(list_ty.clone(), list);
    let index = self.expr(index)?;
    let mut place = self.operand_place(list, list_ty)?;

    place.projections.push(Projection::ListElem { index, elem: ty.clone() });
    place.ty = ty;

    Ok(place)
  }

  /// The element at `index` of `list` (of element type `elem`) as a new place.
  fn elem_of(&mut self, list: &Operand, list_ty: &Type, index: Operand, elem: &Type) -> LResult<Place> {
    let mut place = self.operand_place(list.clone(), list_ty.clone())?;

    place.projections.push(Projection::ListElem { index, elem: elem.clone() });
    place.ty = elem.clone();

    Ok(place)
  }

  fn list_len(&mut self, list: &Operand) -> Operand {
    self.new_value(USIZE, Rvalue::ListLen(list.clone()))
  }

  /// A method of `List` implemented by the compiler.
  pub(crate) fn list_method(&mut self, method: ListMethod, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let list_ty = self.type_of(receiver)?;
    let Type::List(elem) = &list_ty else { return error("list method on a value that is not a list") };
    let elem = elem.as_ref().clone();
    let list = self.expr(receiver)?;
    let list = self.stable(list_ty.clone(), list);
    let mut operands = Vec::new();
    let params = match method {
      ListMethod::Push => vec![elem.clone()],
      ListMethod::Get | ListMethod::Remove => vec![USIZE],
      ListMethod::Insert => vec![USIZE, elem.clone()],
      _ => Vec::new(),
    };

    for (arg, param) in args.iter().zip(&params) {
      let value = self.operand_as(arg, param)?;

      operands.push(self.owned_copy(param.clone(), value));
    }

    match method {
      ListMethod::Len => Ok(self.list_len(&list)),
      ListMethod::IsEmpty => {
        let len = self.list_len(&list);

        Ok(self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Eq, ty: USIZE, lhs: len, rhs: usize_const(0) }))
      }
      ListMethod::Push | ListMethod::Insert => {
        let value = operands.pop().ok_or_else(|| LowerError("list method without its value".to_string()))?;

        // The list takes a reference.
        if self.managed(&elem) {
          self.emit(Stmt::Retain(value.clone()));
        }

        match operands.pop() {
          Some(index) => self.emit(Stmt::ListInsert { list, index, value }),
          None => self.emit(Stmt::ListPush { list, value }),
        }

        Ok(crate::UNIT)
      }
      ListMethod::Pop => {
        let len = self.list_len(&list);
        let cond = self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Gt, ty: USIZE, lhs: len, rhs: usize_const(0) });
        let dest = self.temp(ty.clone());
        let then_body = self.buffer(|lowering| {
          // The element moves out of the list into the `Some`.
          let value = lowering.temp(elem.clone());

          lowering.emit(Stmt::Assign { dest: value, value: Rvalue::ListPop(list.clone()) });
          lowering.emit(Stmt::Assign { dest, value: Rvalue::Variant { index: 1, fields: vec![Operand::Local(value)] } });

          Ok(())
        })?;
        let else_body = self.buffer(|lowering| {
          lowering.emit(Stmt::Assign { dest, value: Rvalue::Variant { index: 0, fields: Vec::new() } });

          Ok(())
        })?;

        self.emit(Stmt::If { cond, then_body, else_body });

        Ok(self.owned_result(Some(dest)))
      }
      ListMethod::Get | ListMethod::First | ListMethod::Last => {
        let len = self.list_len(&list);
        let index = match method {
          ListMethod::Get => Some(operands.pop().ok_or_else(|| LowerError("`get` without an index".to_string()))?),
          ListMethod::First => Some(usize_const(0)),
          // `len - 1`, computed once the list is known not to be empty.
          _ => None,
        };
        let cond = match &index {
          Some(index) if method == ListMethod::Get => Rvalue::Binary { op: BinaryOp::Lt, ty: USIZE, lhs: index.clone(), rhs: len.clone() },
          _ => Rvalue::Binary { op: BinaryOp::Gt, ty: USIZE, lhs: len.clone(), rhs: usize_const(0) },
        };
        let cond = self.new_value(Type::Bool, cond);
        let dest = self.temp(ty.clone());
        let then_body = self.buffer(|lowering| {
          let index = match index {
            Some(index) => index,
            None => lowering.new_value(USIZE, Rvalue::Binary { op: BinaryOp::Sub, ty: USIZE, lhs: len, rhs: usize_const(1) }),
          };
          let place = lowering.elem_of(&list, &list_ty, index, &elem)?;
          let some = lowering.variant(ty.clone(), 1, vec![Operand::Place(place)]);

          lowering.store_moved(dest, some);

          Ok(())
        })?;
        let else_body = self.buffer(|lowering| {
          lowering.emit(Stmt::Assign { dest, value: Rvalue::Variant { index: 0, fields: Vec::new() } });

          Ok(())
        })?;

        self.emit(Stmt::If { cond, then_body, else_body });

        Ok(self.owned_result(Some(dest)))
      }
      ListMethod::Remove => {
        let index = operands.pop().ok_or_else(|| LowerError("`remove` without an index".to_string()))?;

        Ok(self.new_value(elem, Rvalue::ListRemove { list, index }))
      }
      ListMethod::Clear => {
        self.emit(Stmt::ListClear(list));

        Ok(crate::UNIT)
      }
      ListMethod::Reverse => {
        self.emit(Stmt::ListReverse(list));

        Ok(crate::UNIT)
      }
    }
  }

  /// Moves a new value (a pending temporary) into `dest`, which takes over its reference.
  fn store_moved(&mut self, dest: Local, value: Operand) {
    if let Operand::Local(local) = &value
      && let Some(position) = self.pending.iter().rposition(|pending| pending == local)
    {
      self.pending.remove(position);
      self.emit(Stmt::Assign { dest, value: Rvalue::Use(value) });

      return;
    }

    self.store(dest, value);
  }

  /// A method implemented by the prelude function `function`, called with the receiver first.
  pub(crate) fn prelude_method(&mut self, expr: &Expr, function: FnId, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let subst = self.call_subst(expr.id, function, None);
    let params = self.typed.sig(function).map(|sig| sig.substitute(&subst).params).unwrap_or_default();
    let target = self.instance(function, subst);
    let receiver_ty = self.type_of(receiver)?;
    let mut value = self.expr(receiver)?;

    // Arrays become lists.
    if let (Type::Array(..), Some(list_ty)) = (&receiver_ty, params.first()) {
      value = self.list_from_array(list_ty.clone(), &receiver_ty, value);
    }

    let mut operands = vec![value];

    for (arg, param) in args.iter().zip(params.iter().skip(1)) {
      operands.push(self.operand_as(arg, param)?);
    }

    Ok(self.call(Callee::Fn(target), operands, ty.clone()))
  }

  /// The receiver of an `Iter` method called on a list or array, as an iterator.
  pub(crate) fn iter_of(&mut self, receiver: &Expr, to_iter: FnId, args: Vec<Type>) -> LResult<Operand> {
    let subst = Subst::of(ParamOwner::Fn(to_iter), args);
    let params = self.typed.sig(to_iter).map(|sig| sig.substitute(&subst).params).unwrap_or_default();
    let ret = self.typed.sig(to_iter).map(|sig| sig.ret.substitute(&subst)).unwrap_or(Type::Error);
    let target = self.instance(to_iter, subst);
    let receiver_ty = self.type_of(receiver)?;
    let mut value = self.expr(receiver)?;

    if let (Type::Array(..), Some(list_ty)) = (&receiver_ty, params.first()) {
      value = self.list_from_array(list_ty.clone(), &receiver_ty, value);
    }

    Ok(self.call(Callee::Fn(target), vec![value], ret))
  }

  /// `start..end` used as a value, or reversed with `rev()`: the iterator of the prelude's
  /// range function for its integer type.
  pub(crate) fn range_iter(&mut self, range: &Expr, reverse: bool, ty: &Type) -> LResult<Operand> {
    let ExprKind::Range { start, end, inclusive } = &range.kind else { return error("`rev` of something other than a range") };
    let function = self.typed.ranges.get(&range.id).copied().ok_or_else(|| LowerError("range without its function".to_string()))?;
    let target = self.instance(function, Subst::default());
    let int = self.type_of(start)?;
    let start = self.operand_as(start, &int)?;
    let end = self.operand_as(end, &int)?;

    Ok(self.call(Callee::Fn(target), vec![start, end, Operand::Const(Const::Bool(*inclusive)), Operand::Const(Const::Bool(reverse))], ty.clone()))
  }

  /// `a.total_compare(b)` on floats.
  pub(crate) fn total_compare(&mut self, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let float = self.type_of(receiver)?;
    let [other] = args else { return error("`total_compare` without one argument") };
    let lhs = self.expr(receiver)?;
    let rhs = self.operand_as(other, &float)?;
    let function = if float == Type::Float(sagitario_types::FloatTy::F32) { "sag_f32_total_compare" } else { "sag_f64_total_compare" };
    let order = self.new_value(Type::Int(IntTy::I32), Rvalue::Call { callee: runtime(function), args: vec![lhs, rhs] });

    self.ordering(ty, order)
  }

  /// An `Ordering` from the sign of `order` (an `i32`): `Less` (0), `Equal` (1) or `Greater` (2).
  fn ordering(&mut self, ty: &Type, order: Operand) -> LResult<Operand> {
    let zero = Operand::Const(Const::Int { value: 0, negative: false, ty: IntTy::I32 });
    let dest = self.temp(ty.clone());
    let less = self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Lt, ty: Type::Int(IntTy::I32), lhs: order.clone(), rhs: zero.clone() });
    let equal = self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Eq, ty: Type::Int(IntTy::I32), lhs: order, rhs: zero });
    let set = |index: u32| vec![Stmt::Assign { dest, value: Rvalue::Variant { index, fields: Vec::new() } }];

    self.emit(Stmt::If { cond: less, then_body: set(0), else_body: vec![Stmt::If { cond: equal, then_body: set(1), else_body: set(2) }] });

    Ok(Operand::Local(dest))
  }

  /// Whether `function` is a method of the prelude's `Eq`, `Ord` or `Hash`, called on an
  /// integer, `bool`, `char` or `String`, which implement them without methods of their own.
  pub(crate) fn is_primitive_method(&self, function: FnId, receiver_ty: &Type) -> bool {
    let prelude = &self.typed.prelude;

    matches!(receiver_ty, Type::Int(_) | Type::Bool | Type::Char | Type::String)
      && matches!(self.resolution.owner(function), Some(TypeItem::Interface(id)) if [prelude.eq, prelude.ord, prelude.hash].contains(&Some(id)))
  }

  /// `equals`, `compare` or `hash` on an integer, `bool`, `char` or `String`.
  pub(crate) fn primitive_method(&mut self, function: FnId, ty: &Type, receiver: &Expr, args: &[Expr]) -> LResult<Operand> {
    let receiver_ty = self.type_of(receiver)?;
    let lhs = self.expr(receiver)?;
    let interface = match self.resolution.owner(function) {
      Some(TypeItem::Interface(id)) => Some(id),
      _ => None,
    };

    if interface == self.typed.prelude.hash {
      return Ok(match receiver_ty {
        Type::String => self.new_value(Type::Int(IntTy::U64), Rvalue::Call { callee: runtime("sag_string_hash"), args: vec![lhs] }),
        _ => {
          let bits = self.new_value(Type::Int(IntTy::U64), Rvalue::Cast { from: receiver_ty, to: Type::Int(IntTy::U64), operand: lhs });

          self.new_value(Type::Int(IntTy::U64), Rvalue::Call { callee: runtime("sag_hash_u64"), args: vec![bits] })
        }
      });
    }

    let [other] = args else { return error("comparison without one argument") };
    let rhs = self.operand_as(other, &receiver_ty)?;

    if interface == self.typed.prelude.eq {
      return Ok(self.binary(BinaryOp::Eq, receiver_ty, lhs, rhs));
    }

    let order = match receiver_ty {
      Type::String => self.new_value(Type::Int(IntTy::I32), Rvalue::Call { callee: runtime("sag_string_cmp"), args: vec![lhs, rhs] }),
      _ => {
        // -1, 0 or 1 from two comparisons.
        let less = self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Lt, ty: receiver_ty.clone(), lhs: lhs.clone(), rhs: rhs.clone() });
        let greater = self.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Gt, ty: receiver_ty, lhs, rhs });
        let less = self.new_value(Type::Int(IntTy::I32), Rvalue::Cast { from: Type::Bool, to: Type::Int(IntTy::I32), operand: less });
        let greater = self.new_value(Type::Int(IntTy::I32), Rvalue::Cast { from: Type::Bool, to: Type::Int(IntTy::I32), operand: greater });

        self.new_value(Type::Int(IntTy::I32), Rvalue::Binary { op: BinaryOp::Sub, ty: Type::Int(IntTy::I32), lhs: greater, rhs: less })
      }
    };

    self.ordering(ty, order)
  }

  /// `for var in iter` over a list, an iterator or a map; `false` for other iterables.
  pub(crate) fn for_collection(&mut self, var: Local, iter: &Expr, body: &Block) -> LResult<bool> {
    let iter_ty = self.type_of(iter)?;

    if let Type::List(elem) = &iter_ty {
      self.for_list(var, iter, &iter_ty, elem, body)?;

      return Ok(true);
    }

    let source = if let Some((key, value)) = self.typed.map_args(&iter_ty) {
      // The entries of a map.
      let entry = self.typed.entry_type(key, value);
      let iter_of_entries = self.typed.iter_type(entry);
      let map = self.expr(iter)?;
      let Type::Class(class) = &iter_ty else { return error("map that is not a class") };
      let entries = self.resolution.type_function(self.package, TypeItem::Class(class.id), "entries");
      let entries = entries.ok_or_else(|| LowerError("`Map` without `entries`".to_string()))?;
      let target = self.instance(entries, Subst::of(ParamOwner::Item(TypeItem::Class(class.id)), class.args.clone()));

      self.call(Callee::Fn(target), vec![map], iter_of_entries)
    } else if self.typed.iter_elem(&iter_ty).is_some() {
      self.expr(iter)?
    } else {
      return Ok(false);
    };

    self.for_iter(var, source, body)?;

    Ok(true)
  }

  /// The loop over a list: the length is read at every step, and `var` owns a copy of each
  /// element.
  fn for_list(&mut self, var: Local, iter: &Expr, list_ty: &Type, elem: &Type, body: &Block) -> LResult<()> {
    let value = self.expr(iter)?;
    let list = self.temp(list_ty.clone());

    // The list stays alive during the loop.
    self.store(list, value);
    self.pending.push(list);

    let index = self.temp(USIZE);

    self.emit(Stmt::Assign { dest: index, value: Rvalue::Use(usize_const(0)) });

    let list = Operand::Local(list);
    let loop_body = self.loop_buffer(None, |lowering| {
      let len = lowering.new_value(USIZE, Rvalue::ListLen(list.clone()));
      let more = lowering.new_value(Type::Bool, Rvalue::Binary { op: BinaryOp::Lt, ty: USIZE, lhs: Operand::Local(index), rhs: len });

      lowering.emit(Stmt::If { cond: more, then_body: Vec::new(), else_body: vec![Stmt::Break] });

      let place = lowering.elem_of(&list, list_ty, Operand::Local(index), elem)?;

      lowering.store(var, Operand::Place(place));
      lowering
        .emit(Stmt::Assign { dest: index, value: Rvalue::Binary { op: BinaryOp::Add, ty: USIZE, lhs: Operand::Local(index), rhs: usize_const(1) } });
      lowering.loop_var_body(var, body)
    })?;

    self.emit(Stmt::Loop { body: loop_body });

    Ok(())
  }

  /// The loop over an iterator: `next` is called until it returns `None`, and `var` takes the
  /// value inside each `Some`.
  fn for_iter(&mut self, var: Local, source: Operand, body: &Block) -> LResult<()> {
    let Operand::Local(source_local) = source else { return error("iterator without a local") };
    let iter_ty = self.local_type(source_local);
    let Type::Class(class) = &iter_ty else { return error("iterator that is not a class") };
    let elem = class.args.first().cloned().unwrap_or(Type::Error);
    let iter = self.temp(iter_ty.clone());

    // The iterator stays alive during the loop.
    self.store(iter, source);
    self.pending.push(iter);

    let next = self.resolution.type_function(self.package, TypeItem::Class(class.id), "next");
    let next = next.ok_or_else(|| LowerError("`Iter` without `next`".to_string()))?;
    let target = self.instance(next, Subst::of(ParamOwner::Item(TypeItem::Class(class.id)), class.args.clone()));
    let option_ty = Type::Option(Box::new(elem.clone()));
    let loop_body = self.loop_buffer(None, |lowering| {
      // Not pending: the value moves into `var`.
      let option = lowering.temp(option_ty.clone());

      lowering.emit(Stmt::Assign { dest: option, value: Rvalue::Call { callee: Callee::Fn(target), args: vec![Operand::Local(iter)] } });

      let place = Place { local: option, projections: Vec::new(), ty: option_ty.clone() };
      let more = lowering.tag_is(&place, 1)?;

      lowering.emit(Stmt::If { cond: more, then_body: Vec::new(), else_body: vec![Stmt::Break] });

      let mut value = place;

      value.projections.push(Projection::VariantField { variant: 1, field: 0 });
      value.ty = elem.clone();
      lowering.emit(Stmt::Assign { dest: var, value: Rvalue::Use(Operand::Place(value)) });
      lowering.loop_var_body(var, body)
    })?;

    self.emit(Stmt::Loop { body: loop_body });

    Ok(())
  }

  /// The body of a loop whose variable owns its value: released at the end of each iteration,
  /// and by `break` and `continue`.
  fn loop_var_body(&mut self, var: Local, body: &Block) -> LResult<()> {
    let owned = if self.managed(&self.local_type(var)) { vec![var] } else { Vec::new() };

    self.scopes.push(owned);
    self.block_into(body, None)?;

    for local in self.scopes.pop().unwrap_or_default() {
      self.emit(Stmt::Release(local));
    }

    Ok(())
  }

  /// `for var in (a..b).rev()` or `(a..b).step_by(n)`: a counting loop, without an iterator;
  /// `false` for other iterables.
  pub(crate) fn for_range_method(&mut self, var: Local, iter: &Expr, body: &Block) -> LResult<bool> {
    let ExprKind::MethodCall { receiver, method, args } = &iter.kind else { return Ok(false) };
    let ExprKind::Range { start, end, inclusive } = &receiver.kind else { return Ok(false) };
    let reverse = matches!(self.typed.builtin_methods.get(&iter.id), Some(BuiltinMethod::RangeRev));
    let step_by = method.name == "step_by" && self.typed.method_calls.contains_key(&iter.id);

    if !reverse && !step_by {
      return Ok(false);
    }

    let Type::Int(int) = self.type_of(start)? else { return error("range without an integer type") };
    let start = self.operand_as(start, &Type::Int(int))?;
    let start = self.new_value(Type::Int(int), Rvalue::Use(start));
    let end = self.operand_as(end, &Type::Int(int))?;
    let end = self.new_value(Type::Int(int), Rvalue::Use(end));
    let step = match args.first() {
      Some(step) if step_by => {
        let step = self.operand_as(step, &USIZE)?;

        Some(self.new_value(USIZE, Rvalue::Use(step)))
      }
      _ => None,
    };
    let loop_body = self.loop_buffer(None, |lowering| lowering.block_into(body, None))?;

    self.emit(Stmt::ForRange { var, ty: int, start, end, inclusive: *inclusive, reverse, step, body: loop_body });

    Ok(true)
  }

  /// Whether a call expression is `List::from(...)`.
  pub(crate) fn is_list_from(&self, callee: &Expr) -> bool {
    matches!(self.resolution.paths.get(&callee.id), Some(Res::Builtin(sagitario_resolve::Builtin::ListFrom)))
  }
}
