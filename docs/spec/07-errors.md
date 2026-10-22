# 7. Errors and panics (F1)

## Recoverable errors: `Result<T, E>`

```sag
fn parse_age(s: String) -> Result<i32, ParseError> {
  const n = s.parse_i32()?;
  Ok(n)
}
```

- Recoverable errors are values of type `Result<T, E>` (`Ok(x)` / `Err(e)`).
- From F2, error types implement the `Error` interface
  (see [interfaces](05-interfaces-generics.md#the-error-interface-f2)).
- There are no exceptions, no `throw`, no `try`/`catch`, and no stack unwinding.

## The `?` operator

- On `Result`: if the value is `Err(e)`, the function returns `Err(e)` immediately;
  otherwise it evaluates to the `Ok` value.
- On `Option`: if the value is `None`, the function returns `None`; otherwise it evaluates
  to the `Some` value.
- `?` is only valid inside a function whose return type is compatible.
- The error type must match the function's `E` **exactly**; there is no automatic
  conversion. Convert explicitly, and the diagnostic must suggest it:

```sag
const text = fs::read(path).map_err((e) => AppError::Io(e))?;
```

Automatic conversion through a `From<T>` interface is post-MVP; it will be designed together
with implementing the same generic interface several times on one type.

## Panics

`panic` is for unrecoverable errors (bugs, broken invariants). In the MVP a panic prints a
message with the source file and line to stderr and **aborts the process**.

Operations that always panic, in every build mode:

- Integer division by zero.
- Array or list index out of bounds.

Integer overflow panics in debug builds only (see [types](02-types.md#integer-arithmetic)).

## Program entry point and exit codes

```sag
pub fn main() { ... }                    // return type omitted: returns ()
pub fn main() -> Result<(), E> { ... }   // optional
```

Declaring a return type on `main` is optional. When it returns `Result<(), E>`:

- **F1:** `E` must be `String` or a standard library error type known to the compiler
  (`io::Error`, `ParseError`, ...), which the compiler knows how to print.
- **F2:** `E` may be any type implementing `Error`; the error is printed with `message()`.
  F1 programs keep compiling unchanged.

| Outcome | Exit code |
|---|---|
| `main` returns normally or returns `Ok` | `0` |
| `main` returns `Err(e)` (the error is printed to stderr) | `1` |
| A panic occurs | `101` |

Command-line arguments are read with `std::env::args()`; they are not part of `main`'s
signature.
