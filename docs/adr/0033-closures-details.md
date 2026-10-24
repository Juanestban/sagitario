# 0033. Closures: arrow functions, function values and their representation

- Status: Accepted
- Date: 2026-10-23
- Refines: [ADR-0012](0012-closures.md); completes [ADR-0005](0005-result-error-handling.md)
  (`map_err`)

## Context

ADR-0012 and docs/spec/08-closures.md fixed the basics:
- arrow-function syntax and `fn(A, B) -> R` types shared by named functions and closures;
- captures by value, read-only;
- a warning for closures in class fields that capture `self`;
- a function pointer plus an ARC environment, with no allocation when nothing is captured.

F2 increment 6 implements them and settles questions Q58–Q66.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q58 | One parameter without parentheses | Allowed, as in JavaScript: `x => x * 2`, `nums.filter(n => n > 0)`. It does not clash with `match` arms, which start with a pattern, nor with guards, where a lambda is not parsed. |
| Q59 | Return type | Inferred from the body (the type of its first `return`, or else of its value). A lambda has no return annotation in the MVP; the binding is annotated instead (`const f: fn(i32) -> i32 = ...`). |
| Q60 | Parameter types | Written, or taken from the expected type: a `fn(...)` parameter, a binding annotation, a field or a return type. Without either, E0311 asks for an annotation. In a generic call, lambda arguments are checked after the others, so `apply((n) => n + 1, 5)` infers `T = i32` from `5`. |
| Q61 | Captures | By value, when the closure is created: structs are copied, class instances and other references retained. Captured bindings are read-only: assigning to one (also `+=` and fields of a captured struct) is E0331, whose help suggests `fold` or a class instance. `var` fields of a captured class instance can change, since that does not reassign the capture. `self` is captured like any binding. |
| Q62 | Functions as values | Module functions and static functions (`Type::f`) are values (`const f = double;`). A generic one needs its type arguments, explicit or from the expected type. Bound methods (`obj.method` without a call, E0316) and C and prelude functions (`println`, E0314) are not values in the MVP; they are wrapped in a lambda. |
| Q63 | Types and calls | `fn(A, B) -> R`; `fn(A)` without arrow returns `()`. Function types are structural and equal only when identical (no variance). Called with `f(x)`. `obj.field(x)` calls a field of a function type when the type has no method of that name (methods win). Function values cannot be compared with `==` or interpolated. |
| Q64 | Control flow | `return` inside a lambda leaves the lambda. `break` and `continue` do not cross a lambda. `?` uses the lambda's return type. A lambda cannot call itself; recursion uses a named function. |
| Q65 | Cycle warning (W0008) | Emitted when a closure capturing `self`, or the binding of the object itself, is assigned to a field of that object (`self.on_tick = () => self.count += 1`, `obj.cb = () => obj.x`), also in an initializer. It catches the common case without a global analysis. |
| Q66 | Closure methods of `Option` and `Result` | `map`, `map_err` (`Result`), `and_then`, `unwrap_or_else` and `filter` (`Option`), as in Rust. `List` and iterators come in increment 7. |

### Representation

- A function value is a `sag_fnval`: a function pointer and an environment. The environment is
  an ARC object (`sag_envN`) holding copies of the captures, or NULL when nothing is captured,
  so such closures do not allocate. Copying a function value retains its environment.
- Each lambda becomes a separate function that receives the environment first, copies the
  captures into its own locals and runs its body. It is lowered while lowering the enclosing
  function, with that function's type arguments.
- A named function used as a value is called through a wrapper `sag_wrap_N` with the same
  convention, which ignores the environment.
- A call of a function value casts the pointer to the exact signature and passes the
  environment first. The value is kept alive for the duration of the call.

## Alternatives considered

- **Always requiring parentheses**: more uniform, but heavier in the common one-parameter case
  that the user wanted JavaScript-like.
- **TypeScript-style return annotations (`(x: i32): i32 => ...`)**: can be added later without
  breaking code.
- **Bound methods as values**: they would capture the receiver implicitly, a hidden source of
  cycles. Post-MVP.
- **Requiring `(obj.field)(x)` for field calls**: unambiguous, but unfamiliar for callback
  fields; methods taking precedence keeps the rule simple.
- **A global analysis for cycles**: more precise, but too costly for the MVP.

## Consequences

- New diagnostics E0331 (assignment to a captured binding) and W0008 (a closure that forms a
  cycle with its object). E0314 also covers C and prelude functions used as values, and the
  help of E0316 for a method used as a field suggests a lambda.
- `map_err` closes the explicit error conversion promised by docs/spec/07-errors.md.
