# 0112. The language server sees packages as Nova builds them, and shows inferred types

- Status: Accepted
- Date: 2026-11-21
- Refines: [ADR-0058](0058-language-server.md) (the program of a file),
  [ADR-0103](0103-build-scripts-and-binding-generation.md) (build scripts and `bindgen`),
  [ADR-0109](0109-language-server-members.md) (the last analysis that reached the type checker)

## Context

The language server compiled the program of a file as `sag check` does without Nova. Code that
Nova builds without errors was full of them in the editor:

- in `build.sag`, `bindgen::Builder` was "cannot find `bindgen`": Nova gives build scripts the
  library `bindgen` as a dependency, and the server gave none;
- `include!(concat!(env!("OUT_DIR"), "/sdl.sag"))` was "the environment variable `OUT_DIR` is not
  set": Nova passes it with `--env`. The bindings were not included, so every C function they
  declare, such as `PostQuitMessage` inside `unsafe`, was "not found in this scope";
- the dependencies of `Nova.toml` were not loaded either;
- the error of `env!` is found while parsing, so every analysis counted as one with syntax errors,
  and the analysis kept for hover (ADR-0109) was never replaced: after the file was opened, new
  bindings had no hover.

Users of rust-analyzer also expect the types the compiler infers to be shown next to the code.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q799 | Packages | For a file of a package, the server reads `Nova.toml` and the `Nova.toml` of every dependency, by path, and loads them as `nova check` does: the same names, the same graph. A manifest it cannot read leaves the package without dependencies; `nova check` reports why. |
| Q800 | Build scripts | `build.sag` is analysed with the library `bindgen` as its only dependency, from the copy embedded in the server, at the path where Nova writes it (`target/debug/build/.bindgen/lib.sag`). |
| Q801 | `OUT_DIR` | Each package with a `build.sag` sees in `env!` the `OUT_DIR` Nova gives it: `target/debug/build/<name>/out` of the package being analysed (`target/release/...` when only that one exists). When it does not exist, the server runs `nova check` in the package once, as rust-analyzer runs build scripts, so the files the scripts write exist; it runs it again when `build.sag` is saved. `nova` is the one next to `sag`, else on `PATH`; if it cannot run, the error of `env!` stays. |
| Q802 | Syntax errors | Only lexical and syntax errors (`E0001`-`E0199`) make an analysis one that skipped code; errors of macros (`E0335`, `E0346`) do not, so the analysis kept for hover follows the text. |
| Q803 | Inlay hints | `textDocument/inlayHint` shows `: Type` after each binding written without a type (`const`, `var`, the bindings of patterns, `for`, `if let`, `while let`, `match`) and after each parameter of a closure without one, from the last analysis that reached the type checker. Types that hold an error are not shown. |
| Q804 | `env!` | Inside the string of `env!("` completion offers `OUT_DIR` for a package with a build script and the variables of the server's environment, with their values as details. |
| Q805 | Unknown types | Hover on a binding whose type holds an error shows `{unknown}`, as rust-analyzer does, instead of nothing. |

## Alternatives considered

- **Silencing `env!` in the server**: the included bindings would still be missing.
- **Running the build scripts on every change**: slow, and a build script may do anything; only
  when their output is missing or `build.sag` changed.
- **Inlay hints for parameter names and chains**: they can come later, as their own decision.

## Consequences

- Opening a package whose bindings were never generated runs its build scripts, as `nova check`
  would.
- The server's results match what `nova check` reports for packages, dependencies and build
  scripts.
