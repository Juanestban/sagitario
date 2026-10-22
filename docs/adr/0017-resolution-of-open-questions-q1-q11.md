# 0017. Resolution of open questions Q1–Q11

- Status: Accepted
- Date: 2026-10-22

## Context

Writing the first specification exposed points the initial design did not cover (open
questions Q1–Q11 in the spec index). All were needed by F1 or F2.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q1 | Comments | `//`, nestable `/* */`, `///` item docs, `//!` module docs (Rust). Doc comments are not processed in the MVP. |
| Q2 | Attributes | `@name` / `@name(args)` before the item; closed set, no user attributes. MVP: `@export("symbol")`, `@repr(C)`; reserved: `@test`, `@deprecated`. |
| Q3 | Struct member visibility | `pub` / `prv` mandatory, as in classes. A struct literal is only valid where all fields are accessible. |
| Q4 | Ranges | `a..b` and `a..=b` on same-typed integers; empty if `a > b`; `step_by` / `rev` in F2; open ranges post-MVP. |
| Q5 | Arrays and lists | `[T; N]` fixed arrays are value types (`[1, 2, 3]`, `[0; 16]`); `List<T>` is a standard ARC class (`new List<T>()`, `List::from(...)`). Slices post-MVP. |
| Q6 | Literals | Rust forms: `0x` / `0o` / `0b`, `_`, type suffixes, exponents; out-of-range literals are errors; Rust escapes plus `` \` `` and `\$`; no raw strings in the MVP. |
| Q7a | Keep `struct` | `struct` stays as the value type (ADR-0002 unchanged). |
| Q7b | Enums | Rust variant syntax, variants always public; methods follow the variants in the same body with `pub` / `prv`; `struct`, `enum` and `class` implement interfaces with `impl I1, I2`. |
| Q8 | Static functions | Functions without `self` are static, called `Type::f()`. In classes `new` is the initializer, only via `new Class(...)`; in structs/enums `new` is a conventional static function. No static fields or mutable globals in the MVP. |
| Q9 | `Self` and unit | `Self` names the declared or implementing type (static in classes); interfaces using `Self` in signatures cannot be dynamic types. `()` is the unit type; no general tuples in the MVP. |
| Q10 | `?` conversion | Exact error-type match plus explicit `map_err` in the MVP; `From<T>` conversion post-MVP. |
| Q11 | `Error` | `interface Error { fn message(self) -> String; }` in F2. In F1, `main` may return `Result<(), E>` with `E` = `String` or a std error type. `main`'s return type is optional. |

## Alternatives considered

- **Q2 `#[attr]` (Rust)**: noisier, and `@` matches the TS/Java/C# audience.
- **Q3 optional visibility with private default**: two different rules for structs and
  classes.
- **Q7a remove `struct`, everything is a class**: every small value would be a heap
  allocation with reference counting (cache misses, 10–100x slower in hot data), C structs
  could not be modeled for FFI, and the self-hosted compiler would be slow. C# and Swift keep
  both for these reasons.
- **Q7b separate `impl Shape { }` blocks (Rust)**: `impl` already means "implements
  interfaces". **Enums without methods**: impossible, since `Option` and `Result` need them.
  **A new `extend` block**: an extra concept for one case.
- **Q8 forbid `new` on structs**: departs from Rust conventions; the asymmetry with the class
  initializer is accepted.
- **Q10 `From<T>` in the MVP**: interfaces only arrive in F2, and implementing `From<T>` several
  times on one type would need method overloading or per-interface blocks, which Sagitario
  does not have.

## Consequences

- The F1 grammar is fully specified except for features reserved for later phases.
- Q12 (`weak` declaration syntax) remains open until F2.
