# 0031. Interfaces: fields, implementation, interface values and `Error`

- Status: Accepted; completed by [ADR-0032](0032-generics.md) (Q47: defaults are now instantiated per type)
- Date: 2026-10-23
- Refines: [ADR-0004](0004-class-model.md) (interfaces) and [ADR-0005](0005-result-error-handling.md)
  (the `Error` interface)

## Context

docs/spec/05-interfaces-generics.md already defined:
- interfaces with default methods;
- `extends` between interfaces;
- `impl` on classes, structs and enums;
- `Self` as the implementing type;
- interfaces as types with dynamic dispatch, except those that mention `Self`;
- an `Error` interface for error types.

F2 increment 4 implements them. It also settles questions Q42–Q49, including two changes the
project asked for: interfaces describe required **fields** as well as methods, as in
TypeScript, and errors follow Rust.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q42 | Members | An interface declares fields (`const name: T;` can only be read through the interface, `var name: T;` can also be assigned) and methods taking `self`, required (no body) or default (with body). Members take no `pub`/`prv`: they are public. There are no static functions. Generic interfaces arrive with generics (F2 increment 5). |
| Q43 | Implementing | A type implements a method with a plain `pub fn` of the same name and signature, with `Self` replaced by the type; no `override`, which stays for class inheritance. Redefining a default method is also a plain `pub fn`. Each interface field needs a `pub` field of the same name and type, `var` in a class when the interface's is `var` (struct fields are always assignable). Enums have no fields, so they implement only field-less interfaces. Missing or mismatched members are E0329. |
| Q44 | Inheritance | A subclass inherits its parent's interfaces, and inherited methods and fields count as implementations. An `abstract` class may leave required methods to its concrete subclasses. |
| Q45 | Structs and enums as interface values | Converting a struct or enum to an interface copies it into an ARC box. An interface value is always a reference: copies share the box, and assigning a `var` field through it changes the box, not the original. |
| Q46 | Conversions and equality | A class, struct or enum converts implicitly to an interface it implements, and an interface to an ancestor interface, wherever the target type is expected (the rules of ADR-0029, Q34). There is no conversion back. `==` is not defined between interface values in the MVP (E0310). |
| Q47 | Default methods | A default method is checked once, with `self` of the interface type. It is compiled once with dynamic dispatch, and calling it on a concrete value first converts the value to the interface. Specializing defaults per type, a pure optimization with the same behavior, arrives with monomorphization (increment 5). Until then, interfaces that mention `Self` cannot have default methods (E0900). |
| Q48 | `Error` | As in Rust: `Result<T, E>` admits any `E`. `Error` is a prelude interface with `fn message(self) -> String`, and `Error("text")` creates a basic error with that message. `String` does not implement `Error`, but `?` converts the error of a `Result` into the function's error type when that type is an interface the error implements, and turns a `String` error into an `Error`, like Rust's `?` into `Box<dyn Error>`. `main` may return `Result<(), E>` where `E` is `String` or implements `Error`; an `Err` prints `error: <message>` and exits with code 1. |
| Q49 | Validation | `impl` and the `extends` of interfaces name interfaces; `extends` between interfaces cannot loop; an interface mentioning `Self` cannot be a type; a method or field cannot repeat one of a parent interface (E0329). Interfaces cannot be instantiated. |

## Representation

- An interface value is a pair: an object (an instance, or a box holding a struct or enum
  copy) and the itable of the interface for the object's type. The itable holds:
  - thunks calling the type's methods, through the class vtable when they are dynamically
    dispatched;
  - the default methods;
  - a getter and a setter for each field;
  - the itables of the parent interfaces.
- The prelude part written in Sagitario (`Error` and the class behind `Error(...)`) is a module
  the compiler embeds. Its public items are visible everywhere, after a module's own items and
  imports.

## Alternatives considered

- **`override` for interface methods (Kotlin)**: more explicit, but one method can implement
  several interfaces at once, and the keyword would mean two things.
- **Methods only, no fields (Rust traits, Java)**: fields are what the project asked for, as in
  TypeScript, and getters for every field would be noise.
- **`String` implementing `Error`**: shorter, but unlike Rust. The `?` conversion gives the same
  convenience where it matters.
- **Identity `==` for interface values**: natural for classes, but confusing for boxed structs.

## Consequences

- New diagnostic E0329 (invalid `impl` or interface).
- Calls on concrete types stay direct. Calls through interface values go through the itable.
- The `List<Shape>`-style values of the reference "zoo" program are now possible once `List`
  exists (increment 7).
