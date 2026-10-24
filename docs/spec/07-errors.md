# 7. Errors and panics (F1)

## Recoverable errors: `Result<T, E>`

```sag
fn parse_age(s: String) -> Result<i32, String> {
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
- The error type must match the function's `E`, with two conversions (F2), as in Rust's `?`
  into `Box<dyn Error>`: an error whose type implements an interface `E` becomes an `E`, and a
  `String` error becomes an `Error` when `E` is `Error`
  (`fn parse(s: String) -> Result<i32, Error> { const n = s.parse_i32()?; Ok(n) }`).
- Any other conversion is explicit, and the diagnostic must suggest it, with a `match` or with
  `map_err` ([08-closures.md](08-closures.md)):

```sag
const text = fs::read(path).map_err((e) => AppError::Io(e))?;
```

Automatic conversion through a `From<T>` interface is post-MVP; it will be designed together
with implementing the same generic interface several times on one type.

## Panics

`panic` is for unrecoverable errors (bugs, broken invariants). In the MVP a panic prints a
message with the source file and line to stderr and **aborts the process**.

`panic(message)` (prelude) panics explicitly; it never returns, so it fits where any type is
expected: `_ => panic("unsupported shape"),`.

Operations that always panic, in every build mode:

- Integer division by zero.
- Array or list index out of bounds.
- `unwrap()` and `expect(...)` on `None` or `Err`, and `unwrap_err()` on `Ok`.
- `substring` with an index outside the string or inside a character, `repeat` beyond the
  maximum size, and `to_digit` with a radix outside 2 to 36.

Integer overflow panics in debug builds only (see [types](02-types.md#integer-arithmetic)).

## Program entry point and exit codes

```sag
fn main() { ... }                    // return type omitted: returns ()
fn main() -> Result<(), E> { ... }   // optional
```

`main` is declared in the root module (the entry file), with any visibility: `pub fn main()`
is also valid. A function called `main` in another module is an ordinary function. Declaring
a return type on `main` is optional. When it returns `Result<(), E>`:

- `E` is `String`, printed as is, or any type implementing `Error` (F2), printed with
  `message()`. F1 programs keep compiling unchanged.

| Outcome | Exit code |
|---|---|
| `main` returns normally or returns `Ok` | `0` |
| `main` returns `Err(e)` (`error: <e>` is printed to stderr) | `1` |
| A panic occurs | `101` |

Command-line arguments are read with `std::env::args()`; they are not part of `main`'s
signature.
