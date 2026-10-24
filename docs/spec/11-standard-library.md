# 11. Standard library (F3)

Rationale: [ADR-0037](../adr/0037-standard-library-env-io-process.md).

The standard library is the set of modules under the path root `std`. A program imports them
with `use` like its own modules (`use std::io;`, `use std::env::args;`) or names them in full
(`std::process::exit(1)`). Only the modules a program names are compiled, and of those only the
functions it uses.

| Module | Contents | Milestone |
|---|---|---|
| `std::ffi` | C types, `CString`, `string_from_c` ([FFI](10-ffi.md)) | F1 |
| `std::env` | Command-line arguments and environment variables | F3 |
| `std::io` | Standard input and output with errors as values; `io::Error` | F3 |
| `std::process` | `exit` | F3 |

Naming a module that does not exist (`std::net`) is an error.

```sag
use std::env;
use std::io;

fn main() -> Result<(), io::Error> {
  const name = env::args().iter().skip(1).find((arg) => !arg.is_empty()).unwrap_or("world");
  io::println(`Hello, ${name}!`)?;
  Ok(())
}
```

## `std::env`

| Function | Result |
|---|---|
| `args() -> List<String>` | The command-line arguments, starting with the program as the operating system started it (usually its path) |
| `variable(name: String) -> Option<String>` | The value of an environment variable, or `None` if it is not set |

- Arguments and values that are not valid UTF-8 have each ill-formed sequence replaced by
  U+FFFD. On Windows they are read as UTF-16, so any Unicode text arrives intact.
- A variable name that is empty or contains `=` or a NUL character is never set: `variable`
  returns `None`.
- Arguments are not part of `main`'s signature ([errors](07-errors.md#program-entry-point-and-exit-codes)).

## `std::io`

| Function | Result |
|---|---|
| `print(text: String)` | Writes `text` to stdout |
| `println(text: String)` | Writes `text` and `\n` to stdout |
| `eprint(text: String)` | Writes `text` to stderr |
| `eprintln(text: String)` | Writes `text` and `\n` to stderr |
| `flush()` | Writes out what stdout holds in its buffer |
| `read_line() -> Result<Option<String>, io::Error>` | The next line of stdin without its `\n` or `\r\n`; `None` at the end of the input |
| `read_to_string() -> Result<String, io::Error>` | The rest of stdin; `""` at the end of the input |

- The functions without a stated result return `Result<(), io::Error>`. The prelude's `print`,
  `println` and `eprintln` panic instead ([prelude](09-modules.md#prelude)); both write to the
  same streams, in order.
- Input that is not valid UTF-8 is an error of kind `InvalidData`; the bytes read are lost.

### `io::Error` and `io::ErrorKind`

```sag
pub enum ErrorKind {
  NotFound,
  PermissionDenied,
  AlreadyExists,
  InvalidInput,
  InvalidData,
  UnexpectedEof,
  Other,
}
```

- `io::Error` is a class implementing the prelude's `Error` interface. `kind()` returns its
  `ErrorKind`; `message()` returns the operating system's description of the failure.
- `new io::Error(kind, message)` creates one, for code that reports its own I/O failures.
- Like any type implementing `Error`, it converts to `Error` with `?`
  ([errors](07-errors.md#the--operator)), and `main` may return `Result<(), io::Error>`.
- The kinds are the same on every operating system. Failures without a more precise kind are
  `Other`.

## `std::process`

`exit(code: i32)` ends the program with the exit code `code`. It never returns, so like `panic`
it can stand where any value is expected (`const n = if ok { 1 } else { process::exit(2) };`).

- It writes out the buffered output, then ends the program without running pending releases:
  the leak check of the test harness does not report the objects still alive.
- Exit codes outside 0–255 depend on the operating system; POSIX systems keep the low 8 bits.

## Implementation rules

The modules of `std`, except `std::ffi`, are written in Sagitario and embedded in the compiler.
They follow the rules of every module, with two exceptions that only the standard library has:

- It may declare functions of the runtime in `extern "C"` blocks: names starting with `sag_`,
  which may also take and return `String` (borrowed arguments, new references as results).
- It may name the prelude module as `std::prelude`, to implement the prelude's interfaces with
  types of the same name (`io::Error` implements `Error`).

`std::process::exit` is provided by the compiler, since no function written in Sagitario can
declare that it never returns.
