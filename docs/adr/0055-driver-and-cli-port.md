# 0055. The driver and CLI port

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0054](0054-lowering-and-c-generation-port.md)

## Context

F5 increment 7 ports the last part of the compiler: the driver (`sagitario_driver`, which runs
the pipeline, writes the C with the embedded runtime and calls the C compiler) and the command
line (`sagitario_cli`). With it, `sagc` builds and runs programs instead of only dumping the
results of its passes. Increment 7 settles Q275–Q284.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q275 | Scope | `sagc` has the command line of `sag`: `build`, `run`, `check` (with the hidden dumps), `targets`, `help` and `version`, with `--release`, `--sanitize[=thread]`, `--emit-c`, `--cc`, `--target` and `--diagnostic-format`, the same messages and the same exit codes (0, 1, 2 and 101). Libraries, dependencies and `[link]` are not on that command line: Nova passes them to the driver inside its own process, so they stay in Rust, and so does the library header (`header.rs`). Porting Nova is outside F5. |
| Q276 | Stack | Every executable reserves an 8 MiB stack on Windows (`-Wl,/STACK:8388608` with clang for MSVC, `-Wl,--stack,8388608` with MinGW, `/STACK:8388608` with `cl`), the default of Linux and macOS, in both compilers. Programs, the self-hosted compiler among them, recurse as deeply on every system; Windows only reserves the address space. A `std::thread` option for the size of a stack would be a larger language change. |
| Q277 | Colors | Sagitario cannot tell whether stderr is a terminal, so `sagc` colors diagnostics only with `CLICOLOR_FORCE`, honoring `NO_COLOR`. An `io::is_terminal` belongs to the standard library, later. |
| Q278 | The suites with `sagc` | `crates/sagitario_cli/tests/suites.rs` runs `SAG_COMPILER`, when set, instead of the `sag` built by Cargo: the `run-pass`, `run-panic` and `compile-fail` suites run against `sagc`. Increment 7 checks it locally with the four C compilers; the CI job comes with increment 8. The tests of `cli.rs` stay about `sag`. |
| Q279 | Runtime | The driver embeds `runtime/sagitario_runtime.h` and `.c` with `include_str`, as Rust does. |
| Q280 | Temporary files | The standard library has no `temp_dir` and no process id. `sagc` finds the directory as Rust's `env::temp_dir` does (`TMP`, `TEMP`, `USERPROFILE` on Windows; `TMPDIR`, then `/tmp`, elsewhere) and creates `sag-<nanoseconds>-<n>` with `fs::create_dir`, trying the next `n` when another process took the name. The directory holds files only, so `read_dir`, `remove_file` and `remove_dir` remove it. |
| Q281 | Messages of the system | Failures of the operating system (a C compiler that cannot run, a file that cannot be written) name `io::Error::message`, which may differ from Rust's text. Every other message is the same. |
| Q282 | `run` | Exit codes of the program outside 0–255, and programs ended by a signal, give 1, as in Rust. With sanitizers on Windows, the directory of their runtime goes first in the program's `PATH`, through `Command::env`. |
| Q283 | Files | `compiler/src/driver.sag` (the pipeline, the toolchain and finding the C compiler), `compiler/src/driver/targets.sag` (Android through the NDK, iOS through `xcrun`) and `compiler/src/main.sag`, the command line. `TargetInfo` moves to `ast.sag` with `Package::target`, as in Rust, so `--target` reaches the type checker and the lowering. |
| Q284 | Rust changes | The stack of Q276 in the driver and `SAG_COMPILER` in the harness. |

## Alternatives considered

- **A `std::thread` builder with a stack size, and `sagc` working on such a thread as the Rust
  compiler does (ADR-0050, Q235):** a new standard library API, and programs would still recurse
  less deeply on Windows than elsewhere.
- **Porting Nova too:** a second program, with TOML parsing and dependency resolution, that the
  bootstrap does not need.

## Consequences

- `sagc`, built by `sag`, passes the `run-pass`, `run-panic` and `compile-fail` suites with
  official clang, gcc, MinGW clang and `cl`; the acceptance of F3 runs through Nova and stays with
  `sag`.
- `sagc` ends through `process::exit` also on success: the leak check of `SAG_LEAK_CHECK`, which
  the harness sets for the programs `sagc` runs, would otherwise look at the compiler itself.
- `sagc` is told what it runs on at run time: Windows from how `std::path` joins paths (as the
  loader already did), macOS from a file only it has, and 64-bit pointers always.
- On Windows `sagc`, like every Sagitario program, writes `\r\n` line endings to stdout and
  stderr; the tests compare text with line endings normalized.
- `sagc version` prints the version of `sag` it implements, `0.4.0`, and its help names `sagc`.
