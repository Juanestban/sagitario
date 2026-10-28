# 0066. The test harness in Sagitario

- Status: Accepted
- Date: 2026-10-27
- Refines: [ADR-0060](0060-driver-parity-for-nova.md) (the tests, Q327; the modules made from
  Rust, Q329) and [ADR-0064](0064-nova-in-sagitario.md) (the tests of Nova, Q348)

## Context

Increment 3c of F6 ports the end-to-end tests from Cargo to a program in Sagitario, so that 3d can
remove Rust and run CI without `cargo`. Those tests are the `.sag` suites
(`crates/sagitario_cli/tests/suites.rs`), the command line (`cli.rs`), the grammar of the editor
(`editor.rs`), the sessions of the language server and the comparison of `sag` with `sagc`
(`selfhost.rs`), and Nova (`crates/sagitario_nova/tests/nova.rs`). Q327 replaces the comparison
with Rust by expected dumps of the corpus of each pass.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q356 | The program | `tests/harness/` (`main.sag`, with a module per group), run from the root of the repository: `sag run tests/harness/main.sag -- --sag <compiler> [--nova <nova>] [group...]`. The groups are `suites`, `dumps`, `lsp`, `editor`, `cli` and `nova`, all of them by default. `--sag` falls back to `SAG_COMPILER`, `--nova` to `NOVA_BINARY`; without either, the harness builds Nova from `compiler/src/nova_main.sag` with the compiler under test, and Nova runs that compiler through `SAG`. It prints `test <group>::<name> ... ok` or `FAILED` per test, then the failures and a summary; it exits with 1 if a test fails and 2 for a usage error. `SAG_BLESS=1` and `SAG_TEST_SANITIZE=0` keep their meaning. |
| Q357 | What it ports | The suites with the same annotations, and the check that every code of `compiler/src/diagnostics/codes.sag` has a `compile-fail` case (that file is already the registry the compiler in Sagitario uses, Q329); the sessions of the language server against the same `.expected` files; the keywords of the editor; the command line; every test of Nova, with every code of `compiler/src/nova/codes.sag` covered. They test the same things, with two changes: the version may say `sagc` until 3d renames it, and the input that is not a `.sag` file is `README.md`, since `Cargo.toml` goes in 3d. |
| Q358 | Expected dumps | For each case of `tests/lexer`, `tests/parser`, `tests/resolve`, `tests/types` and `tests/codegen` (a file, or a directory with `main.sag`), `<case>.expected` next to it holds the exit code, stdout and stderr of `sag check --dump=<pass> --target x86_64-linux-android`, with the dump of its own pass (`tokens`, `ast`, `resolve`, `types`, `c`) and the diagnostics in the human format without colors. One target on every host keeps the sizes of `usize` and of the C types, and the C, the same everywhere; `\` in the diagnostics becomes `/`. `SAG_BLESS=1` rewrites them, so a change to the compiler shows as a diff of these files. |
| Q359 | Bytes of files | `std::fs` reads UTF-8 text only, and the Android tests of Nova check the ELF class and machine of what they build: the harness compiles a small C program with the C compiler of the tests, which prints them. |
| Q360 | What stays in Cargo until 3d | The comparison of `sag` and `sagc`, the modules of `compiler/` made from Rust, the unit tests of the crates (all retired with Rust, Q328), and the fixed point of the bootstrap (it moves to the scripts of the bootstrap, Q326). The Cargo tests that the harness ports run as before until then. |
| Q361 | CI | The `bootstrap` job runs the whole harness with stage 1 on each of its systems and C compilers, instead of the Cargo suites and Nova tests with `SAG_COMPILER` and `NOVA_BINARY`. |

## Alternatives considered

- **Comparing dumps with Rust until 3d, and writing the expected files then:** the expected files
  would arrive with the removal of Rust, in the change that most needs them.
- **Dumps for the host:** the expected files would differ between systems, with one set per
  system.
- **Short diagnostics in the dumps:** the human format also covers labels, notes, help and the
  excerpts of the source.
- **Reading bytes through FFI (`fopen`):** the harness would declare by hand functions of the C
  library that the runtime already includes from `<stdio.h>`; a `std::fs::read` for bytes is a
  change of the library, for a later increment.

## Consequences

- 22 expected files (about 780 KB) under `tests/`. The resolution and type dumps include the
  prelude, so a change to it rewrites them; `SAG_BLESS=1` does that in one run.
- The harness is a program of several modules written in Sagitario, so the compiler under test
  must build it before testing anything.
