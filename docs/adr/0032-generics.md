# 0032. Generics: type arguments, inference, bounds and monomorphization

- Status: Accepted; refined by [ADR-0034](0034-collections-and-iterators.md)
- Date: 2026-10-23
- Refines: [ADR-0003](0003-static-typing-and-generics.md); completes
  [ADR-0031](0031-interfaces.md) (Q47)

## Context

ADR-0003 and docs/spec/05-interfaces-generics.md fixed the basics:
- type parameters on functions, methods, classes, structs, enums and interfaces;
- bounds `T: I1 + I2` and `where` clauses;
- monomorphization, without boxing or runtime type information.

F2 increment 5 implements them and settles questions Q50–Q57.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q50 | Explicit type arguments | TypeScript style, without `::`: `max<i32>(a, b)`, `new Box<i32>(1)`, `Pair<i32>::new(1, 2)`, `Maybe<String>::Nothing`. Chained comparisons are already an error, so `<` after a path starts type arguments whenever they parse and are followed by `(`, `::` or `{`; otherwise it is a comparison. |
| Q51 | Inference | Local. Type arguments come from the arguments of a call (or the fields of a literal) and from the expected type (`const b: Box<i32> = new Box(1)`). One that stays unknown is E0311, asking for an annotation. `Self::f()` inside a generic type keeps the type's own arguments. |
| Q52 | What a `T` can do | Without bounds: be stored, copied, passed, returned and put inside other types. With bounds: also the methods and fields of those interfaces. Generic code is checked once, at its definition, never per instance. |
| Q53 | Bounds | Interfaces, possibly generic. They are satisfied by classes, structs and enums implementing them, by interface types (`T = Shape` satisfies `T: Shape`) and by type parameters bounded by them. Primitives and `String` implement no interfaces in the MVP; operations such as equality and order for `List` are decided in increment 7. |
| Q54 | Generic interfaces | Allowed (`interface Source<T> { fn next(self) -> Option<T>; }`). A type implements each generic interface once. An instantiated generic interface can be a type (`Source<i32>`) if it can be a type at all. |
| Q55 | Scope | Generic methods inside generic types. Generic classes extend and are extended with arguments (`class Labeled<T> extends Box<T>`, `class IntBox extends Box<i32>`). No variance: `Box<Dog>` is not `Box<Animal>`. No default type arguments, no constant generics. |
| Q56 | Interfaces with `Self` | `Self` is a parameter of the interface, bounded by it. Interfaces mentioning `Self` may have default methods. Default methods are instantiated for each implementing type, also when called through an interface value. Such interfaces still cannot be types, only bounds. |
| Q57 | Endless instances | Generic code whose instances keep needing larger type arguments is E0330 once arguments nest more than 64 levels. `sag check` also instantiates, to find it. |

### Consequences of monomorphization

- Methods with their own type parameters cannot be overridden or be abstract (E0327): a vtable
  slot holds one function. An interface with such methods cannot be a type (E0329): its values
  could not call them.

### Representation

- A program type carries its type arguments (`Pair<i32, String>`). Type parameters are
  `Type::Param`, and the checker records the type arguments of every generic call, literal and
  `new`.
- Lowering treats generic functions, methods of generic types and interface methods as
  templates. It instantiates them on demand from a worklist, substituting the instance's
  arguments in every type.
- Each class type with its arguments is a class instance, with its own struct, constructor,
  destructor and vtable. Generic structs and enums become one C type per instantiation.

## Alternatives considered

- **Rust's turbofish (`max::<i32>(...)`)**: unambiguous, but heavier for the TypeScript- and
  Java-like audience. The chained-comparison rule already removes the ambiguity.
- **Global inference (Hindley–Milner)**: more inference, but slower and with worse errors.
- **Checking each instance (C++ templates)**: more permissive, but errors would come late,
  repeated, and inside the generic code instead of at the use.
- **Built-in interfaces for primitives (`Comparable`, `Equatable`) now**: deferred to the
  design of `List` (increment 7), which needs them.

## Consequences

- New diagnostic E0330 (unsatisfied bound, a bound that is not an interface, endless
  instances). E0318 also covers generic program types and functions.
- `Option` and `Result` stay built-in; programs may now define their own generic enums.
