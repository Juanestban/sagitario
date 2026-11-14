# 0106. The core of the runtime in Sagitario (S3)

- Status: Proposed
- Date: 2026-11-14
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q543: S3),
  [ADR-0102](0102-binding-generation-before-the-core.md) (S3 after S4),
  [ADR-0085](0085-conditional-compilation-and-std-time.md) (Q552: what the core calls keeps its C
  name), [ADR-0087](0087-atomic-storage-and-std-sync.md) (`Atomic<T>` only in classes),
  [ADR-0098](0098-locks-orderings-and-threads.md) (orderings),
  [ADR-0105](0105-c-types-at-the-ffi-boundary.md) (calls through C function pointers),
  [ADR-0067](0067-bootstrap-from-the-seed.md) (the seed and its copy of the runtime),
  docs/spec/09-modules.md, 10-ffi.md, 11-standard-library.md, docs/architecture.md

## Context

S3 rewrites the core of the runtime in Sagitario (Q543): what is still C in `runtime/` after S1
moved the standard library to `std::sys`. That is `sagitario_runtime.c`, about 2,260 lines, and
`sagitario_runtime.h`, about 1,230:

- reference counts and the state each thread keeps, class instances, interface and function
  values;
- `String` and its API, `char`, `StringBuilder`, parsing numbers, the debug form of values;
- lists, slices, arrays, hashing and order;
- panics, printing, statics and the life cycle of the program;
- `async` tasks (the executor, timers, tasks across threads) and the threads the core starts;
- the header's 133 `static inline` functions: the checks of integer arithmetic, the methods of
  integers and floats, and the conversions of `as`.

The acceptance of S3: the runtime is Sagitario code, and what stays C is the generated code and
the system's C library; anything else is listed here with its reason.

S1 showed how: `std::sys` reaches the system through `extern "C"` blocks, what the C core calls
keeps its C name through `@export` (Q552), and the C core calls Sagitario that way until it
moves. The difficulty is that the core implements what the rest of the language rests on:
reference counting cannot be written with values that count references.

The user chose:

- Modules `core` without reference counting, as Rust's `core`, under `std` and the prelude.
- Lang items: the core declares the internal structures and functions, and the compiler
  generates their C from them; no header is written by hand.
- The core compiled with each program, in its translation unit, so that C inlines it as it
  inlines the header's functions today; `runtime/` disappears.
- No benchmark more than 5% slower than with the C runtime.
- `Atomic<T>` as a field of the core's structs, and a condition `@cfg(threads)` for the programs
  without threads, whose counts are not atomic.
- The header's functions as Rust has them: the operators' checks and `as` in the compiler, the
  methods of numbers in `core`.

## Decision

### The layer

