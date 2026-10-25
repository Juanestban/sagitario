# 0008. Single-threaded MVP; Rust-style concurrency later

- Status: Accepted (superseded in part by [ADR-0045](0045-threads-and-send.md): a single
  `Send`, and atomic counts for the whole program)
- Date: 2026-10-21

## Context

Concurrency decides whether ARC counting is atomic (5-10x more expensive per operation) and
how shared mutable class state is made safe.

## Decision

- **MVP**: single-threaded, non-atomic ARC.
- **Phase F4**: Rust's model: `Send` / `Sync` auto-traits inferred by the compiler, `spawn`,
  `Mutex<T>`, `Atomic<T>`, atomic ARC only for shareable types; later `async`/`await`.
- `Send`, `Sync`, `spawn`, `async`, `await` are reserved from the MVP.

## Alternatives considered

- **Free threads, everything shareable (Java, C#)**: always-atomic ARC and no race safety.
- **Single `Sendable` marker (Swift)**: less precise than the move/share distinction.
- **Actors / isolates**: very safe, but a larger departure from the chosen model.
- **Concurrency in the MVP**: would add auto-trait inference, two ARC variants and a
  per-platform threading runtime before the compiler works end to end.

## Consequences

- Fastest possible ARC in the MVP.
- Type design must not preclude `Send`/`Sync` inference later.
