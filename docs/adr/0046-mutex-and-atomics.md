# 0046. `std::sync`: `Mutex` and atomics

- Status: Accepted
- Date: 2026-10-24
- Refines: [ADR-0045](0045-threads-and-send.md) (Q179: atomics per type instead of a generic
  `Atomic<T>`)

## Context

F4 increment 2 adds the shared state that ADR-0045 planned: `Mutex<T>` (Q178) and atomic
integers (Q179). A lock and an atomic object are resources of the runtime, which must be freed
with the value that holds them, and Sagitario has no destructors. Increment 2 settles
Q186–Q193.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q186 | Module | `std::sync` holds `Mutex` and the atomics; the channels of increment 3 will join them. |
| Q187 | Atomics per type | `AtomicBool`, `AtomicI32`, `AtomicI64`, `AtomicU32`, `AtomicU64` and `AtomicUsize`, as in Rust. A generic `Atomic<T>` restricted to integers and `bool` would need a bound the language cannot express and conversions to bits built into the compiler; the classes per type are written in Sagitario. |
| Q188 | Atomic API | `new AtomicI64(0)`, `load()`, `store(v)`, `swap(v)` (the previous value) and `compare_exchange(expected, desired) -> Result<T, T>` (`Ok(expected)` if it stored, `Err(actual)` if not). Integers also have `fetch_add(n)` and `fetch_sub(n)`, which return the previous value and wrap around at the type's limits, also in debug builds. Everything is sequentially consistent. |
| Q189 | `Mutex` API | `class Mutex<T: Send>`: `new Mutex(value)`, `get() -> T`, `set(v)`, `update(f: fn(T) -> T) -> T`, which applies `f` while holding the lock and returns the new value. A thread locking a mutex it holds (say, `get()` inside `update` of the same mutex) panics instead of deadlocking. |
| Q190 | `Send` | `Mutex<T>` is `Send` when `T` is, and the atomics always are. The compiler knows the classes of `std::sync` by name: they guard their own state, so their `var` fields do not count. |
| Q191 | Runtime | Locks are `pthread_mutex_t` on POSIX and `SRWLOCK` on Windows; atomic cells are `_Atomic uint64_t`. Both are runtime objects, held in a private `fn()` field whose environment the runtime creates, so ARC frees them with the instance. Cells keep integers in a canonical form (sign- or zero-extended from their width) so that `compare_exchange` compares the whole word. |
| Q192 | Threads mode | A program using `std::sync` is compiled with `SAG_THREADS`, like one using `std::thread`. |
| Q193 | Tests and documentation | A `run-pass` case with TSan (eight threads adding to atomics and mutexes, `Mutex<String>`, a `Mutex` of an immutable class, `swap`, `compare_exchange`, wrapping), a `run-panic` case for the relocking thread, a `compile-fail` case for `Mutex<List<i32>>` and `Mutex<Counter>`; this ADR, the specification and the roadmap. |

## Alternatives considered

- **A built-in `Atomic<T>`:** a new type in the compiler with its own methods, or a special
  bound plus intrinsics; more compiler for the same programs.
- **Lock guards (`lock()` returning a guard):** they need destructors to unlock.
- **A new opaque type for runtime resources:** the `fn()` environment already is an ARC
  reference that the code generator retains and releases.
- **Spin locks on atomics:** portable, but they waste processor time under contention, and the
  system's locks are what ThreadSanitizer knows best.

## Consequences

- The standard library's runtime functions may also return `fn()` values (new references).
- `TypedModule::sync_classes` lists the classes of `std::sync`, which `Send` checks through
  their type arguments only.
- A mutex is not reentrant; the runtime identifies the thread holding it by the address of a
  thread-local variable.
- `Mutex<T>` needs `T: Send`, so it cannot hold a `List` or a class with `var` fields (Q181 of
  ADR-0045).
