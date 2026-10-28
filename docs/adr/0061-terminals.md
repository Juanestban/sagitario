# 0061. Terminals: `io::is_terminal` and colored diagnostics in `sagc`

- Status: Accepted
- Date: 2026-10-26
- Supersedes: Q277 of [ADR-0055](0055-driver-and-cli-port.md)

## Context

The Rust `sag` colors diagnostics when stderr is a terminal. `sagc` could not tell, since
Sagitario had no way to know, so Q277 colored only with `CLICOLOR_FORCE`: running a program with
`sagc` lost the colors that `sag` showed. As `sagc` becomes `sag`, it needs Rust's rule.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q334 | API | `io::Stream { Stdin, Stdout, Stderr }` and `io::is_terminal(stream: io::Stream) -> bool`: whether a standard stream is connected to a terminal rather than to a file or a pipe. Rust's `IsTerminal`; an enum, since Sagitario has no objects for the streams. |
| Q335 | Runtime | `isatty` on Linux and macOS. On Windows a console has a console mode (`GetStdHandle`, `GetConsoleMode`), as Rust checks: the CRT's `_isatty` also answers yes for `NUL` and other character devices. Both are declared by the runtime, which stays strict C11. |
| Q336 | When `sagc` colors | Rust's rule: never with `NO_COLOR` or `--diagnostic-format short`; always with `CLICOLOR_FORCE` (other than `0`); otherwise when stderr is a terminal, and on Windows only in terminals known to interpret ANSI codes (`WT_SESSION`, `TERM`, `TERM_PROGRAM`, `ConEmuANSI`), which VS Code and Windows Terminal set. |
| Q337 | Documentation and tests | This record, the specification of `std::io`, and `tests/run-pass/std_is_terminal.sag`: the harness connects the three streams to pipes, which are not terminals. |

## Consequences

- `sagc` colors diagnostics in a terminal as `sag` does.
- MSYS terminals such as mintty connect programs through pipes, which are not consoles; there,
  as before, `CLICOLOR_FORCE=1` asks for colors.