| # | Topic | Decision |
|---|---|---|
| Q750 | `core` | The compiler embeds the modules of `core`, a path root as `std` is, which every program compiles: `core::object`, `core::string`, `core::list`, `core::panic` and the others the increments add. `std` and the prelude may use `core`; a program's own code may not (`E0200` names it the standard library's), as with `std::sys` (Q551). |
| Q751 | Without reference counts | The code of `core` holds no value that counts references: its types are numbers, `bool`, `char`, raw pointers, C function pointers, and structs, enums, unions, arrays, tuples, `Option` and `Result` of those. `String`, `List`, `Map`, classes, interfaces, closures, `CString` and `StringBuilder` are an error there (`E0347`, new), as are `new` and the methods of those types. The core works on the representations of those values, through raw pointers. |
| Q752 | Lang items | `@lang("name")` marks the structs and functions of `core` the compiler knows: the struct whose C is a type of the generated code (`@lang("sag_string")` is `String`'s representation), and the function the generated code calls under that C name (`@lang("sag_retain")`), as `@export` does (Q552). A lang struct is `@repr(C)`, and the codegen writes its C from it; a lang function is `extern "C"`, defined in the program's translation unit, where C may inline it. A lang item the compiler needs and does not find, or one declared twice, is an internal compiler error, since `core` is the compiler's own. |
| Q753 | Atomics | In `core`, `Atomic<T>` may be the type of a field of a struct (`struct ObjectHeader { count: Atomic<usize>; ... }`), with the operations and orderings of ADR-0098. Elsewhere it stays a `const` field of a class (Q565). |
| Q754 | `@cfg(threads)` | A condition of `@cfg`, for `core` and `std`, that holds when the program uses threads: what today defines `SAG_THREADS`. The core has its counts atomic under it and plain without it. |
| Q755 | Calls through pointers | A call of `core` through a C function pointer that the generated code gives it (the destructor of a class, the functions of a vtable or of a task) is a call of the program's translation unit, not one of ADR-0105's helpers: both ends are the program's own C, with the same types. |
| Q756 | Panics in the core | A panic writes its message to standard error through the C library without allocating, and a panic while one is being written ends the process (`abort`), so that a fault of the core in a panic does not recurse. Integer arithmetic in `core` is written with the methods that say what happens on overflow (`wrapping_add`, `checked_mul`), and the operators that may overflow are an error there (`E0348`, new). |

### What moves, and how

| # | Topic | Decision |
|---|---|---|
| Q757 | Numbers | As Rust does: the checks of the integer operators and the conversions of `as` are generated by the codegen, as C of the program, like `rustc` generates them. The methods of integers and floats (`checked_add`, `wrapping_mul`, `leading_zeros`, `pow`, `sqrt`, `sin`...) are functions of `core::num`, as Rust's `core::num`, written in Sagitario over intrinsics the codegen writes where C has the operation (the arithmetic of a width, a shift, a comparison) and over the C library's `<math.h>` for the floats, as Rust's `std` uses `libm`. |
| Q758 | The system | The core reaches the system as `std::sys` does: allocation (`malloc`, `realloc`, `free`), output, threads, time and the rest through `extern "C"` blocks of `core::sys`. Thread-local state is a `@thread_local static var` (ADR-0083). |
| Q759 | During the moves | Each increment moves a part and deletes its C. What still C calls of what moved keeps its C name through `@lang` (or `@export`), and what moved calls of what is still C through `extern "C"` declarations of the runtime, as S1 did. |
| Q760 | The bootstrap | The seed is the C of the compiler, its core included in `sag.c`; `bootstrap/` loses its copy of the runtime when `runtime/` is gone, and the driver no longer embeds C. While C remains, the seed keeps it as today. The seed is updated (`--update-seed`) only when the compiler needs what the seed cannot compile. |
| Q761 | Guards | What keeps a fault in the core from breaking the compiler: every increment passes the harness with ASan and UBSan, and with TSan on Linux and macOS; the bootstrap's fixed point (stages 1 and 2 generate the same C) runs with the moved core; and the benchmarks of ADR-0090 are measured before and after each increment that touches the paths they exercise (reference counts, strings, lists, tasks). |
| Q762 | Performance | No benchmark of ADR-0090 more than 5% slower than with the C runtime, measured on the same machine; the results are recorded in `docs/benchmarks.md` with the increment. An increment that does not meet it is not done. |

### Increments and acceptance

| # | Topic | Decision |
|---|---|---|
| Q763 | Increments | 1: the layer (`core`, Q750-Q756), with panics, printing and the life cycle of the program. 2: reference counts, class instances, interface and function values. 3: `String`, `char`, `StringBuilder`, parsing and the debug form. 4: lists, slices, arrays, hashing and order. 5: the numbers (Q757), and the header's `static inline` functions gone. 6: statics and the state of each thread. 7: tasks and threads. 8: `runtime/` gone: the driver embeds no C, the seed has no copy of the runtime, and the acceptance. Each with its spec, tests, editor support and benchmarks. |
| Q764 | Acceptance | `runtime/` does not exist. What a program's C is: the generated code, the C library's headers it includes, and `stdatomic.h`. The exceptions, if any are found, are listed in this record with their reasons. |

## Alternatives considered

- **`@no_arc` functions in `std`, without a root:** the same check, but nothing would separate
  the code the language rests on from the standard library above it.
- **A dialect with manual reference counting:** the core could use classes and `String` with
  explicit `retain` and `release`, which is more expressive and much easier to get wrong.
- **A small C header with the layouts:** less to generate, but C written by hand would stay, as
  an exception.
- **The core's C generated once and kept in `runtime/`:** closer to today, but in a unit of its
  own, which C does not inline into the program.
- **Atomics as intrinsics on raw pointers (`atomic_fetch_add(ptr, 1, ordering)`):** more general,
  but an API of its own beside `Atomic<T>`.
- **Always atomic counts:** one version, slower for programs without threads.
- **The checks of the operators in `core`:** as functions, the core would depend on its own
  checked arithmetic, and the compiler already decides what each operator does.

## Consequences

- The runtime of a program is Sagitario: what it does can be read, tested and changed in the
  language, and the C a program compiles is what the compiler generates.
- `core` is a third layer under `std` and the prelude, with rules of its own (Q751, Q756) that
  the checker enforces.
- The compiler grows the lang items and the codegen of what the header held; the header and
  `runtime/` go away.
- Building a program compiles more Sagitario, the core, which the benchmarks of the compiler
  itself (its time to build the harness) show.
