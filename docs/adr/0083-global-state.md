# 0083. Global state: `static`, `static var` and `@thread_local`

- Status: Accepted
- Date: 2026-11-10
- Supersedes in part: [ADR-0017](0017-resolution-of-open-questions-q1-q11.md) (Q8: no static
  fields or mutable globals in the MVP) and docs/spec/03-memory.md (the same rule)
- Refines: [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) (G1: "global state: mutable
  globals or a sanctioned alternative")

## Context

A module can only hold `const` items, whose values the compiler computes from literals
(docs/spec/03-memory.md#module-level-constants). Anything computed at run time, such as a
`Map`, a `Mutex`, a counter or the function pointers loaded from a dynamic library, is created in
`main` and passed down through every call. An engine has much of this state: the Vulkan instance
and its function table, the window, the allocator, the configuration, logging. Passing all of it
explicitly is noisy, and C libraries with callbacks (`qsort`, GLFW's input callbacks) take no
context to pass it through.

Rust, the reference of the language for safety, separates three things:

- `const`: a value computed at compile time and copied where it is used.
- `static`: one value with a fixed address for the whole program, shared by every thread, so its
  type must be `Sync`. It is never mutated in place except through interior mutability
  (`Mutex`, atomics, `OnceLock`); a value computed at run time is wrapped in `LazyLock`, which
  computes it on first use.
- `static mut`: a mutable global any thread can reach, so every read and write is `unsafe`.
  Since the 2024 edition, references to it are rejected.
- `thread_local!`: one value per thread, created on its first use in that thread, safe to use
  without locks since no other thread sees it.

Rust has no static fields in types: modules play that role (`config::WIDTH`), and types only
have associated constants.

In Sagitario, `Send` is the bound of what threads may share (ADR-0045, Q171–Q172: Sagitario
has no moves, so `Send` covers both Rust's `Send` and `Sync`), and a program that uses threads
counts references atomically.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q526 | `static` | `static NAME: T = expr;` at module level declares one value for the whole program, as Rust's `static`. `pub` exports it, `use` imports it, and `config::NAME` names it, as a `const`. Its type is always written, and must be `Send` (as Rust's `Sync`), since every thread can read it: a primitive, `String`, a struct of `Send` values, a class with only `const` `Send` fields, `Mutex<T>`, an atomic or a `Channel<T>`. A `List`, a `Map`, a class with `var` fields or a function value is an error that names the reason, as `thread::spawn` reports it (`E0332`). Naming it gives its value; it cannot be assigned. Mutation goes through the value's own methods: `COUNTER.fetch_add(1)`, `CACHE.update(...)`. |
| Q527 | Initialization | The initializer is any expression, as Rust's `LazyLock`. It is computed **on the first use** of the static, exactly once even when several threads reach it at once (the others wait). A static whose initializer is a constant expression (the rules of `const`) is computed by the compiler and has no check. A static that needs itself to be initialized, directly or through others, panics naming it. A panic in an initializer ends the program, as every panic does. |
| Q528 | `static var` | `static var NAME: T = expr;` is a mutable global, as Rust's `static mut`: `NAME = value`, `NAME += 1`, and reading it, are allowed only inside `unsafe`, since threads could reach it at once and the compiler cannot tell; its type needs not be `Send`. As in Rust 2024, `&NAME` and `&mut NAME` are errors; `&raw const NAME` and `&raw mut NAME` give a pointer, for C libraries that keep one. Its initialization follows Q527. |
| Q529 | `@thread_local` | `@thread_local static var NAME: T = expr;` gives each thread its own value, as Rust's `thread_local!`: it is created on the first use in each thread (Q527, per thread), and read and assigned without `unsafe`, since no other thread sees it. Its type needs not be `Send`. A thread's value is released when the thread ends, and the main thread's when the program ends. `&NAME` and `&mut NAME` are errors, since the value could change while borrowed. `@thread_local` applies only to `static var`; on `static` it is an error, since an immutable value per thread is a `static` or a `const`. Sagitario has no `Cell`, so a `var` takes the role of Rust's `Cell` and `RefCell` inside `thread_local!`. |
| Q530 | Static fields | None, as in Rust: a type has no `static` members, and `Type::NAME` is not a field. A module groups globals instead (`window::INSTANCE`). Associated constants (`Type::MAX`) stay a deferred decision. |
| Q531 | `const` and `static` | `const` keeps its rules: copied where it is used, computed at compile time, and limited to primitives and `String`. A value that must be unique, computed at run time, or of another type is a `static`. |
| Q532 | Lifetime | The value of a static lives until the program ends and is never released while the program runs, as in Rust, where statics are not dropped. Sagitario has no destructors, so this is only visible to the leak check (`SAG_LEAK_CHECK`), which does not count the values of statics. |
| Q533 | Keyword | `static` becomes a keyword (docs/spec/01-lexical.md). It was not reserved; no code of the compiler, Nova, the standard library or the tests uses it as a name. |
| Q534 | The FFI | `@export` on a static and `extern "C" { static errno: c_int; }` (C's globals) stay deferred; `&raw const NAME` passes a static to C. |
| Q535 | The editor | The grammar highlights `static`; `sag lsp` completes the keyword and `@thread_local`, gives statics their types in completion and hover, and colors them as variables (`readonly` for `static`); snippets `static`, `staticvar` and `threadlocal` (CLAUDE.md). |
| Q536 | Documents | This record, `docs/spec/01-lexical.md`, `docs/spec/03-memory.md` (a section "Global state", replacing the rule of Q8), `docs/spec/10-ffi.md` (`static var` and `&raw`), `docs/spec/11-standard-library.md` (statics with `Mutex` and atomics), `docs/roadmap-v2.md`, `docs/architecture.md` and the extension. |

### Example

```sag
use std::ffi::c_int;
use std::sync::{AtomicI64, Mutex};

/// Shared by every thread: `Send`, and computed on its first use.
pub static FRAMES: AtomicI64 = new AtomicI64(0);
static NAMES: Mutex<String> = new Mutex(load_names());

/// One per thread, used without locks.
@thread_local
static var SCRATCH: List<f32> = new List();

/// Reached from a C callback, which takes no context: `unsafe`.
static var PRESSED: i32 = 0;

extern "C" fn on_key(key: c_int) {
  unsafe { PRESSED = key; }
}

fn frame() {
  FRAMES.fetch_add(1);
  SCRATCH.clear();
  SCRATCH.push(0.5);
}
```

### Increments

1. **`static`.** The keyword, the item in the parser, resolver and type checker (its `Send`
   check), the lazy initialization once per program (a flag, and a lock in programs with
   threads), the constant fast path, cycles, and the leak check; the editor (Q535).
2. **`static var`.** Assignment and compound assignment to a static, the `unsafe` check for reads
   and writes, the rejection of `&` and `&mut`, and `&raw`.
3. **`@thread_local`.** Storage per thread (C11's `_Thread_local`), its lazy initialization, the
   release of each thread's values when it ends, and the acceptance: a program counts frames from
   several threads in a `static` atomic, keeps a scratch list per thread, and receives a C
   callback (`qsort`'s comparison counting its calls) through a `static var`; it passes with
   ThreadSanitizer on Linux and macOS, and with the leak check.

## Alternatives considered

- **Only `const` with run-time initializers:** fewer concepts, but a `const` is copied where it is
  used, which for a `Mutex` would create one per use; Rust separates the two for that reason.
- **`static var` without `unsafe`, as in C, Java or C#:** the most familiar, but threads would
  race on it with no check, against the language's model of `Send`.
- **Module-level `var` being thread-local by default:** safe and short, but it surprises anyone who
  expects one value for the program, and nothing in Rust or C works so.
- **Initialization before `main`, in order of modules:** predictable, but the order across
  modules becomes part of the program's meaning, and a static used before its turn is found only
  at run time; it also costs start-up time for statics never used.
- **Static fields (`static var count` inside a class):** familiar from Java and C#, but Rust has
  none, modules already group and export, and it would add members that are neither fields nor
  methods.
- **An explicit `LazyLock<T>` type:** closer to Rust's library, but every use would need
  `.get()`; since Sagitario has no destructors or `Deref`, making every static lazy is simpler.

## Consequences

- `static` is a keyword; code that names something `static` must rename it (none in the
  repository).
- Each lazy static costs a check of a flag at each use (an atomic load in programs with threads);
  constant ones cost nothing.
- The values of statics are roots for the leak check, so a value reachable only from a static is
  not reported.
- A `static` cannot hold a `List` or a `Map` shared by threads, as `Mutex<T>` cannot (ADR-0045,
  Q181); a `@thread_local static var` or a `static var` in `unsafe` can.

## Notes

- Increment 1 (`static`) is done: `docs/spec/03-memory.md#global-state-g1`, the tests
  `tests/run-pass/statics/`, `tests/run-panic/static_cycle.sag`, `tests/compile-fail/static_errors.sag`
  and `static_syntax.sag`. Each static has a function without parameters computing its value
  (`StaticDecl::init`), which the resolver, the type checker and SIR treat as any function; the C
  of each static is its storage, its state and `sag_static_N(file, line)`, which computes the value
  on the first use and returns it with a reference (`codegen_c.sag`, `Generator::statics`), and the
  runtime waits for a value another thread computes (`sag_static_begin`, `sag_static_end`).
  `static var` is reported as not implemented yet (`E0900`).
