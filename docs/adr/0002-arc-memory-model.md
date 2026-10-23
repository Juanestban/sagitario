# 0002. ARC memory model with value and reference types

- Status: Accepted; refined by [ADR-0027](0027-classes-and-arc.md)
- Date: 2026-10-21

## Context

The language must be fast, simpler than Rust, and support classes and closures in the style
of TypeScript/Java/C#. Mobile targets penalize garbage-collector pauses.

## Decision

Swift-style hybrid model: `struct` is a value type (inline/stack, copied or moved) and
`class` is a reference type managed by automatic reference counting. `weak` references break
cycles. ARC optimizations (e.g. escape analysis to elide retain/release) are deferred.

## Alternatives considered

- **Ownership + borrow checker (Rust)**: maximal performance, but lifetimes contradict the
  simplicity goal, classes with shared references fight the borrow checker, and it is the
  most expensive feature to implement twice (Rust compiler, then self-hosted compiler).
- **Tracing GC (Go, C#)**: simplest for users, but pauses and a runtime GC.
- **Manual memory (Zig, C)**: simple compiler, unsafe programs.

## Consequences

- Deterministic destruction, no GC pauses.
- Natural fit for classes and closures.
- Reference cycles leak unless broken with `weak`; the compiler warns on known patterns.
- Atomic counting will be needed for types shared across threads (see ADR-0008).
