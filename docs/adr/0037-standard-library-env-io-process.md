# 0037. The standard library: `std::env`, `std::io` and `std::process`

- Status: Accepted
- Date: 2026-10-24
- Refines: [ADR-0019](0019-prelude-and-printing.md) (the `std::io` functions),
  [ADR-0024](0024-modules-imports-and-visibility.md) (the `std` root)

## Context

F3 begins after the MVP (docs/roadmap.md). Its first increment builds the standard library and
its first modules. Until now `std` held only `std::ffi`, implemented in the compiler. The spec
already promised `std::env::args()` and `std::io` functions returning `Result`. Increment 1
settles Q84–Q90, after the F3 plan in the roadmap.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q84 | How `std` is written | Modules written in Sagitario (`crates/sagitario_driver/src/std/*.sag`), embedded in the compiler like the prelude. A module is loaded only when a program names it, and its functions are lowered only when used. They reach the operating system through runtime functions declared in `extern "C"` blocks. Only the standard library may declare names starting with `sag_`, which may also take and return `String`. It may also name the prelude as `std::prelude`. |
| Q85 | `std::env` | `args() -> List<String>`, with the program first as in C and Rust. `variable(name) -> Option<String>`. Text that is not UTF-8 is converted lossily (U+FFFD); Windows reads the UTF-16 command line and environment. Setting variables waits for F4, since changing the environment is not thread-safe. The current directory belongs to `std::fs` (increment 2). |
| Q86 | `std::process::exit` | `exit(code: i32)` never returns, typed like `panic` without a surface `!` type, so the compiler provides it. It flushes output and ends the program without pending releases, like Rust's. The leak check does not apply after it. Codes outside 0–255 depend on the system. |
| Q87 | `io::Error` | A class implementing `Error`, with `kind() -> io::ErrorKind` and `message()`. The kinds are `NotFound`, `PermissionDenied`, `AlreadyExists`, `InvalidInput`, `InvalidData`, `UnexpectedEof` and `Other`, and the message is the system's (`strerror`). `std::fs` and `Command` will reuse it. |
| Q88 | `std::io` functions | `print`, `println`, `eprint` and `eprintln` return `Result<(), io::Error>`, as do `flush()`, `read_line() -> Result<Option<String>, io::Error>` (without `\n`/`\r\n`, `None` at the end) and `read_to_string()`. Input that is not UTF-8 is an `InvalidData` error. |
| Q89 | Test harness | The annotations `// args: a b`, `// stdin: line` (repeatable) and `// env: NAME=value`. `expect-exit` checks `exit`. |
| Q90 | Documentation | A spec chapter, `docs/spec/11-standard-library.md`, that grows with each increment. Naming a missing module of `std` is an unresolved name (E0200), no longer "not implemented". |

`var` is a keyword, so the function Q85 called `var` is `env::variable`.

## Alternatives considered

- **`std` implemented in Rust, like `std::ffi`:** more compiler code to port when self-hosting,
  and every function would need its own type checking and lowering.
- **Calling the C library directly from Sagitario:** C strings and `errno` would need raw
  pointers in every module, and `errno`, `stdin` and `stdout` are macros or objects C does not
  let a program declare portably.
- **A surface never type (`fn exit(code: i32) -> !`):** a language change for one function. It
  can come later without breaking `exit`.
- **`args()` without the program:** most languages include it, and programs that print usage
  messages need it.

## Consequences

- Runtime functions take raw pointers for results beyond their return value
  (`sag_io_read_line(status: *mut i32) -> String`). The standard library wraps them in safe
  functions.
- The runtime declares the two kernel32 functions it needs on Windows itself, since
  `<windows.h>` is not strict C11.
- The harness cannot write invalid UTF-8 to a program's stdin, so the `InvalidData` path of
  `std::io` has no end-to-end test.
