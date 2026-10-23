# 0007. Mutability with `const` / `var`

- Status: Accepted; amended by [ADR-0028](0028-inferred-self-mutation.md)
- Date: 2026-10-21

## Context

Classes are shared ARC references and there is no borrow checker. The mutability model
determines how safe programs are against side effects and, later, data races.

## Decision

- Bindings use `const` (immutable) and `var` (mutable); there is no `let` / `mut`.
  Module-level `const` is a compile-time constant.
- Struct fields take no modifier; struct mutability comes from the binding. Mutating
  methods declare `var self`.
- Class fields are declared `const` or `var`; `var` fields are mutable through any reference.
- Data-race safety is delegated to the concurrency model (ADR-0008).

## Alternatives considered

- **`let` / `let mut` (Rust)**: the initial proposal; replaced by `const`/`var` for
  familiarity with JavaScript/TypeScript.
- **Forbid mutating shared class instances without `Cell`/`Mutex`**: safer, but contradicts
  Java/C#-style class ergonomics.

## Consequences

- Local, predictable mutation for value types.
- Shared mutable state is possible for classes; safe in the single-threaded MVP, guarded by
  `Send`/`Sync` once threads exist.
