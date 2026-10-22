# 0020. Operators, bindings, control flow, interpolation and build modes

- Status: Accepted
- Date: 2026-10-22

## Context

F1 increment 2 (variables, operators, control flow, functions with return values,
interpolation) exposed points the specification did not define (questions Q14–Q18).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q14 | Operators | Rust's precedence table without bitwise operators; comparisons and ranges are non-associative; assignments are statements; unary `-` is invalid on unsigned types; `!` only on `bool`; `%` keeps the dividend's sign and panics on zero. |
| Q15 | Bindings | Optional type annotation; declaration without value allowed under definite assignment (a `const` exactly once); shadowing allowed, also in the same block; lexical block scope; parameters are `const`; warnings for never-reassigned `var` and unused bindings (unless `_`-prefixed). |
| Q16 | Control flow | `break value` only in `loop`; `while`/`for` are `()`; no labels in the MVP; diverging expressions have an internal never type; `-> T` functions must return on every path; `if` without `else` is `()`; unreachable-code warning. |
| Q17 | Interpolation | `${expr}` with any expression; integers, floats (shortest round-trip, no exponent, `2.0` prints `2`), `bool`, `char`, `String`; user types need a future `Display`. ARC for `String` moves into increment 2. |
| Q18 | Build modes and casts | Debug (default: overflow panics, `-O0 -g`) and `--release` (wraps, `-O2`); `as` follows Rust: truncation, bit reinterpretation, saturating float→int with `NaN` → 0, `bool`/`char` → integer, `u8` → `char`, everything else rejected; `as` never panics. |

## Alternatives considered

- **Forbid shadowing in the same block (Kotlin, Java)**: stricter, but forces names like
  `input2` for successive transformations.
- **Labels with Rust's `'label` syntax**: `'` is reserved for `char` literals in Sagitario; a
  different syntax would need design, so labels wait.
- **`2.0` printed as `2.0` (Java, C#)**: inconsistent with the shortest-representation rule.
- **Interpolation without ARC (leaking temporaries)**: would violate the no-leak criterion and
  be rewritten in increment 6 anyway.

## Consequences

- The runtime gains ARC strings, overflow-checked arithmetic helpers, saturating casts and
  float formatting.
- `sag build` and `sag run` accept `--release`.
