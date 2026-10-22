# 0005. Errors as values with `Result` and `?`

- Status: Accepted
- Date: 2026-10-21

## Context

Java/C#/TypeScript use exceptions; Rust uses errors as values. With ARC and a C backend,
exceptions would need `setjmp`/`longjmp` plus manual release of every live reference.

## Decision

Recoverable errors are `Result<T, E>` values propagated with `?`, as in Rust. `panic` is
separate, for unrecoverable errors, and aborts the process in the MVP. This requires enums
with data and exhaustive `match` in the core language.

## Alternatives considered

- **Exceptions with unwinding**: familiar, but fragile and slow to implement in C with ARC,
  and prone to leaks.
- **Swift-style `throws` + `try`/`catch` lowered to return values**: familiar syntax with
  the same performance; rejected in favor of Rust's explicit model.

## Consequences

- Predictable performance; errors are ordinary return values in C.
- Every failure point is visible in the code.
- More verbose than exceptions; library helpers (`map_err`, `unwrap_or`, ...) are needed.
