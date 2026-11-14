# 0084. Roadmap v2: a complete language, with the game engine as a project apart

- Status: Accepted (Q545 superseded by [ADR-0102](0102-binding-generation-before-the-core.md))
- Date: 2026-11-11
- Supersedes: Q342 of [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) (the roadmap v2 of a
  game engine)

## Context

ADR-0063 planned the roadmap v2 around a game engine, in phases `G1`–`G9`. G1 took up what the
engine needed from the language, and most of it is done: `std::math` (ADR-0076, 0077, 0080), a
complete FFI without C shims (ADR-0078), references and slices (ADR-0079), operator overloading
(ADR-0081), format strings (ADR-0082) and global state (ADR-0083). Its acceptance program,
`tests/run-pass/g1_acceptance/`, passes.

What G1 has left shows the problem with that plan. Memory for frames (arenas or pools of the many
short-lived values of a frame) is something an engine needs; a general-purpose language does not
need it in its standard library. Rust has none there either: `slotmap`, `bumpalo` and the
entity-component systems are crates. Growing the language with what only an engine uses makes it
heavier for every program.

The goal of Sagitario is a complete language:

- the standard library a language needs to work well, with Rust's std as the reference;
- C libraries usable through the FFI, which Vulkan was an example of;
- self-reliance: today about 4,800 lines of hand-written C (`runtime/sagitario_runtime.c` and
  its header) sit under every program, and Sagitario code should replace as much of that as it
  can.

Rust's std is written in Rust over the C library of each system (libc, or the Win32 API on
Windows). The runtime of Sagitario has two halves:

- **The standard library half** (about 2,000 lines of `sagitario_runtime.c`): `std::env`,
  `std::io`, `std::process` with `Command`, `std::fs`, `std::path`, `std::time`, `std::thread`,
  `std::sync` and dynamic libraries. It is calls to the C library and the system, which the FFI
  of ADR-0078 already reaches.
- **The core** (about 1,800 lines): reference counting, `String`, `List`, slices, hashing,
  panics, `async` tasks, statics and the life cycle of the program. The generated code calls it
  directly. Writing it in Sagitario needs compiler support, as Rust's `core` and its lang items
  do.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q537 | The goal | This repository builds a general-purpose language: the compiler, the standard library, Nova and the tools. A feature enters the language or its standard library only if a general-purpose language would have it, with Rust's std as the reference; what only one kind of program needs is a Nova package. |
| Q538 | The engine | The game engine is a project apart: its own repository of Nova packages written in Sagitario. The plan of its phases `G2`–`G9` leaves the roadmap of the language. Until that repository exists, the plan is kept, unchanged, in `docs/engine-roadmap.md`, which changes nothing in the language. What only the engine needs is built there: memory for frames (pools, handles, arenas), an entity-component system, a render interface. When the engine finds something missing in the language, the language takes it up only if Q537 allows it. |
| Q539 | G1 | G1 is closed with what is done, and its acceptance passes. Memory for frames goes to the engine (Q538). Binding generation becomes phase S4. |
| Q540 | Roadmap v2 | `docs/roadmap-v2.md` plans a complete language in four phases, `S1`–`S4`, named apart from the language's `F` phases and from the old `G` phases. As before, each phase is planned with its own decision table and records when it starts, and the roadmap changes nothing in the language until a record does. |
| Q541 | S1: the standard library in Sagitario | The standard library half of the runtime is rewritten in Sagitario over the C library and the system API (POSIX, Win32) through `extern "C"`, without C shims, one module per increment. A module's C is deleted from the runtime once its Sagitario version passes the harness on every platform of CI. If the FFI lacks something a module needs, the record of that module adds it to the language. The first increment fixes the pattern (declaring the C API of each system, choosing by target, testing) with a small module, such as `std::time` or `std::env`. Its record fixes the order of the others. The seed is updated only when the compiler needs it (CLAUDE.md). |
| Q542 | S2: a complete standard library | S2 starts with an audit, `docs/std-audit.md`, which compares each module of Rust's std with Sagitario. It notes what exists, what is missing and what is out of scope, with the reason: collections, `fmt`, `io`, `net`, `sync`, `thread`, `time`, `fs`, `path`, `env`, `process`, `hash`, `cmp`, `iter`, strings and characters, numbers, `mem`, `ptr`, `ffi` and `os`. Each gap is filled by a record of its own. The crates most Rust programs use (`serde`, `regex`, `rand`, `chrono`) are Nova packages, not the standard library. |
| Q543 | S3: the core in Sagitario | The core of the runtime is rewritten in Sagitario, on a low-level layer the compiler provides: intrinsics for atomics, allocation and thread-local storage, and code without reference counting, as Rust's `core` and its lang items. Its record is written after S1, with S1's experience. What stays C is what C itself is: the language of the generated code and the system's C library. Anything else that remains is listed in that record with its reason. |
| Q544 | S4: binding generation | A Nova tool writes Sagitario declarations (`extern "C"` blocks, `@repr(C)` structs, unions and enums, function pointers) from C headers, as Rust's `bindgen` does. Its inputs, the translation of macros, opaque types and callbacks, and its command are settled by its record. |
| Q545 | Order | S1 first, since it removes C right away and uses the FFI against real C libraries. The audit of S2 follows. Filling its gaps can interleave with S3 and S4, which come in that order. |
| Q546 | Documents | This record; `docs/roadmap-v2.md`, rewritten for S1–S4 with G1 as its closed first phase; `docs/engine-roadmap.md` with the plan of G2–G9; the roadmap of `README.md` and its list of what is missing, without the items of the engine; `docs/roadmap.md`, where it points to the roadmap v2; and the index of records. |

## Alternatives considered

- **Keep the engine in the roadmap of the language:** one plan for everything, but the engine's
  needs would keep entering the language (memory for frames first), against Q537.
- **Delete the plan of G2–G9:** the engine will be built, and its plan, its questions and
  their recommendations are work worth keeping until its repository holds them.
- **Rewrite the whole runtime at once:** the core needs compiler support that the standard library
  half does not, so doing S1 first removes C sooner and teaches the pattern before the harder part.
- **The core first:** it is the part every program uses, but it is the riskiest: a fault in it
  breaks the compiler itself through the bootstrap.

## Consequences

- G1's open item of memory for frames is no longer a deferred decision of the language: it is
  work of the engine.
- `runtime/sagitario_runtime.c` shrinks with each increment of S1, and `bootstrap/` follows it
  when the seed is updated (two runtimes: docs/architecture.md).
- The standard library becomes Sagitario code that users can read and step through, as Rust's is.
- The roadmap of `README.md` lists the phases S1–S4 instead of G2–G9.
