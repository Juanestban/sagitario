# Roadmap v2: a complete language

Rationale: [ADR-0084](adr/0084-a-complete-language-and-the-engine-apart.md), which replaces the
roadmap of a game engine of [ADR-0063](adr/0063-distribution-phase-and-roadmap-v2.md). The first
roadmap ([roadmap.md](roadmap.md)) builds the language and its tools; this one makes it a complete
general-purpose language: a standard library as complete as Rust's, written in Sagitario, the
core of the runtime in Sagitario too, and C libraries usable with little effort. Its phases are
named `S1`, `S2`... to keep them apart from the language's `F` phases; G1 is the phase this
roadmap started with, when it planned a game engine.

Each phase is planned with its own decision table and records when it starts, as the language's
phases are. What follows are its goals, its acceptance and the questions to settle; nothing here
changes the language until a record does.

## Principles

- **A general-purpose language.** A feature enters the language or its standard library only if
  a general-purpose language would have it, with Rust's std as the reference; what only one kind
  of program needs is a Nova package (Q537).
- **The game engine is a project apart**, written in Sagitario: its plan is in
  [engine-roadmap.md](engine-roadmap.md) until its repository exists (Q538).
- **Sagitario over the C library of each system**, as Rust's std is over libc and the Win32 API:
  hand-written C is replaced, not added to. C stays only as the language of the generated code
  and the system's C library.
- **Every phase passes the harness on every platform of CI**, and the bootstrap keeps its fixed
  point.

## G1: Language prerequisites (closed)

What a game engine needed from the language, much of it among the deferred decisions of the first
roadmap.

**Status:** done and closed. `std::math` is done, in three increments
([ADR-0076](adr/0076-std-math-and-methods-of-numbers.md)): the basic methods of floats and
integers, the constants, and infinities and NaN in constants; powers, roots and logarithms
([ADR-0077](adr/0077-std-math-powers-and-logarithms.md)); and trigonometry
([ADR-0080](adr/0080-std-math-trigonometry.md)). Two records plan the rest of the FFI and references:
[ADR-0078](adr/0078-ffi-without-c-shims.md) (function pointers, `unsafe fn`, arrays, unions and
enums in C layouts, `std::mem`, `&raw`, lists as buffers, `fs::read`, dynamic libraries), so that
a package wraps a C library such as Vulkan in Sagitario alone, without C files; and
[ADR-0079](adr/0079-references-and-borrowing.md) (`&T` and `&mut T` in parameters with a borrow
checker, and slices), which removes copies and reference counting from calls while classes and
closures keep ARC. ADR-0078 is done, in five increments: function pointers and `unsafe fn`; C
layouts (arrays in `@repr(C)` structs, unions, enums with an integer representation, `std::mem`);
`&raw const`, `&raw mut`, lists as buffers and `fs::read`; `std::ffi::Library`; and the acceptance
program `tests/run-pass/reference_ffi/`. ADR-0079 is done, in three increments: `&T` and `&mut T` in
parameters, with the borrow checker; slices and borrowed lists; and `fs::write_bytes`. Operator
overloading through interfaces ([ADR-0081](adr/0081-operator-overloading.md)) is done, in three
increments: `impl ... for` blocks and several implementations of a generic interface; the
arithmetic operators through `Add`, `Sub`, `Mul`, `Div`, `Rem` and `Neg`; and `==` and `<` through
`Eq` and `Ord`. The acceptance program `tests/run-pass/g1_acceptance/` passes. Global state
([ADR-0083](adr/0083-global-state.md)) is done, in three increments: `static`, one `Send`
value for the program computed on its first use; `static var`, a mutable global used in
`unsafe`; and `@thread_local static var`, one value per thread, with the acceptance program

**Acceptance:** a program computes and prints the matrices of a camera with operators, reads a
binary file into a byte buffer, and calls a C function through a pointer loaded from a dynamic
library: `tests/run-pass/g1_acceptance/`, which passes.

## S1: The standard library in Sagitario

The standard library half of the runtime, about 2,000 lines of `runtime/sagitario_runtime.c`, is
rewritten in Sagitario over the C library and the system API through `extern "C"`, without C
shims (Q541).

**Status:** done. [ADR-0085](adr/0085-conditional-compilation-and-std-time.md) fixed the
pattern and the order of the modules, with conditional compilation by `@cfg`. Every module of the
standard library is written on the system API in `std::sys`, without C: `std::time`, `std::env`,
`std::path`, `std::io`, `std::fs`, `std::process` with `Command`, dynamic libraries
(`std::ffi::Library`), `std::sync` and `std::thread`. `std::io` needed C's global variables in the
FFI ([ADR-0086](adr/0086-c-globals-and-std-io.md)), and `std::sync` an atomic type the compiler
provides to `std` ([ADR-0087](adr/0087-atomic-storage-and-std-sync.md)). What each thread does
when it starts and ends stays in the runtime's core, which S3 moves.

