# 0015. Compiler architecture and C toolchain

- Status: Accepted
- Date: 2026-10-21

## Context

The compiler will be rewritten in Sagitario (F5). Anything it depends on must be
re-implementable, and the code must be easy to port. The C backend needs a supported C
compiler on every desktop OS.

## Decision

- Cargo workspace of `sagitario_*` crates (span, diagnostics, lexer, ast, parser, resolve,
  types, sir, codegen_c, driver, cli), plus `runtime/` (minimal C, embedded in the binary),
  `std/` (`.sag`) and `tests/`. Details in [architecture.md](../architecture.md).
- Hand-written lexer and recursive-descent/Pratt parser.
- Zero external dependencies in the core crates; dependencies allowed only in the CLI and as
  dev-dependencies.
- The executable is `sag`.
- Official C compiler: clang on Windows (UCRT + MSVC linker), Linux and macOS; gcc also in CI
  on Linux. Generated code is strict, portable C11. Detection: `SAG_CC` / `--cc`, then
  `clang`, then `gcc`. MSVC `cl.exe` and `zig cc` are re-evaluated in F3.

## Alternatives considered

- **Parser generators (pest, lalrpop)**: worse diagnostics and not portable to self-hosting.
- **Supporting clang, gcc and MSVC from day one**: three dialects and flag sets.
- **Bundling `zig cc`**: great cross-compilation, but a large pre-1.0 binary dependency.
- **Binary named `sagitario`**: explicit but long to type.

## Consequences

- Each phase is testable in isolation.
- Windows contributors need LLVM plus Visual Studio Build Tools.
- Keeping C11 strict leaves the door open to MSVC and `zig cc` later.
