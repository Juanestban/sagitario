# 0104. `include_str!` instead of `include_str`

- Status: Accepted
- Date: 2026-11-14
- Supersedes: Q222 of [ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md) (`include_str`)
- Refines: [ADR-0103](0103-build-scripts-and-binding-generation.md) (Q725), [ADR-0067](0067-bootstrap-from-the-seed.md)

## Context

ADR-0049 gave Sagitario `include_str("path")`, named after Rust's `include_str!` when the
language had no macros: a builtin valid only as the value of a module-level constant, which the
compiler uses to embed the runtime, the prelude and the standard library. ADR-0103 adds Rust's
built-in macros, `include_str!` among them (Q725), valid in any expression. Two forms would do
the same.

The seed, the C of the compiler from which it builds, cannot parse `!`, so the compiler's own
sources can use `include_str!` only after the seed is updated.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q740 | One form | `include_str!(path)` replaces `include_str(path)`, as in Rust. The old form is an error (`E0346`) that gives the new one. `include_str` is no longer a builtin name. |
| Q741 | Migration | The compiler gains the macros, the seed is updated with it (`--update-seed`), and then the compiler's sources use `include_str!`. The seed keeps the FFI units of the C apart, `bootstrap/sag_ffi.c` and `bootstrap/sag_std_ffi.c`, which the bootstrap compiles as translation units of their own, as `sag` does: the standard library exports functions to the runtime's core since ADR-0085, and their declarations cannot share a unit with the runtime's header. |

## Alternatives considered

- **Both forms:** no change to the seed, but two ways to write the same thing.
- **Only the old form:** the other macros would take `!` and this one not.

## Consequences

- Programs using `include_str(...)` write `include_str!(...)`.
- The seed has three C files of the compiler instead of one.
