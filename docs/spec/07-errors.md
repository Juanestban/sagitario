# 7. Errors and panics (F1)

## Recoverable errors: `Result<T, E>`

```sag
fn parse_age(s: String) -> Result<i32, ParseError> {
  const n = s.parse_i32()?;
  Ok(n)
}
```

- Recoverable errors are values of type `Result<T, E>` (`Ok(x)` / `Err(e)`).
- Errors are types implementing the `Error` interface (members: open question Q11).
- There are no exceptions, no `throw`, no `try`/`catch`, and no stack unwinding.

## The `?` operator

- On `Result`: if the value is `Err(e)`, the function returns `Err(e)` immediately;
  otherwise it evaluates to the `Ok` value.
- On `Option`: if the value is `None`, the function returns `None`; otherwise it evaluates
  to the `Some` value.
- `?` is only valid inside a function whose return type is compatible. Error type
  conversion: open question Q10.

## Panics

`panic` is for unrecoverable errors (bugs, broken invariants). In the MVP a panic prints a
message with the source file and line to stderr and **aborts the process**.

Operations that always panic, in every build mode:

- Integer division by zero.
- Array or list index out of bounds.

Integer overflow panics in debug builds only (see [types](02-types.md#integer-arithmetic)).

## Program entry point and exit codes

```sag
pub fn main() { ... }
pub fn main() -> Result<(), E> { ... }   // E implements Error
```

| Outcome | Exit code |
|---|---|
| `main` returns normally or returns `Ok` | `0` |
| `main` returns `Err(e)` (the error is printed to stderr) | `1` |
| A panic occurs | `101` |

Command-line arguments are read with `std::env::args()`; they are not part of `main`'s
signature.
