# 0045. Threads and `Send`

- Status: Accepted
- Date: 2026-10-24
- Supersedes in part: [ADR-0008](0008-concurrency-deferred.md) (a single `Send`, without
  `Sync`; atomic counts for the whole program instead of per type)

## Context

F4 brings concurrency. ADR-0008 planned Rust's model: `Send` and `Sync` inferred by the
compiler, `spawn`, `Mutex`, `Atomic`, and atomic reference counts only for shared types. That
model relies on moves and borrows. Sagitario has neither. Structs are copied, class instances
are shared ARC references, and a `var` field changes through any reference. A value given to a
thread is therefore shared with the thread that gave it: transferring and sharing are the same.

F4 is planned in four increments: threads and `Send`; `Mutex` and `Atomic`; channels and thread
utilities; and a decision record for `async`/`await` with the acceptance program. Increment 1
settles Q171–Q185, the decisions for the whole phase, and implements the first increment.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q171 | `Send` and `Sync` | Only `Send`, the prelude's interface. `Sync` stays reserved, in case moves ever exist. |
| Q172 | What is `Send` | Inferred, never implemented, extended or used as a type; it is a bound (`T: Send`). These are `Send`: primitives, `String`, `CString`; arrays, `Option`, `Result`, structs and enums of `Send` values; type parameters bounded by `Send`; classes whose fields, own and inherited, are all `const` and `Send`, and whose subclasses are `Send` too (the compiler sees the whole program); and later `Mutex`, `Atomic` and `Channel`. These are not: `List`, `Map`, classes with `var` or `weak` fields, `StringBuilder`, function values, raw pointers and interface values. |
| Q173 | `spawn` | `thread::spawn(body)` returns a `JoinHandle<T>`, and `join()` returns `T`. `body` is a lambda whose captures are all `Send`, or a function named by its path. `spawn` cannot be a value. The result needs not be `Send`: what the thread creates is reachable only from it, and passes whole to the joining thread. An unjoined thread keeps running. |
| Q174 | Panics | A panic in any thread ends the process with exit code 101; nothing unwinds. `join` returns `T`, not a `Result`. |
| Q175 | Atomic counts | For the whole program: one that uses `std::thread` is compiled with `SAG_THREADS`, which makes every reference count atomic. Other programs keep plain counts and pay nothing. Atomic counts per object, by type, can come later as an optimization without changing the language. |
| Q176 | Runtime | pthreads on POSIX (Linux, macOS, Android, iOS), `CreateThread` on Windows, `<stdatomic.h>` everywhere; `cl` needs `/experimental:c11atomics`, which the driver passes. The stack of objects being destroyed is per thread (`_Thread_local`). The first thread to panic or exit ends the program; any other one waits. |
| Q177 | Ending | When `main` returns, the program ends without waiting for running threads. The leak check runs only when no thread is running. |
| Q178 | `Mutex<T>` (increment 2) | `new Mutex(value)` with `T: Send`. Without destructors there are no lock guards: `get()`, `set(v)` and `update(f: fn(T) -> T)`, which applies `f` while holding the lock. Locking again from the same thread panics instead of deadlocking. |
| Q179 | `Atomic<T>` (increment 2) | For integers and `bool`: `load`, `store`, `swap`, `fetch_add`, `fetch_sub`, `compare_exchange`, all sequentially consistent. |
| Q180 | Channels (increment 3) | `new Channel<T>()` with `T: Send`, unbounded, one object: `send(v)`, `receive() -> Option<T>` (`None` once closed and empty), `close()`. With `thread::sleep` and `thread::available_parallelism`. |
| Q181 | Shared mutable collections | Not in F4. Without moves or borrows, a `List` placed in a `Mutex` could still be reachable from outside it. Programs use `Mutex<T: Send>`, channels, or collections returned by `join`. |
| Q182 | ThreadSanitizer | `--sanitize=thread` (`sag` and `nova`) builds with TSan where the C compiler has it (Linux and macOS), and otherwise with ASan and UBSan after a warning. Test cases marked `// sanitize: thread` run with it. |
| Q183 | `async`/`await` | Not implemented in F4. Increment 4 writes their decision record (executor, I/O, relation to threads); the keywords stay reserved. |
| Q184 | F4 acceptance | A parallel word count: threads (as many as `available_parallelism`) count files and return `Map`s through `join`, an `Atomic` tracks progress, and a producer feeds consumers through a `Channel`. It passes with TSan on Linux and macOS, and with the leak check. |
| Q185 | `env::set_variable` | Still deferred: changing the environment while threads run is not safe on POSIX. |

## Alternatives considered

- **`Send` and `Sync` as in Rust:** without moves, a value sent is also shared, so the two
  would always agree.
- **Atomic counts per object, by type (the first proposal for Q175):** the `JoinHandle`, the
  closure starting a thread and objects inside interface values reach two threads without being
  `Send`. Marking them needs special cases or walking object graphs at `spawn`, which is complex
  and easy to get wrong.
- **Explicit `impl Send` on classes:** better error locality, but the inference is cheap since
  the compiler sees every class and subclass, and ADR-0008 chose inference.
- **`join` returning `Result` for panics:** that needs unwinding, which Sagitario does not do
  anywhere.
- **C11 `<threads.h>`:** macOS does not provide it.

## Consequences

- `Send` is a prelude interface, which `TypedModule::implements` answers by inference. The type
  checker reports a capture that is not `Send` with the reason (`E0332`). It also reports a
  `spawn` given another function value or used as a value (`E0333`).
- `spawn` is no longer a reserved name. `Send` and `Sync` stay reserved for declarations, and
  the prelude declares `Send`.
- The standard library's runtime functions may take `fn()` values, which is how
  `std::thread` hands its closure to the runtime.
- Object headers keep one weak reference for all strong ones, so that freeing an object never
  races with the release of its last weak reference.
- The generated C and the runtime start with `#define SAG_THREADS 1` in programs using threads.
  Programs and libraries using threads link with `-pthread` outside Windows.
- TSan was not run locally (Windows has none); CI's Linux and macOS jobs run it.
