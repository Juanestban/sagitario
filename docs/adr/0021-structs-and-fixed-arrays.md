# 0021. Structs, methods and fixed arrays

- Status: Accepted
- Date: 2026-10-22

## Context

F1 increment 4 (structs with methods and static functions, fixed arrays) exposed points that
the specification left open: struct literals, mutation through fields and elements, how
methods and static functions are called, recursive structs, name collisions, indexing and
iteration over arrays (question Q20). ADR-0017 had already settled member visibility (Q3),
arrays versus lists (Q5) and static functions (Q8).

## Decision

| # | Topic | Decision |
|---|---|---|
| 1 | Struct literals | Every field exactly once, in any order; shorthand `Point { x, y }`; no `..base`. Not allowed directly in the condition of `if`/`while` or the iterable of `for` (parenthesize them), as in Rust. |
| 2 | Copies | Assigning or passing a struct or array copies it; copying retains the `String`s inside. There is no move. |
| 3 | Mutating places | `p.x = v`, `a[i] = v`, `p.pos.x += 1`, `a[i].x = 0` require the place to belong to a `var` binding or to `var self`; `self` is immutable in `self` methods. |
| 4 | `var self` calls | Only on mutable places (a `var`, `var self`, or their fields and elements); never on `const` bindings, parameters, loop variables or temporaries. |
| 5 | Call forms | Methods are called `value.m()`, static functions only `Type::f()`; the other forms are errors. `new` is a keyword, but it may name struct functions (`Point::new`). |
| 6 | Recursive structs | A struct that contains itself by value (directly, through other structs or arrays) is an error; recursion needs a reference type (`class`, F2). |
| 7 | Namespaces | Functions and structs (later enums, classes and constants) share one namespace per module; fields and functions share one namespace per struct. |
| 8 | `prv` | A `prv` member is accessible only inside the body of its struct. Item visibility (`pub struct`) is enforced with modules (F1 increment 7). |
| 9 | Indexing | Index type `usize`; `a.len()` returns `usize`; out of bounds panics with `index out of bounds: the len is 3 but the index is 5`, in every build mode. |
| 10 | Array length | `N` is an integer literal (`usize`); module constants will be allowed once they exist. `[x; N]` copies `x` N times. `[]` needs a type from context. |
| 11 | `for` over arrays | Iterates over a copy taken before the loop; each element is a fresh `const` binding. |
| 12 | Operations | Structs and arrays support neither `==` nor interpolation. Fields of temporaries can be read. |
| 13 | Warnings | A `var` modified only through its fields, elements or `var self` methods counts as reassigned. No unused-member warnings in the MVP. |
| 14 | Out of scope | Generic structs and `impl` on structs (F2), `@repr(C)` (F1 increment 8). |

## Alternatives considered

- **Move semantics for structs (Rust)**: avoids reference-count traffic, but needs
  use-after-move analysis and makes values behave differently from primitives; copies with
  ARC are simpler and match the value semantics already specified for arrays.
- **Separate type and value namespaces (Rust)**: allows `struct P` next to `fn P`, which is
  confusing and rarely useful.
- **Calling methods as `Type::m(value)` (Rust's fully qualified form)**: two spellings for one
  call; it can be added later if generic code needs it.
- **Any integer type as index**: hides conversions between signed and unsigned values.

## Consequences

- The spec gains the rules above in chapters 2, 3 and 6.
- SIR gains places (fields and bounds-checked elements), aggregate construction and
  retain/release of any type that contains `String`s; the C backend emits a C struct per
  struct and per array type, with per-type retain/release functions.
- New diagnostics: E0103 (missing member visibility), E0205 (private member), E0312
  (modifying an immutable place), E0313 (`var self` method on an immutable value), E0314
  (wrong call form), E0315 (missing fields), E0316 (unknown field or method), E0317
  (recursive struct).
