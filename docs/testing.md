# Testing

Rationale: [ADR-0016](adr/0016-engineering-process.md).

## Levels

| Level | Location | What it checks |
|---|---|---|
| Unit | inside each crate | Lexer tokens, parser AST, type errors, SIR, C output. Snapshot tests with `insta`. |
| `run-pass` | `tests/run-pass/*.sag` | Program compiles, runs, and matches expected stdout and exit code. |
| `compile-fail` | `tests/compile-fail/*.sag` | Compiler rejects the program with the expected diagnostic codes at the expected lines. |
| `run-panic` | `tests/run-panic/*.sag` | Program panics with the expected message and exit code 101. |

A case is a single `.sag` file, or a directory holding a multi-file program whose entry file
is `main.sag` (for example `tests/run-pass/modules/main.sag` with `geometry.sag` and
`util/strings.sag` beside it).

The end-to-end suites run through an own harness (in the style of rustc's `compiletest`),
`crates/sagitario_cli/tests/suites.rs`, invoked by `cargo test --workspace`. It runs the built
`sag` binary: `sag run <file>` for `run-pass` and `run-panic`, and
`sag check --diagnostic-format short <file>` for `compile-fail` (expected exit code 1). They require a C compiler (see
[architecture](architecture.md#c-toolchain)).

## Annotations

Annotations are line comments inside the `.sag` test file.

```sag
// expect-stdout: total = 6
// expect-exit: 0
fn main() { ... }
```

```sag
const x: i32 = "hello";   //~ ERROR E0042
```

```sag
// expect-panic: index out of bounds
fn main() { ... }
```

- `expect-stdout` may appear several times; lines are matched in order. In a multi-file case,
  `expect-*`, `sag-flags`, `args`, `stdin` and `env` annotations are read from `main.sag`, and `//~` annotations
  from every file of the program (each one matched against its own file).
- `// sag-flags: --release` passes extra options to `sag run` (e.g. to test release-mode
  wrapping).
- `// sanitize: thread` runs the case with `--sanitize=thread` instead of `--sanitize`:
  ThreadSanitizer on Linux and macOS, ASan and UBSan elsewhere
  ([ADR-0045](adr/0045-threads-and-send.md)).
- `// args: a b` passes command-line arguments to the program, split on spaces.
  `// stdin: text` writes a line (`text` and `\n`) to its stdin and may repeat; stdin is closed
  after the last line, or at once without any. `// env: NAME=value` sets an environment
  variable for the program and may repeat.
- `run-pass` and `run-panic` programs run in a new empty working directory, removed afterwards,
  so they can create files with relative paths.
- Panics print `panic at <file>:<line>: <message>`; `expect-panic` matches a substring.
- `//~ ERROR Ennnn` and `//~ WARN Wnnnn` assert a diagnostic with that code on the same line.
  Several can share a line: `//~ ERROR E0311, WARN W0001 WARN W0002`.
- In `compile-fail`, every reported error **and warning** must be annotated, and every
  annotation must be reported. A file with only `WARN` annotations must compile (exit code 0);
  otherwise the exit code must be 1. Prefix incidental bindings with `_` to keep tests focused.
- `every_diagnostic_code_has_a_compile_fail_test` fails if a code in
  `crates/sagitario_diagnostics/src/codes.rs` has no `compile-fail` case.

## Memory checks

- The runtime counts live ARC objects. When the environment variable `SAG_LEAK_CHECK` is set
  (the harness always sets it), a program that exits normally with objects still alive prints
  `leak check: N ARC object(s) still alive at exit` and exits with code 102, failing the test.
- The harness runs every `run-pass` and `run-panic` case with `sag run --sanitize`: ASan and
  UBSan, with any undefined behavior aborting the program, so a report fails the test. Toolchains
  without sanitizers (MinGW) build without them. `SAG_TEST_SANITIZE=0` turns them off for faster
  local runs ([ADR-0036](adr/0036-sanitizers-and-the-reference-zoo.md)).
- `tests/run-pass/reference_zoo/` is the reference program of the F2 acceptance criteria.
- `tests/acceptance/f3/` is the F3 acceptance program: the `wc` and `textcount` packages and a C
  program using the library, run by the Nova test `f3_acceptance`
  ([ADR-0044](adr/0044-c-toolchains.md)).

## Rules

- The compiler must never panic on user input. Any internal compiler error found is fixed
  and gets a regression test.
- Every language feature needs at least one `run-pass` test, and one `compile-fail` test per
  rule a program can violate.
- Benchmarks (fibonacci, n-body, binary-trees versus equivalent C) are informational and do
  not gate the MVP.

## CI

GitHub Actions matrix on Windows, Linux and macOS running `cargo fmt --check`,
`cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace` with
clang, plus a Linux job with `SAG_CC=gcc` and a Windows job with `SAG_CC=cl` (without
sanitizers). The Ubuntu job requires the Android NDK (`SAG_REQUIRE_NDK=1`).
