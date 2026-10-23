# 0029. Inheritance: construction, overriding, subtyping and dispatch

- Status: Accepted
- Date: 2026-10-23
- Refines: [ADR-0004](0004-class-model.md)

## Context

ADR-0004 and docs/spec/04-classes.md fixed the class model:
- single inheritance with `extends`;
- `zod.new(...)` first in a subclass's initializer;
- mandatory `override` and `zod.method(...)`;
- `abstract`;
- no `open`/`final`, with devirtualization of methods nobody overrides.

F2 increment 2 implements it. Several points were still open: when `zod.new` is required, which
methods run while a parent initializes, what is inherited, the rules of `override`, how
subclasses convert to their parents, and the rules for abstract classes (questions Q30–Q36).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q30 | `zod.new` | When an ancestor has an initializer, the subclass's own `new` starts with `zod.new(...)`, which calls the nearest ancestor's initializer; afterwards the inherited fields count as assigned. `zod.new` anywhere else is an error. A subclass without fields of its own may omit `new` only if that inherited initializer takes no arguments, and then it runs implicitly. Initializers are not inherited: `new Dog(...)` always uses `Dog`'s. |
| Q31 | Calls during construction | As in C++: while a parent's initializer runs, the instance behaves as the parent. Its methods run the parent's implementations, so an `override` never sees the subclass's unassigned fields. Calling an abstract method then panics. |
| Q32 | Inheritance | Fields and methods are inherited; `prv` members stay accessible only inside the class that declares them. Static functions are not inherited (`Animal::create()`, never `Dog::create()`). A class cannot declare a field or function with the name of an inherited member, unless it is an `override` of a public method (E0201). |
| Q33 | `override` | The signature must be identical: same parameters and return type, no covariance in the MVP. Only public methods can be overridden, and an `override` is `pub`. Replacing a method without `override`, or writing `override` with nothing to override, is E0327. `zod.method(...)` exists only in a subclass's methods and initializer and calls the parent's implementation without dynamic dispatch; `zod` is no value and has no fields. |
| Q34 | Subtyping | An instance of a class can be used wherever one of its ancestors is expected: arguments, assignments, `return`, fields, and elements or `Some(...)` when the expected type is known. Branches of `if` and `match` take the expected type, or the ancestor when one branch is the other's ancestor. Containers have no variance: an `Option<Dog>` already built is not an `Option<Animal>`. `==`/`!=` work between related classes. There is no downcast in the MVP. |
| Q35 | Abstract | `new` on an abstract class is an error (E0326), but an abstract class may declare `new` for its subclasses and is a type. An `abstract fn` has no body and is a public method. The first concrete subclass implements every abstract method with `override` (E0326). A class that is not `abstract` cannot declare abstract methods. |
| Q36 | Validation | Only classes can be extended, and a chain of `extends` cannot loop (E0326). Classes of other modules can be extended if visible; a `pub` class extending a private one gets W0006. |

### Representation

- The struct of a subclass starts with its parent's, so a pointer to an instance is a valid
  pointer to its parent part. In C every class value is a `sag_object *`; fields are read
  through the struct of the class that declares them.
- Methods that are abstract or overridden somewhere get a slot in the vtable of their class and
  of its subclasses. The other methods, and calls through `zod`, are direct calls.
- Each initializer points the instance's vtable at its own class once its parent part is
  initialized, which gives Q31.

## Alternatives considered

- **Swift's two-phase initialization** (the subclass assigns its fields before calling
  `super.init`): virtual calls in the parent are then safe. But `zod.new` would no longer be
  the first statement, contradicting the specified syntax.
- **Inherited static functions (Java)**: one more lookup rule, and `Dog::create()` would
  return an `Animal`.
- **Covariant return types**: useful for `clone`-like methods. They can be added later without
  breaking programs.
- **Downcasts (`as? Dog`, `is`)**: they need runtime type information. They are deferred,
  together with type-based `match`.

## Consequences

- New diagnostics: E0326 (invalid inheritance) and E0327 (invalid `override` or abstract
  method).
- The runtime's `sag_object` header gains the vtable pointer.
