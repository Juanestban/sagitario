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
- The error type must match the function's `E`, with three conversions, as in Rust's `?`: an
  error whose type implements an interface `E` becomes an `E`, and a `String` error becomes an
  `Error` when `E` is `Error`, as Rust's into `Box<dyn Error>`
  (`fn parse(s: String) -> Result<i32, Error> { const n = s.parse_i32()?; Ok(n) }`); otherwise,
  an error of type `X` becomes `E::from(e)` when `E` implements
  [`From<X>`](05-interfaces-generics.md#conversions)
  ([ADR-0094](../adr/0094-conversions-and-defaults.md), Q635):

```sag
enum AppError { Parse(String), Io(io::Error) }

impl From<String> for AppError {
  fn from(message: String) -> AppError { AppError::Parse(message) }
}

fn port(text: String) -> Result<i32, AppError> {
  Ok(text.parse_i32()?)   // a `String` error becomes an `AppError`
}
```

- Any other conversion is explicit, and the diagnostic (`E0321`) suggests implementing `From`, or
  `map_err` ([08-closures.md](08-closures.md)):

```sag
const text = fs::read(path).map_err((e) => AppError::Io(e))?;
```

## Panics

`panic` is for unrecoverable errors (bugs, broken invariants). In the MVP a panic prints a
message with the source file and line to stderr and **aborts the process**.

`panic(message)` (prelude) panics explicitly; it never returns, so it fits where any type is
expected: `_ => panic("unsupported shape"),`.

Assertions ([ADR-0093](../adr/0093-everyday-methods.md), Q618) are prelude functions that panic,
in every build mode, at the file and line of their call:

| Call | Panics when | Message |
|---|---|---|
| `assert(condition)` | `condition` is `false` | `assertion failed` |
| `assert_eq(left, right)` | `left != right` (`T: Eq`) | `assertion failed: left == right`, then `  left: ` and ` right: ` with the debug form of each |
| `assert_ne(left, right)` | `left == right` (`T: Eq`) | `assertion failed: left != right`, then both values |
| `unreachable()` | always | `entered unreachable code` |
| `todo()` | always | `not yet implemented` |

`unreachable()` and `todo()` never return, as `panic`. There is no message argument: a message
is `if !condition { panic(f"...") }`. A function of a program with one of these names takes
their place in that program.

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

`main` may also be an `async fn` with the same return types; its body then runs with
`task::block_on` ([`async` and `await`](12-async.md#running-tasks)).

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
| `std::process::exit(code)` is called (F3) | `code` |

Command-line arguments are read with `std::env::args()`; they are not part of `main`'s
signature ([standard library](11-standard-library.md)).
