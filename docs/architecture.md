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

A program is a *package* of modules, one per file (docs/spec/09-modules.md). The driver lexes
and parses the entry file, asks `sagitario_resolve::module_references` which module paths it
mentions, loads those that exist as files (`pkg::net::http` is `net/http.sag` next to the
entry file) and repeats for each new module. Resolution builds the module tree, resolves `use`
declarations and checks visibility; from then on the package is analyzed as a whole, and
functions and types are numbered across all modules.

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

SIR is structured (`If`/`Loop`/`ForRange`/`ForArray` rather than a control-flow graph), since
it is emitted as C. Fields and array elements are *places*: a local plus field and
bounds-checked index projections. Enum values (including `Option` and `Result`) have a tag, the
index of their variant, and the fields of that variant, read only after testing the tag. A
`match` tests each arm's pattern without binding anything, then binds copies of the matched
parts, so a failed arm or guard leaves nothing unreleased.

ARC is explicit. A type is *managed* if it contains a `String` or another reference (a class
instance, a `StringBuilder`, a `CString`), directly or inside structs, enums and arrays. Every
managed binding owns one reference to each one inside it and is released at
the end of its block, before `break`/`continue`/`return`, or when overwritten (the new value is
retained first); temporaries are released at the end of the statement or branch that created
them; parameters are borrowed; return values are owned by the caller. Building a struct or
array retains its parts. Reading a managed field or element borrows it; reading an unmanaged
one copies it at that point. Arguments of a call to a method that modifies `self` (inferred by
the type checker, ADR-0028) that borrow a managed value are copied first, since the method may
modify the receiver they borrow from. Fields of class instances are places behind a
`ClassField` projection; since any call may change them through another reference, reading a
managed value from one takes an owned copy, and an instance reached through another place is
retained for the statement. `new C(args)` allocates the instance (`Alloc`) and calls the
initializer on it.

SIR is the place for ARC optimizations and the boundary behind which other backends (LLVM)
could be added.

## C code generation

- Output is strict, portable **C11**: no GCC/clang extensions (no statement expressions, no
  bare `__attribute__`; any compiler-specific feature hides behind a runtime macro with a
  portable fallback).
- **No undefined behavior**: signed arithmetic goes through runtime helpers that detect
  overflow (panic in debug, wrap in release); shifts, division and indexing are checked.
- `#line` directives map C back to `.sag` sources (the file and line of each statement); one is
  repeated right before each runtime check that reports `__LINE__`, since a statement can span
  several C lines.
- `sag build --emit-c` writes the generated C for inspection.
- Each struct becomes a C struct `sag_sN` (fields `fN`), each enum a `sag_eN`, and each array,
  `Option` and `Result` type a `sag_tN`. Arrays wrap a C array; enum-like types hold a `tag` and
  a union `u` with a struct `vN` per variant with fields. All are copied by assignment and
  passed by value, and emitted in dependency order. Managed ones get
  `sag_retain_*`/`sag_release_*` functions. A method that modifies a struct's or enum's `self`
  receives a pointer to the caller's value.
- Each class becomes a C struct `sag_cN` whose first member is its parent's struct (`base`) or,
  without parent, the runtime's `sag_object` header (strong and weak counts, drop function,
  vtable), followed by its own fields. Every class value is a `sag_object *`, so an instance
  passes for any ancestor without conversions; a field is read through the struct of the class
  that declares it. `sag_new_cN` allocates a zeroed instance with one reference, and
  `sag_drop_cN` releases its own fields and then its parent's when `sag_object_release` drops the
  last reference. Destruction is iterative: objects released while another is being destroyed
  wait on an explicit stack.
- Methods that are abstract or overridden somewhere (decided by the type checker) get a slot in
  the vtables `sag_vtable_cN`; calls to them convert the entry back to the method's own function
  type. Other methods and `zod` calls are direct. Every initializer sets the instance's vtable to
  its class's after `zod.new(...)` (SIR `SetVtable`), and an abstract method's body panics.
- A `weak` field is a bare `sag_object *` (NULL for `None`) holding a weak reference. SIR reads it
  with `WeakLoad` (`sag_object_upgrade`: a strong reference while the instance is alive) and
  writes it with `WeakStore` (`sag_object_weak_set`). An object's memory is freed when both its
  strong and weak counts reach zero.
- Indexing goes through `sag_index_at`, which panics when the index is out of bounds.
- When `main` returns `Result<(), String>`, the C `main` prints an `Err` and exits with code 1.
- Every aggregate is declared (`typedef struct sag_sN sag_sN;`) before any is defined, so raw
  pointers can refer to any type, including the struct that contains them.
- FFI (docs/spec/10-ffi.md): calls to C functions and exported `extern "C" fn`s go through a
  second translation unit, `sag_ffi.c`, which includes no system header (only `stdint.h`,
  `stddef.h`, `stdbool.h`). It declares each C function with the program's signature and
  defines a wrapper `sag_ffi_N` for it, plus a function with each exported symbol that calls
  the Sagitario function; exported functions are the only non-`static` ones. A dereference
  goes through `SAG_CHECK_PTR`, which panics on a null pointer.
- Planned (F2): `Option<ClassRef>` as a nullable pointer.

## Runtime

Minimal C, embedded in the `sag` binary and compiled with every program:

- ARC object header and retain/release (non-atomic in the MVP),
- panic (message, file, line, exit code 101), including the array bounds check
  `sag_index_at`,
- `String` storage and UTF-8 helpers, allocation,
- basic I/O used by `std`,
- overflow-checked integer helpers (`sag_add_i32_at`, ...) that panic in debug and wrap when
  compiled with `-DSAG_RELEASE` (`sag build --release`), saturating float-to-integer casts,
- a string builder used by template strings, with shortest round-trip float formatting,
- the `String` API (`sag_string_*`; `trim` and `substring` return views sharing the buffer),
  parsing (`sag_parse_*`), ASCII `char` helpers and the ARC `StringBuilder` (`sag_builder_*`),
- FFI support: the ARC `CString` (`sag_cstring_*`), `sag_string_from_c_at` (lossy UTF-8),
  `sag_c_char` (C's `char`) and the raw pointer macros (`SAG_PTR_ADD`, `SAG_CHECK_PTR`, ...),
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

- The file given to `sag` is the program's root module; the modules it uses are loaded from
  its directory.
- `sag build main.sag` writes the executable next to the source (`main` / `main.exe`);
  `--emit-c` also writes `main.c` there (it includes `sagitario_runtime.h` from `runtime/`),
  and `main_ffi.c` when the program uses the FFI.
  `sag run` builds into a temporary directory that is removed afterwards.
- The driver stops after lexing or parsing if they report errors (an incomplete syntax tree
  would only cause confusing follow-up errors); otherwise resolution, type checking and flow
  analysis run together, so one run reports all their errors and warnings.
- Diagnostics are printed in source order. They carry secondary labels ("previous definition
  here", "expected because of this type"), notes and `help:` suggestions, including "a name
  with a similar spelling exists" for unresolved names. A label in another file (such as a
  private item's declaration) gets its own excerpt, introduced by `:::`.
- The human format is colored when stderr is a terminal; `NO_COLOR` disables and
  `CLICOLOR_FORCE=1` forces it. On Windows, colors are only used in terminals known to support
  ANSI codes (Windows Terminal, VS Code, MSYS2/mintty, ConEmu).
- A run with errors ends with `error: could not compile ... due to N previous errors`; a run
  with only warnings ends with `warning: ... generated N warnings`.
- Diagnostic codes are registered in `crates/sagitario_diagnostics/src/codes.rs`, with one
  range per phase.
