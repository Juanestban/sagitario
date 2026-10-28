# 0074. `async` methods, generic `async fn` and `async fn` as values

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0072](0072-async-await-increments.md) (increment 3 of phase F8, Q418)

## Context

Increment 1 ([ADR-0072](0072-async-await-increments.md)) allowed `async` only on functions of a
module, not generic and only called: an `async` method, a generic `async fn` and an `async fn`
used as a value were `E0900`. The lowering of increment 1 already makes a frame out of any list of
parameters and any substitution of type arguments, so generic functions and function values need
no new mechanism. Methods do: the `self` of a struct or an enum is a value of the caller, which a
method that modifies it receives by reference (ADR-0028), and a task may outlive both that value
and the call.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q439 | Methods | Methods of classes, structs, enums and interfaces, also `abstract`, `override` and static ones, may be `async`. An initializer (`new`) cannot (`E0337`). |
| Q440 | `self` | The task keeps `self` as it keeps the parameters: a reference to the instance for a class, whose fields an `async` method may modify; a copy of the value for a struct or an enum, and for `Self` in a default method of an interface. Such a method cannot modify `self`, directly or by calling a method that does (`E0338`): the change would be lost, since the caller's value is not the task's. |
| Q441 | Signatures | An `async fn` returning `T` has the type of a function returning `Task<T>`. Implementing an interface method or overriding a method compares those types only: `async fn f(self) -> T` and `fn f(self) -> Task<T>` implement and override each other. An `abstract` or interface method may be declared `async` without a body. |
| Q442 | Default methods | A default method of an interface may be `async`, with the rule of Q440 for `self`: assigning to its fields is `E0338`. Calling another method of the interface on `self` cannot be checked, since whether it modifies `self` depends on each implementing type: with a struct or an enum, it changes the task's copy only. |
| Q443 | Generic `async fn` | Allowed, with bounds as any generic function: each instance has its own frame. |
| Q444 | As values | An `async fn` (or a static `async` function) used as a value has the type `fn(A, ...) -> Task<T>`; calling the value starts the task, as calling the function does. Methods remain values only through lambdas, as before. |
| Q445 | Not in F8 | `async` lambdas ([ADR-0048](0048-async-await.md), Q203) and cancellation stay out of F8. |
| Q446 | Tests | `run-pass` cases for `async` methods of classes (modifying fields), structs and enums (on copies), interfaces as types with `async` methods, a default `async` method, `override` across `async fn` and `fn -> Task<T>`, generic `async fn` with bounds, and `async fn` passed as values; `compile-fail` cases for `E0337` and `E0338`. |
| Q447 | Documents | This record, `docs/spec/12-async.md`, and the roadmap. |

## Alternatives considered

- **`async` methods only in classes:** structs and enums are common receivers, and a copy is what
  a value type gives in C# (`async` methods of a `struct` work on a copy of it as well).
- **Keeping the caller's reference, as Rust's `&mut self` futures:** without lifetimes, nothing
  stops the caller from changing or releasing the value while the task runs.
- **Requiring the same `async` in implementations:** the caller only sees a `Task<T>`; an
  implementation that returns another call's task without awaiting it is legitimate, as in C# and
  TypeScript.

## Consequences

- With increment 3, `async` reaches every kind of function except lambdas and initializers.
- Increment 4 (the acceptance program and closing F8) remains.
