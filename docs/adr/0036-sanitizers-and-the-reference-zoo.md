# 0036. Sanitizers and the reference "zoo" program

- Status: Accepted
- Date: 2026-10-23
- Refines: [ADR-0016](0016-engineering-process.md) (ASan/UBSan in tests),
  [ADR-0018](0018-windows-c-toolchain.md) (sanitized programs on Windows)

## Context

F2 is accepted when a reference "zoo" program passes with zero live ARC objects at exit and
clean ASan (docs/roadmap.md). The runtime already counts live objects (`SAG_LEAK_CHECK`).
ADR-0016 and docs/testing.md planned ASan and UBSan "where supported", and ADR-0018 found that
only official LLVM clang provides them on Windows. Increment 8 settles Q78–Q83.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q78 | Activation | `sag build --sanitize` and `sag run --sanitize` compile with `-fsanitize=address,undefined -fno-sanitize-recover=undefined`, so any undefined behavior aborts the program. |
| Q79 | Support | Decided by the target from `-dumpmachine`. MinGW targets have none: the program is built without sanitizers after the usual MinGW warning. Others (gcc and clang on Linux, clang on macOS, clang-msvc on Windows) use them. If the C compiles without sanitizers but not with them, `sag` reports that the toolchain lacks them, not an internal compiler error. On Windows, `sag run` puts the sanitizer runtime directory (from `-print-resource-dir`) on the program's `PATH`, and `sag build` prints it. |
| Q80 | Test harness | Every `run-pass` and `run-panic` case runs with `--sanitize`; a sanitizer report fails the test through its exit code. `SAG_TEST_SANITIZE=0` turns it off locally. |
| Q81 | Zoo program | `tests/run-pass/reference_zoo/`, a program of four modules with a fixed output. It uses an abstract class and three subclasses (`zod.new`, `zod.method`, `override`), interfaces as types (a struct boxed as one too), a default method, bounded generics, `List`, `Map`, lambdas with `map`/`filter`/`fold`, sorting, a `weak` back-reference, a closure in a field, `Error` with `?`, and imports. |
| Q82 | CI | The existing jobs run the harness, so they run sanitized. The Windows job sets `SAG_CC` to the runner's LLVM clang. |
| Q83 | Closing F2 | The roadmap marks F2 done, and the README's status reflects that F0–F2 work. |

## Alternatives considered

- **An environment variable instead of a flag:** less discoverable. The harness sets the flag
  itself.
- **Sanitizers always on in debug builds:** slower builds for everyone, and impossible with
  MinGW.
- **A separate sanitizer CI job:** developers would not see failures locally.

## Consequences

- The first sanitized run of the suite found a miscompilation. Iterator methods called on a
  list inside generic code passed an unsubstituted element type to C, and UBSan reported the
  call through a mistyped function pointer. The zoo's generic `chorus` keeps it covered.
- Tests take longer to compile; `SAG_TEST_SANITIZE=0` restores the previous speed.
