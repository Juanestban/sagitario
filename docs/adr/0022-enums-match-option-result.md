# 0022. Enums, `match`, `Option`/`Result` and `?`

- Status: Accepted
- Date: 2026-10-22

## Context

F1 increment 5 (enums, exhaustive `match`, `Option`/`Result` and `?`) exposed points the
specification left open (question Q21). In particular, `Option` and `Result` are generic while
user generics only arrive in F2, and the pattern language was only sketched.

## Decision

| # | Topic | Decision |
|---|---|---|
| 1 | `Option`/`Result` before generics | Generic enums known to the compiler, instantiated for each combination of types used. User generic enums arrive in F2. |
| 2 | Inference of `None`, `Ok(x)`, `Err(e)` | The missing type comes from context (annotation, return type, argument, the other branch when it comes first); without it, error E0311. `Some(x)` takes its type from `x`. |
| 3 | Methods in F1 | `is_some`/`is_none`, `is_ok`/`is_err`, `unwrap`, `expect(msg)`, `unwrap_or(v)`, `unwrap_err`. Methods taking closures (`map`, `map_err`, `and_then`) arrive in F2. `unwrap` and `expect` panic with Rust's messages. |
| 4 | Building variants | Always with their path (`Shape::Circle(1.0)`, `Shape::Rect { w, h }`, `Shape::Empty`, `Self::Empty`); only the prelude's `Some`/`None`/`Ok`/`Err` go without one. |
| 5 | Variant shapes | `A()`, `A {}` and enums without variants are errors; recursive enums by value are errors (like structs). |
| 6 | Patterns | `_`, bindings, integer (also negative), `char` and `bool` literals, nested variants, named fields with shorthand and `..`, and alternatives `A \| B`. String patterns arrive with `String` comparison (increment 6). No float, range or struct patterns. |
| 7 | Guards | `pattern if condition =>`; a guarded arm does not count for exhaustiveness. |
| 8 | Diagnostics | A non-exhaustive `match` is an error listing missing cases; an unreachable arm is warning W0004. Rust's usefulness algorithm handles nested patterns. |
| 9 | Pattern bindings | New immutable bindings holding a copy of the matched part (retained if it holds `String`s); the alternatives of `\|` bind the same names with the same types. |
| 10 | `?` | Errors are never converted; the diagnostic suggests converting with `match` (`map_err` needs closures, F2). In `main`, `?` needs `main -> Result<(), String>`. |
| 11 | `main` errors | `main` returning `Err(e)` prints `error: <e>` to stderr and exits with code 1. |
| 12 | `if let` / `while let` | Not in the MVP; use `match`. Recorded as a deferred decision. |
| 13 | `as` on enums | Not allowed. |

## Alternatives considered

- **Unification-based inference for `None`/`Ok`/`Err` (Rust)**: infers from later uses, but needs
  type variables across statements, a much larger checker for little benefit in F1.
- **Implement user generic enums now**: would pull most of F2's generics into F1.
- **Variants usable without their path (Swift's `.circle`)**: shorter, but ambiguous across enums
  without type-directed lookup.

## Consequences

- The spec gains the rules above in chapters 2, 6 and 7.
- SIR represents enum values as a tag plus the fields of the active variant; `match` lowers to
  tag tests that run before any binding, so failed arms leave nothing to release.
- In C, enums are `sag_eN` and each array, `Option` and `Result` type a `sag_tN` struct holding a
  tag and a union of per-variant structs.
- New diagnostics: E0104 (empty enum or variant), E0206 (inconsistent `|` bindings), E0318
  (wrong type arguments), E0319 (non-exhaustive `match`), E0320 (invalid pattern), E0321
  (invalid `?`) and warning W0004 (unreachable pattern).
