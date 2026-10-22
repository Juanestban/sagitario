# 0013. Semantics of primitives and `String`

- Status: Accepted
- Date: 2026-10-21

## Context

Primitive semantics touch every line of code and the C runtime. Signed overflow is undefined
behavior in C, so the backend must not emit naive arithmetic.

## Decision

1. Integer overflow panics in debug and wraps in release; `wrapping_*`, `checked_*`,
   `saturating_*` in the library. The generated C never contains UB.
2. No implicit numeric conversions; `as` for explicit casts; unsuffixed literals default to
   `i32` / `f64`.
3. Division by zero and out-of-bounds indexing always panic.
4. `String` is immutable UTF-8 managed by ARC, with `StringBuilder`; no byte indexing;
   `char` is a 32-bit Unicode scalar.
5. `bool` is strict; no truthiness.
6. No operator overloading in the MVP; `==` on classes is identity; structs compare via a
   method until an `Eq` interface exists.

## Alternatives considered

- **Owned, mutable `String` (Rust)**: more control, but more ceremony; immutable shared
  strings match Java/C#/JS expectations.
- **Always-wrapping or always-panicking overflow**: Rust's debug/release split balances
  safety and speed.

## Consequences

- The runtime needs overflow-checked arithmetic helpers with a portable C11 path.
- String concatenation in loops should use `StringBuilder`.
