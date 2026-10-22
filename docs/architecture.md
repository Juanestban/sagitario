# Compiler architecture

How the `sag` compiler is structured. Rationale: [ADR-0001](adr/0001-compile-to-c-via-sir.md),
[ADR-0015](adr/0015-compiler-architecture-and-toolchain.md).

## Pipeline

```
.sag files ─▶ lexer ─▶ parser ─▶ AST ─▶ resolve ─▶ types ─▶ SIR ─▶ codegen_c ─▶ .c ─▶ clang/gcc ─▶ executable
                                                                       ▲
                                                         runtime/ (C) ─┘ compiled and linked together
```

Phases communicate only through data structures (tokens, AST, typed AST, SIR). There is no
global mutable state; each crate can be tested in isolation.

## Repository layout (target)

```
Cargo.toml          virtual workspace; shared versions in [workspace.dependencies]
crates/
  sagitario_span/         source files, file ids, byte offsets, spans, line/column mapping
  sagitario_diagnostics/  diagnostics (code E0001…, severity, spans, notes) and rendering
  sagitario_lexer/        hand-written lexer: text → tokens
  sagitario_ast/          AST definitions
  sagitario_parser/       recursive descent + Pratt parsing for expressions: tokens → AST
  sagitario_resolve/      module tree from files, `use`, name resolution, visibility
  sagitario_types/        type checking, local inference, generics, inheritance, interfaces
  sagitario_sir/          SIR definition + lowering from typed AST + retain/release insertion
  sagitario_codegen_c/    SIR → C11 text
  sagitario_driver/       orchestrates the pipeline, embeds the runtime, invokes the C compiler
  sagitario_cli/          the `sag` binary
runtime/            minimal C runtime (ARC, panic, String, allocation, basic I/O)
std/                standard library written in Sagitario
tests/              end-to-end suites (see testing.md)
```

## Crate dependency rule

Dependencies flow strictly forward through the pipeline:

```
span ◀─ diagnostics ◀─ lexer ◀─ parser ─▶ ast
                                  ▲
resolve ◀─ types ◀─ sir ◀─ codegen_c ◀─ driver ◀─ cli
```

- A crate never depends on a later phase.
- Core crates (`sagitario_span` through `sagitario_codegen_c`) have **zero external
  dependencies**. `sagitario_driver` should stay dependency-free too; `sagitario_cli` may use
  CLI crates. Dev-dependencies (e.g. `insta`) are allowed everywhere.
- Rust code is written to be portable to Sagitario (F5): plain structs, enums, functions and
  simple traits; no proc macros or elaborate trait-level generics.

## SIR

Sagitario Intermediate Representation: a typed, lower-level form where

- generics are already monomorphized,
- ARC retain/release are explicit instructions,
- method calls are resolved to direct calls or vtable calls (devirtualized when no override
  exists),
- `?`, `match` and closures are lowered to explicit control flow and environments.

SIR is structured (`If`/`Loop`/`ForRange` rather than a control-flow graph), since it is
emitted as C. ARC is explicit: every `String` binding owns one reference and is released at the
end of its block, before `break`/`continue`/`return`, or when overwritten (the new value is
retained first); temporaries are released at the end of the statement or branch that created
them; parameters are borrowed; return values are owned by the caller.

SIR is the place for ARC optimizations and the boundary behind which other backends (LLVM)
could be added.

## C code generation

- Output is strict, portable **C11**: no GCC/clang extensions (no statement expressions, no
  bare `__attribute__`; any compiler-specific feature hides behind a runtime macro with a
  portable fallback).
- **No undefined behavior**: signed arithmetic goes through runtime helpers that detect
  overflow (panic in debug, wrap in release); shifts, division and indexing are checked.
- `#line` directives map C back to `.sag` sources.
- `sag build --emit-c` writes the generated C for inspection.
- Value types become C structs; enums become tagged unions; `Option<ClassRef>` is a nullable
  pointer; `Result` is a tagged union.

## Runtime

Minimal C, embedded in the `sag` binary and compiled with every program:

- ARC object header and retain/release (non-atomic in the MVP),
- panic (message, file, line, exit code 101),
- `String` storage and UTF-8 helpers, allocation,
- basic I/O used by `std`,
- overflow-checked integer helpers (`sag_add_i32_at`, ...) that panic in debug and wrap when
  compiled with `-DSAG_RELEASE` (`sag build --release`), saturating float-to-integer casts,
- a string builder used by template strings, with shortest round-trip float formatting,
- a live-object counter checked at exit when `SAG_LEAK_CHECK` is set (exit code 102).

Anything expressible in Sagitario belongs in `std/` instead.

## C toolchain

- Official compiler: **clang** on Windows (UCRT + MSVC linker), Linux and macOS; **gcc** is
  also tested on Linux.
- Detection order: `SAG_CC` environment variable or `--cc` flag, then `clang` on `PATH`, then
  `gcc`. If none is found, `sag` fails with installation instructions per OS.
- Debug builds of tests use `-fsanitize=address,undefined` where supported.
- Windows ([ADR-0018](adr/0018-windows-c-toolchain.md)): the supported compiler is official
  LLVM clang targeting `x86_64-pc-windows-msvc`. MinGW clang (`*-windows-gnu`, detected with
  `-dumpmachine`) is tolerated with a warning and without sanitizers. When running sanitized
  programs, the driver adds the sanitizer runtime directory to the child `PATH`.

## CLI

`sag build`, `sag run`, `sag check` (front end only, no C compilation), with `--emit-c` and
`--cc` (build and run only), `--diagnostic-format human|short`, and `sag run file.sag -- args`
to forward arguments to the program.

- `human` (default) renders multi-line diagnostics with source excerpts; `short` prints one
  line per diagnostic, `path:line:column: error[E0001]: message`, for tools and the test
  harness.
- Exit codes of `sag` itself: `0` success, `1` compilation failed or input unreadable, `2`
  invalid command-line usage. `sag run` returns the program's own exit code once it runs.

- `sag build main.sag` writes the executable next to the source (`main` / `main.exe`);
  `--emit-c` also writes `main.c` there (it includes `sagitario_runtime.h` from `runtime/`).
  `sag run` builds into a temporary directory that is removed afterwards.
- The driver stops after lexing or parsing if they report errors (an incomplete syntax tree
  would only cause confusing follow-up errors); otherwise resolution, type checking and flow
  analysis run together, so one run reports all their errors and warnings.
- Diagnostics are printed in source order. They carry secondary labels ("previous definition
  here", "expected because of this type"), notes and `help:` suggestions, including "a name
  with a similar spelling exists" for unresolved names.
- The human format is colored when stderr is a terminal; `NO_COLOR` disables and
  `CLICOLOR_FORCE=1` forces it. On Windows, colors are only used in terminals known to support
  ANSI codes (Windows Terminal, VS Code, MSYS2/mintty, ConEmu).
- A run with errors ends with `error: could not compile ... due to N previous errors`; a run
  with only warnings ends with `warning: ... generated N warnings`.
- Diagnostic codes are registered in `crates/sagitario_diagnostics/src/codes.rs`, with one
  range per phase.
