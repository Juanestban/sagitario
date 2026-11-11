# 0086. C's global variables in the FFI, and `std::io` in Sagitario (S1)

- Status: Accepted
- Date: 2026-11-11
- Refines: [ADR-0083](0083-global-state.md) (Q534: `extern "C" { static ...; }` was deferred) and
  [ADR-0085](0085-conditional-compilation-and-std-time.md) (Q554: `std::io` in S1)

## Context

`std::io` is the fourth module of S1. Its C in the runtime writes to stdout and stderr, reads
stdin, tells whether a stream is a terminal, and turns `errno` codes into an `ErrorKind` and a
message.

The prelude's `print`, `println` and `eprintln`, the panics and the end of the program stay in the
runtime's core until S3, and they write through C's `FILE *stdout` and `stderr`, which buffer.
The spec promises that the prelude and `std::io` write to the same streams in order
(docs/spec/11-standard-library.md#stdio). The C library's streams also lock, so that threads may
use them at once: the language server reads stdin with `io::read_exact`.

`std::io` in Sagitario must therefore use the same `FILE *` streams. In the C library, though, the
streams are global variables (`stdout` in glibc and Android's bionic, `__stdoutp` on Apple
systems), and Sagitario cannot name a global variable of C: ADR-0083 deferred
`extern "C" { static ...; }` (Q534). On Windows the streams come from a function
(`__acrt_iob_func`). Rust declares C's globals in `extern` blocks, as statics; reading or writing
one is always `unsafe`, since C may change it at any time.

Writing to the file descriptors directly, as Rust's std does, would need buffers of Sagitario
code, a lock for stdin, and a flush of C's buffers before each write while the prelude still uses
them. That is the model for when S3 moves the core; until then it would be more code and slower
writes.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q557 | Syntax | An `extern "C"` block may declare C's global variables: `static NAME: T;` and `static var NAME: T;`, with `pub` as its functions take, and `@cfg` on each (ADR-0085). The name is the C symbol, as a function's is. |
| Q558 | Use | As in Rust, reading a C global, or calling its methods, happens only in `unsafe` (`E0323`): C may change it at any time, from any thread. A `static var` may also be assigned in `unsafe`, as a whole or by its fields; assigning a `static` is an error (`E0307`). |
| Q559 | Types | The type of a C global is FFI-safe (docs/spec/10-ffi.md#ffi-safe-types), as the parameters of a C function are (`E0322`). |
| Q560 | Addresses | `&NAME` and `&mut NAME` are errors (`E0340`), as on a `static var` (ADR-0083, Q528). `&raw const NAME` gives the address of any C global, and `&raw mut NAME` that of a `static var`; taking an address is safe. |
| Q561 | Symbols | The symbol is checked as a function's is: a C identifier, not reserved, except in the standard library, and not used for something else (`E0208`). |
| Q562 | The C code | The FFI unit (`sag_ffi.c`) declares each C global used and gives its address through a function, `sag_ffi_static_N`, so that the program's C, which includes the runtime's header, never declares C's names. |
| Q563 | The editor | Hover shows `extern static NAME: T` (or `extern static var`). A snippet `externstatic` declares one in a block (CLAUDE.md). |
| Q564 | Increments | **1** C globals in the FFI: the parser, the resolver, the checks of Q558–Q561, SIR and the C of Q562, the editor and tests with real globals of the C library. **2** `std::io` in Sagitario over C's stdio: `std::sys` reaches `stdin`, `stdout` and `stderr` per C library. `std::io` writes, flushes and reads with `fwrite`, `fflush`, `getc` and `fread`, and tells terminals with `isatty` and `GetConsoleMode`. The kinds and messages of errors (`std::os_error`) map `errno` codes and use `strerror`. The C of `std::io` is deleted from the runtime. |

### Example

```sag
use std::ffi::{c_char, c_int, c_void};

@cfg(any(target_os = "linux", target_os = "android"))
extern "C" {
  static stdout: *mut c_void;
  fn fputs(text: *const c_char, stream: *mut c_void) -> c_int;
}

@cfg(any(target_os = "linux", target_os = "android"))
fn stdout_stream() -> *mut c_void {
  unsafe { stdout }
}
```

## Alternatives considered

- **File descriptors without stdio, as Rust's std:** the end state once S3 moves the prelude's
  printing to Sagitario, but until then each write must flush C's buffers first, stdin needs its
  own buffer behind a lock, and `io::print` loses its buffer.
- **A function of the core giving the streams (`sag_stdio(1)`):** no change to the language, but it
  keeps C in the runtime for the standard library, against S1, and C libraries wrapped in Sagitario
  would still lack their globals.
- **Reading a C `static` without `unsafe`:** Rust requires `unsafe` for every C global, since C may
  write even one declared `const`; Sagitario follows it.

## Consequences

- C libraries whose API includes globals (`environ`, `optarg`, `stdout`) can be wrapped in
  Sagitario alone.
- `std::io` keeps the buffer, the order with the prelude and the locks of C's stdio, until S3.
- The FFI unit grows a function per C global used.

## Notes

- Increment 1 (C globals) is done: docs/spec/10-ffi.md#c-globals, the test
  `nova::reads_and_writes_the_globals_of_c`, which reads and writes a number, a pointer and a
  struct of a C source of the package on every platform, and `tests/compile-fail/c_global_errors.sag`.
  The parser makes each C global a `StaticDecl` with `extern_c`, an item of the module, so that
  the resolver, the checks of `static` and `static var`, SIR and the hover treat it as a static.
  Its function has no body, and is marked abstract, so that nothing checks it and SIR only makes a
  stub that is never called. The getter of the static returns the address that
  `sag_ffi_static_N` gives in the FFI unit. Windows' C library has no global variables, only
  functions, so the test defines its own in C.
- Increment 2 (`std::io`) is done. `std::sys` reaches C's streams (`stdin`, `stdout` and `stderr`
  in glibc and bionic, `__stdinp`... on Apple systems, `__acrt_iob_func` on Windows), `errno`
  (`_errno` on Windows), `strerror`, `isatty` and `GetConsoleMode`. `std::io` writes, flushes and
  reads with the C library's functions, and checks UTF-8 itself. `std` maps the codes of errors to
  their kinds and messages (`os_error`, `last_error`), for `std::fs` too, whose C still returns
  `errno` codes. The C of `std::io` (about 160 lines) is deleted from the runtime. The C
  declarations of the standard library, functions and globals, go to a unit of their own,
  `sag_std_ffi.c`, and its symbols are checked apart from the program's: a program may declare
  the same C function with other types, as `tests/run-pass/ffi_buffers.sag` declares `fwrite`.

