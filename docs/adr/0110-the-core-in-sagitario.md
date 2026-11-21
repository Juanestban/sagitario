# 0110. The core in Sagitario

- Status: Accepted
- Date: 2026-11-21
- Supersedes: [ADR-0045](0045-threads-and-send.md) Q175 (atomic counts for the whole program)
- Refines: [ADR-0067](0067-bootstrap-from-the-seed.md) and [ADR-0107](0107-a-seed-per-host.md)
  (what the seed holds), [ADR-0085](0085-conditional-compilation-and-std-time.md) (how a module
  of the standard library reaches C)
- Roadmap: S3 of [`docs/roadmap-v2.md`](../roadmap-v2.md) (Q543)

## Context

S1 left the runtime's core in C: `runtime/sagitario_runtime.c`, about 2,260 lines, and
`runtime/sagitario_runtime.h`, about 1,230. The driver embeds both (`include_str!`) and compiles
the `.c` with every program, with `SAG_THREADS` defined when the program uses threads. The `.c`
holds:

| Part | Lines | What it does |
|---|---|---|
| Reference counts, thread-local state | ~55 | `sag_count_*`, atomic only with `SAG_THREADS` |
| Panics | ~45 | `sag_panic*`, one thread ends the program |
| Strings, their API, parsing, `StringBuilder` | ~750 | ARC strings, UTF-8, `parse_*` |
| The debug form of values, printing | ~185 | `{:?}`, `print`, `println` |
| Class instances, lists, slices | ~340 | allocation, drop, weak references |
| Hashing and order | ~40 | |
| FFI | ~125 | `CString`, C strings |
| `std::process`, `std::thread`, locks of the core | ~170 | what each thread does at its start and end |
| Tasks, tasks across threads | ~400 | the executor of `async`, `spawn_blocking` |
| Statics | ~155 | lazy statics, `@thread_local`, their release |
| Program lifecycle | ~40 | `sag_rt_init`, `sag_rt_finish`, the leak check |

The header holds the C types the generated code uses (`sag_string`, `sag_object`, `sag_fnval`...)
and 133 `static inline` functions it calls in every expression: checked integer arithmetic,
conversions, indexing, the methods of numbers.

S1 showed the pattern: a module of `std` declares the C library in Sagitario (`std::sys`) and its
own functions over it, with `@cfg` per target. The core differs in three ways: the generated code
calls it by C symbols, it implements the reference counting that Sagitario code relies on, and part
of it depends on whether the program uses threads, which the compiler knows only after generating
the program.

The user chose (2026-11-21): the C generated from the core goes into the seed; a change of the
core is accepted only when the harness passes with sanitizers and the compiler reproduces itself;
the design below; and atomic counts as Rust's `Rc` and `Arc`, chosen by the type the programmer
declares rather than for the whole program.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q780 | Place | The core is the module `std::core` (`compiler/std/core.sag`), which every program compiles, as `std::env` and `std::time`. As `std::sys` (ADR-0085, Q551), only the modules of `std` can name it; for a program it does not exist (`E0200`). |
| Q781 | Symbols | The core's functions are `extern "C" fn` with `@export("sag_...")`, the symbols the generated code calls today: code generation does not change. `runtime/sagitario_runtime.h` keeps the C types and declarations the generated code includes. |
| Q782 | Without reference counting | Inside `std::core`, ARC types (`String`, `List`, classes, closures, interface values) cannot be used: the core handles them as their C layouts, `@repr(C)` structs reached through raw pointers, so the compiler never retains or releases in the code that implements retaining and releasing. Everything else in it is ordinary unsafe Sagitario: integers, raw pointers, `@repr(C)` structs, `extern "C"` declarations of the C library, `-> never`. Programs cannot write in `std::core`, so this is no diagnostic: the harness checks that the core names no ARC type. |
| Q783 | Intrinsics | None new. Allocation is `malloc`/`realloc`/`free` of `std::sys`; atomics are the `Atomic` the compiler gives `std` ([ADR-0087](0087-atomic-storage-and-std-sync.md)); thread-local state is `@thread_local static var` ([ADR-0083](0083-global-state.md)); locks are those `std::sys` declares for `std::sync`. |
| Q784 | Atomic counts | As Rust's `Rc` and `Arc`, by type: in a program using threads, an object's count is atomic when its type is `Send` (strings, `CString`, classes whose fields are all `const` and `Send`, the classes of `std::sync`), and plain otherwise, since only `Send` values cross threads. The kind is chosen when the object is created, by its own class, and kept in the object: one seen through an interface value or a base class keeps it. Three kinds of objects reach another thread without being `Send` (ADR-0045): the closure given to `thread::spawn` (created atomic at that call, which `E0333` keeps a closure literal), and the objects created by `std::thread`, `std::sync` and `std::task` (atomic, such as `JoinHandle` and the state inside a channel). Programs without threads keep every count plain. The language does not change: `Send` already says which values cross threads. |
| Q789 | Layouts | The core's `@repr(C)` structs have the layouts of the header's types (`sag_string`, `sag_object`...) and cross between the program's unit and the FFI unit as every aggregate does (ADR-0105, Q746): by layout, through the wrappers the FFI unit gives each exported function. |
| Q785 | What stays C | The header's types and its `static inline` functions, which are part of the generated code: C11 has no checked arithmetic, and calling a Sagitario function for every `+` would cost what inlining saves. Also the C library, and `main`, which the generated code writes. |
| Q786 | Bootstrap | The core is part of `std`, and its exported functions are always generated, as `sag_env_init` is today: the C of the core is in every program's C, the compiler's own `sag.c` of the seed included. The bootstrap stops compiling `bootstrap/sagitario_runtime.c` once every part has moved, and the driver stops embedding `runtime/sagitario_runtime.c`, which is deleted. |
| Q787 | Checks | A change of the core is accepted when the harness passes with ASan, UBSan and TSan, and the compiler reproduces itself: the compiler the seed built (stage 1) and the one it built (stage 2) generate the same C for the compiler, which the bootstrap already checks and fails otherwise. |
| Q788 | Order | One part per increment, each removing its C from the runtime once the harness passes on every platform of CI: hashing and order, panics, the debug form and printing, `CString`, strings and parsing, `StringBuilder`, slices, lists, class instances and reference counts, statics, threads, tasks, the lifecycle. |

## Alternatives considered

- **Lang items as Rust's (`#[lang = "..."]`)**: the generated code already names the core by C
  symbols, which `@export` gives; an attribute would add syntax for no gain.
- **New intrinsics for atomics and allocation**: `std` already reaches them (S1).
- **Moving the header's inline functions too**: they are code generation, not a library; moving
  them needs inlining across units, which the C compiler does only with link-time optimization.
- **Two stages at every bootstrap instead of the core in the seed**: rejected by the user.
- **The core generated twice, with and without threads, as `SAG_THREADS` does now**, or **every
  count atomic**: the first keeps a whole-program switch, the second costs programs without
  threads; the user chose counts by type, as Rust's.
- **A new marker for shared classes (`Arc`-like)**: `Send` already marks them, by inference
  (ADR-0008), so declaring it again would break programs that share classes today.

## Consequences

- `runtime/` ends holding only the header. Programs compile the core's C instead of the runtime's;
  the time of a build stays about the same.
- A fault in the core breaks the compiler that built it; the fixed point (Q787) and the sanitizers
  catch it before a seed is written.
- An increment updates the seed only when the seed cannot build the compiler, as always.
- Programs with threads stop paying atomic counts for objects that never leave their thread, such
  as lists and classes with `var` fields. TSan in the harness checks that no object crosses a
  thread with a plain count.
