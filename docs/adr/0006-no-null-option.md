# 0006. No null; `Option<T>` only

- Status: Accepted
- Date: 2026-10-21

## Context

Null pointer exceptions must be avoided at all costs.

## Decision

There is no `null`. Absence is expressed only with `Option<T>` (`Some` / `None`), handled
with `match`, `?`, and library methods. There is no `T?` sugar and no `?.` / `??` operators.
Internally, `Option<ClassRef>` compiles to a nullable pointer, never exposed to users.

## Alternatives considered

- **Null allowed everywhere (classic Java/C#)**: rejected outright.
- **`T?` as sugar for `Option<T>` with `?.` and `??` (Kotlin, Swift, TS strict)**: more
  ergonomic, rejected in favor of Rust's single explicit form.

## Consequences

- Absence is always visible in types and checked by the compiler.
- No runtime cost for `Option` of class references.
- Chained optional access is more verbose than in TypeScript.
