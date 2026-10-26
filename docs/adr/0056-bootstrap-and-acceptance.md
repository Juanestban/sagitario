# 0056. The bootstrap and the acceptance of F5

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0055](0055-driver-and-cli-port.md)

## Context

With increment 7, `sagc` builds and runs programs. F5 increment 8 closes the phase: the
bootstrap of [ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md) (Q213: stage 0 is
the Rust compiler, stage 1 is `sagc` built by stage 0, stage 2 is `sagc` built by stage 1, and
stages 1 and 2 generate the same C for the compiler), its CI job, and the acceptance of Q219:
stage 1 passes every suite on the three systems with the three C compilers, and stage 2 reaches
the fixed point. Locally, on Windows, the fixed point already held when the increment started.
Increment 8 settles Q285–Q292.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q285 | The fixed point test | `bootstrap_reaches_the_fixed_point` in `crates/sagitario_cli/tests/selfhost.rs` builds stage 1 with `sag`, copies it out of `compiler/src/` (Windows cannot replace a running executable), builds stage 2 with that copy, and compares the `--dump=c` of `compiler/src/main.sag` of both, with line endings normalized. It runs with every `cargo test`, so every CI job checks it with its C compiler. It takes turns with `sagc_dumps_the_same_results`, which also writes `sagc` to `compiler/src/`. |
| Q286 | The CI job | A `bootstrap` job builds stage 1, runs `cargo test --test suites` with `SAG_COMPILER` set to it, and runs the fixed point test, on Windows with official clang, `cl` and MinGW gcc, on Ubuntu with clang and gcc, and on macOS with clang. It is separate from the jobs of `cargo test --workspace`, so local runs do not take twice as long. |
| Q287 | MinGW in CI | The acceptance of F5 names MinGW, which no job tested. The `bootstrap` job adds it with the MinGW-w64 gcc of the Windows runners (`C:\mingw64\bin\gcc.exe`), without sanitizers, best-effort as before (ADR-0018). |
| Q288 | Acceptance | F5 is accepted when the `bootstrap` job and the fixed point test pass on the three systems. Linux and macOS are only checked through CI. |
| Q289 | Retiring the Rust compiler | Not decided here. As Q214 says, a decision record after F5 settles whether the Rust compiler retires, for example by bootstrapping from committed generated C, and with it the rename of `sagc` to `sag` (Q212). Both are deferred decisions of the roadmap. |
| Q290 | Performance | Informational (Q218). Generating the C of the compiler (`check --dump=c compiler/src/main.sag`, about 261 000 lines of C) took 2.0 s with a debug `sag` and 4.8 s with a debug stage 1; 1.2 s and 2.1 s in release builds (Windows, official clang). |
| Q291 | Documentation | This record; the roadmap (increment 8, the acceptance of F5 and the deferred decisions); `docs/testing.md` (the test and the job); `CONTRIBUTING.md` (bootstrapping by hand). |
| Q292 | Rust changes | Only the test and the job; neither the compiler nor the language changes. |

## Alternatives considered

- **Running the suites with stage 1 inside `cargo test`:** every local run would compile and run
  every case twice.
- **A bootstrap script instead of a test:** a Rust test runs on every system and in every CI job
  without a shell of its own.

## Consequences

- The bootstrap is checked on every pull request, with every C compiler of the CI.
- Language changes still go into both compilers (Q214) until the Rust compiler retires.
