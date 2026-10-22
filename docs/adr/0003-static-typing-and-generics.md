# 0003. Static typing, local inference, monomorphized generics

- Status: Accepted
- Date: 2026-10-21

## Context

A compiled systems language needs static types for performance. Developers coming from
TypeScript/Java/C# expect generics such as `List<T>` and bounded functions.

## Decision

- Static typing with local inference; public signatures are fully annotated.
- Generics are monomorphized. Bounds are interfaces, written `<T: A + B>`, with an optional
  `where` clause for long bounds.
- Interfaces used as types provide dynamic dispatch through vtables.
- No higher-kinded types, specialization or variadic generics in the MVP.
- Entry point: `pub fn main()` or `pub fn main() -> Result<(), E>`; exit code 1 on `Err`,
  101 on panic; arguments via `std::env::args()`.

## Alternatives considered

- **Type erasure with boxing (Java, TypeScript)**: smaller binaries, but every `T` becomes a
  heap pointer, which contradicts the speed goal.
- **Runtime reification (C#)**: needs a JIT/runtime; incompatible with AOT compilation to C.
- **Bounds syntax `T impl A` or `T extends A`**: `impl` keeps a single meaning ("this type
  implements this interface") and `extends` is reserved for class inheritance.

## Consequences

- Maximum performance, each instantiation is a concrete C function.
- Larger binaries and longer compile times.