- One module per increment: `std::env`, `std::io`, `std::process` with `Command`, `std::fs`,
  `std::path`, `std::time`, `std::thread`, `std::sync` and dynamic libraries.
- The C API of each system (POSIX on Linux, macOS, Android and iOS; Win32 on Windows) declared in
  Sagitario, chosen by target.
- A module's C is deleted from the runtime once its Sagitario version passes the harness on every
  platform of CI.
- What the FFI lacks for a module is added to the language by that module's record.
- Question: the order of the modules, and how a module chooses its system API by target.
  Recommended: a small module first, `std::time` or `std::env`, whose record fixes the pattern.

**Acceptance:** the runtime keeps no standard-library code: `runtime/sagitario_runtime.c` holds only
the core, and every module of `std` is Sagitario code, with the harness and the bootstrap passing on
every platform of CI.

## S2: A complete standard library

The standard library a general-purpose language needs, with Rust's std as the reference (Q542).

**Status:** in progress. The audit is written, [`docs/std-audit.md`](std-audit.md), with the order
of the records that fill its gaps; the first are gaps of the language itself (bitwise operators,
the integer helpers spec 2 promises, `Display`), done with `fmt` and the benchmarks, whose first
results are [`docs/benchmarks.md`](benchmarks.md).

- An audit first, `docs/std-audit.md`, which compares each module of Rust's std with Sagitario
  and records what exists, what is missing, and what is out of scope with the reason. The modules
  are collections, `fmt`, `io`, `net`, `sync`, `thread`, `time`, `fs`, `path`, `env`, `process`,
  `hash`, `cmp`, `iter`, strings and characters, numbers, `mem`, `ptr`, `ffi` and `os`.
- Each gap is filled by a record of its own.
- The crates most Rust programs use (`serde`, `regex`, `rand`, `chrono`) are Nova packages, not
  the standard library.
- A suite of benchmarks, `benchmarks/`, part of the audit: the same programs in Sagitario, Rust,
  C, C#, JavaScript (Node.js) and Python, built or run optimized, to measure how far Sagitario is
  from each and where. Numeric loops over
  structs and arrays (n-body, spectral norm), allocation and reference counting (binary trees),
  strings and collections (word counts, sorting, maps), threads (a parallel sum, channels), and
  the start of a program. It records times and memory per platform, and names the work that
  closes the gaps: retains and releases removed when a value moves or is lent, atomic counts only
  for values that cross threads, `restrict` for `&mut` parameters and slices, and closures that
  never escape kept off the heap. The goal is the speed of Rust in code without allocation, and
  close to it elsewhere.
- The results can be watched: a tool of the suite renders each benchmark as an animated race of
  the languages, an HTML page with no dependency, which can be recorded as a video. Writing the
  video file itself (MP4) would need an encoder such as `ffmpeg`; whether the tool uses one is a
  question for the suite's record, since the project adds no dependency without asking.

**Acceptance:** every row of the audit is done or out of scope with its reason, and the
benchmarks run, locally, with their results recorded ([ADR-0090](adr/0090-benchmarks.md)).

## S3: The core in Sagitario

The core of the runtime, about 1,800 lines, is rewritten in Sagitario (Q543): reference counting,
`String`, `List`, slices, hashing, panics, `async` tasks, statics and the life cycle of the
program.

- A low-level layer in the compiler, as Rust's `core` and its lang items: intrinsics for atomics,
  allocation and thread-local storage, and code without reference counting.
- Its record is written after S1, with what S1 teaches.
- Question: how the bootstrap builds a compiler whose own runtime is Sagitario code, and which
  checks keep a fault in the core from breaking the compiler.

**Acceptance:** the runtime is Sagitario code. What stays C is the generated code and the system's C
library; anything else is listed in S3's record with its reason.

## S4: Binding generation

A Nova tool that writes Sagitario declarations from C headers, as Rust's `bindgen` does (Q544):
`extern "C"` blocks, `@repr(C)` structs, unions and enums, and function pointers.

- Question: its inputs (C headers, through a parser of the part of C that headers use, or
  descriptions such as Vulkan's `vk.xml`), the translation of macros, opaque types and
  callbacks, and its command.

**Acceptance:** bindings generated from the headers of a real C library are used by a program in
the harness without being edited by hand.

## Order

S1 first, since it removes C right away and uses the FFI against real C libraries; the audit of
S2 follows; filling its gaps can interleave with S3 and S4, which come in that order (Q545).
