# Testing

Rationale: [ADR-0016](adr/0016-engineering-process.md).

## Levels

| Level | Location | What it checks |
|---|---|---|
| Unit | inside each crate | Lexer tokens, parser AST, type errors, SIR, C output. Snapshot tests with `insta`. |
| `run-pass` | `tests/run-pass/*.sag` | Program compiles, runs, and matches expected stdout and exit code. |
| `compile-fail` | `tests/compile-fail/*.sag` | Compiler rejects the program with the expected diagnostic codes at the expected lines. |
| `run-panic` | `tests/run-panic/*.sag` | Program panics with the expected message and exit code 101. |

The end-to-end suites run through an own harness (in the style of rustc's `compiletest`)
invoked by `cargo test --workspace`. They require a C compiler (see
[architecture](architecture.md#c-toolchain)).

## Annotations

Annotations are comments inside the `.sag` test file. (The comment syntax is still open
question Q1 in the spec; `//` is assumed here.)

```sag
// expect-stdout: total = 6
// expect-exit: 0
pub fn main() { ... }
```

```sag
const x: i32 = "hello";   //~ ERROR E0042
```

```sag
// expect-panic: index out of bounds
pub fn main() { ... }
```

- `expect-stdout` may appear several times; lines are matched in order.
- `//~ ERROR Ennnn` asserts a diagnostic with that code on the same line. Every expected
  diagnostic must appear and no unexpected error may appear.

## Memory checks

- In test mode the runtime counts live ARC objects; a non-zero count at exit fails the test.
- Where supported, test programs are compiled with ASan and UBSan.

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
clang, plus a Linux job with `SAG_CC=gcc`.
