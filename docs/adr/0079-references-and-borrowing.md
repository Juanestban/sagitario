# 0079. References and borrowing in parameters

- Status: Proposed
- Date: 2026-11-09
- Refines: [ADR-0002](0002-arc-memory-model.md), [ADR-0063](0063-distribution-phase-and-roadmap-v2.md)
  (slices and byte buffers in G1 of roadmap v2)

## Context

Sagitario manages memory with ARC (ADR-0002): structs, arrays and enums are values, copied when
passed; classes, lists and closures are references, retained when passed and released after.
That is safe and simple, and keeps the object-oriented and functional styles of the language:
graphs of objects that point to each other, inheritance, closures that capture.

It also costs what Rust does not pay. Passing a `Mat4` (64 bytes) or a large struct copies it, and
a function cannot change the caller's value except through a method replacing `self`; passing a
class instance retains and releases it, atomically in programs with threads. A game engine does
both thousands of times per frame. And there is no way to give a function a view of part of an
array or list without copying it.

Rust avoids these costs with references (`&T`, `&mut T`) checked by the borrow checker: many
shared references or one mutable one, never outliving their value. Its full form, with lifetimes
in structs and results, is what makes graphs of shared mutable objects hard in Rust, which is why
it favors composition over the object-oriented style. Swift (`borrowing`, `inout`), C# (`in`,
`ref`) and Hylo show a smaller form that keeps ARC or a garbage collector: references that only
live while a call does. That form needs no lifetimes and leaves classes as they are.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q488 | The model | References as Rust writes them, `&T` and `&mut T`, only as **types of parameters** (of functions, methods, `extern "C" fn` items, lambdas and function types): they live while the call does. They cannot be the type of a field, a result, a binding, an element or a type argument (`List<&T>`, `Option<&T>`), so there are no lifetimes to write. ARC, classes, inheritance, interfaces, `weak` and closures keep their rules. Stored and returned references, with lifetimes, may come in a later record. |
| Q489 | At the call | The caller writes the borrow, as in Rust: `f(&x)` lends `x`, `f(&mut x)` lends it to be changed; a value is never borrowed implicitly. `&expr` of a value that is not a place (a call, a literal) lends a temporary that lives until the call returns; `&mut` needs a place that could be assigned: one of a `var` binding or of a `&mut` parameter, or a `var` field of a class instance. A reference parameter is passed on as it is (`g(v)`), and a `&mut T` where a `&T` is expected. |
| Q490 | Inside the callee | Fields and methods are used through a reference directly (`v.x`, `v.length()`), as in Rust; the whole value is read with `*v` (a copy) and replaced with `*v = value`, both safe. Through a `&T` nothing can be changed; through a `&mut T` fields can be assigned and methods that change `self` called. |
| Q491 | Places that can be borrowed | A binding or parameter, then fields of structs and elements of fixed arrays, as in Rust. A field of a class instance may be borrowed too when its type holds no ARC references (numbers, `bool`, raw pointers, and structs and arrays of those): the compiler keeps the instance alive until the call returns. Fields holding ARC references (a `String`, a class, a list) cannot be borrowed through a class instance, since other references to the instance could replace them while borrowed: copy them into a binding first (an error, with that `help`). |
| Q492 | The borrow checker | In one call, a place lent with `&mut` cannot also be lent with `&` or `&mut`, or be passed by value, unless the places are disjoint: different fields of the same struct, as in Rust; two elements of an array are not disjoint (`f(&mut a[i], &a[j])` is an error). Through class instances the checker cannot see every alias (another reference may reach the same instance), so there exclusivity is not guaranteed, as with C#'s `ref`: aliasing only shows changes, it never frees memory in use (Q491). |
| Q493 | Escaping | A reference parameter cannot be captured by a lambda (lambdas capture by value and outlive the call) nor used by an `async fn` or an `async` method, whose body outlives the call; both are errors, with the `help` of copying the value (`const copy = *v;`). |
| Q494 | Slices | `&[T]` and `&mut [T]`, parameter types like references, are views of a run of elements: `&array`, `&list`, `&array[a..b]`, `&list[a..b]` (and `&mut`). They have `len()`, `is_empty()`, indexing checked as arrays are (`&mut` also assigns), `iter()`, `as_ptr()` (and `as_mut_ptr()` through `&mut`) and sub-slices `s[a..b]` passed on. A range out of bounds panics. |
| Q495 | Borrowed lists | Lists are shared, so while a list is lent as a slice another reference could grow it and move its elements. A list counts the slices lent from it; `push`, `insert`, `remove`, `pop`, `clear`, `resize` and the other methods that change its length panic while it is lent (`cannot change the length of a list while it is borrowed`), as Rust's `RefCell` does. Changing its elements in place stays allowed. |
| Q496 | The FFI | A `&T` passes where a `*const T` is expected and a `&mut T` where a `*mut T` is, as in Rust: `vkCreateInstance(&info, ...)`. A reference parameter of an `extern "C" fn` is a non-null pointer for C. |
| Q497 | Bytes | With slices, `fs::write` gains its form for bytes: `fs::write_bytes(path, bytes: &[u8])`, the counterpart of `fs::read` ([ADR-0078](0078-ffi-without-c-shims.md), Q483). |
| Q498 | The C code | A `&T` is a `const T *` and a `&mut T` a `T *`; a slice, a pointer and a length. Borrowing costs nothing at run time except Q491's retain of an instance and Q495's count of a list. |
| Q499 | Increments | **1** `&T` and `&mut T` in parameters, places, the borrow checker of Q492 and the errors of Q493; **2** slices and borrowed lists; **3** `fs::write_bytes` and the uses in the standard library and the compiler where they remove copies. Each updates the spec, with `run-pass` cases and a `compile-fail` case per rule; the borrow checker has its own pass after the type checker. |
| Q500 | Documents | This record, `docs/spec/03-memory.md` (references and borrowing), `docs/spec/02-types.md` (slices), `docs/spec/08-closures.md` (captures), `docs/spec/12-async.md`, `docs/spec/10-ffi.md` and `docs/spec/11-standard-library.md`, and `docs/roadmap-v2.md`. |

## Alternatives considered

- **Rust's full model, with lifetimes and stored references:** the most expressive, but it
  needs lifetimes in the types of structs and results, and clashes with graphs of shared objects,
  the core of the object-oriented style; Q488 leaves room to add it later.
- **Swift's keywords (`borrowing`, `inout`) instead of `&`:** the same model, but Rust is the
  reference of the language, and its marks at the call (`&mut x`) show what may change.
- **Implicit borrowing of large structs by the compiler:** removes copies without syntax, but
  hides what a function may change, and gives no `&mut`.
- **Dynamic exclusivity for class fields (Swift):** a check on every access to a borrowed
  instance; Q491 and Q492 instead keep ARC fields out of borrows, so aliasing never frees memory.
- **Slices of lists without a borrow count:** a `push` from another reference would leave the
  slice pointing at freed memory.

## Consequences

- Functions of engines and libraries take large values and buffers without copies and without
  reference counting, and change the caller's values without returning them.
- Classes, lists and closures behave as before; code that does not write `&` does not change.
- The compiler gains reference and slice types, a borrow checker pass, and the borrow count of
  lists in the runtime.
