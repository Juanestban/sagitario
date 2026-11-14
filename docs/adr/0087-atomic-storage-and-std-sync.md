# 0087. Atomic storage for the standard library, and `std::sync` and `std::thread` in Sagitario (S1)

- Status: Accepted
- Date: 2026-11-11
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q543: the intrinsics for
  atomics, brought forward from S3), [ADR-0085](0085-conditional-compilation-and-std-time.md)
  (Q554: `std::thread` and `std::sync` last in S1) and [ADR-0046](0046-mutex-and-atomics.md)
  (Q187: how the atomics are implemented)

## Context

`std::thread` and `std::sync` are the last modules of S1. Their C in the runtime starts and joins
threads, counts processors, and implements `Mutex` (a lock, a condition variable and the thread
holding it) and the atomic cells behind `AtomicBool`, `AtomicI32`, `AtomicI64`, `AtomicU32`,
`AtomicU64` and `AtomicUsize`, each one a runtime object that the class holds in a `fn()` field.

The C API of threads, locks and conditions can be declared with `extern "C"`, as S1 did for the
other modules. Atomics cannot: C11's `atomic_load`, `atomic_fetch_add` and the rest are generic
macros of `<stdatomic.h>`, not functions, and they act only on objects declared `_Atomic`. Rust
writes its atomics on compiler intrinsics over raw pointers (`core::intrinsics::atomic_load`),
whose memory is an `UnsafeCell`; strict C11 does not define treating a plain `uint64_t` as an
`_Atomic` one, so Sagitario's intrinsics need storage the compiler declares `_Atomic` itself.

Q543 planned intrinsics for atomics in S3, for the runtime's core. `std::sync` needs the atomic
part now; the rest (allocation, thread-local storage, code without reference counting) stays in
S3.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q565 | The type | `std::sys::Atomic<T>`, provided by the compiler as `CString` is, holds a `T` that threads read and change at once. Like `std::sys`, only the modules of `std` can name it (`E0200` elsewhere): the public atomics of `std::sync` keep their API. `T` is `bool` or an integer type (`i8`–`i64`, `u8`–`u64`, `isize`, `usize`); another `T` is an error (`E0318`). |
| Q566 | Where it may be | Only as the type of a `const` field of a class, since an atomic must keep one address and never be copied: an instance of a class lives in one place. Anywhere else (a local, a parameter, a result, a field of a struct or enum, a `var` field, a type argument, a static) is an error (`E0339`), as a reference outside a parameter is (ADR-0079): both are types allowed in one place only. Only `std` can name `Atomic`, so no program reaches these errors, and they need no codes of their own: a `compile-fail` test cannot name `std::sys` (`E0200`). |
| Q567 | Initialization | `Atomic::new(value)` is only allowed as the value given to such a field in the initializer of its class (`self.count = Atomic::new(0);`), and the C is `atomic_init`. The field is read only through its methods: reading it as a value is an error (`E0339`). |
| Q568 | Operations | Methods, safe to call: `load() -> T`, `store(value)`, `swap(value) -> T`, `compare_exchange(expected, desired) -> Result<T, T>` (`Ok(expected)` if it stored `desired`, `Err(actual)` if not), and on integers `fetch_add(n) -> T` and `fetch_sub(n) -> T`, which return the previous value and wrap around at the type's limits in every build, as C11 defines atomic arithmetic. Every operation is sequentially consistent (`atomic_load`, `atomic_compare_exchange_strong`...). Orderings like Rust's `Ordering` are left for a later record. |
| Q569 | Send and the C | An `Atomic<T>` field does not keep its class from being `Send` (ADR-0045): threads may share it, which is its purpose. The C of the field is `_Atomic T`. A program whose types hold an `Atomic` is compiled with `SAG_THREADS`, as one using `std::sync` is today, so `<stdatomic.h>` is included and MSVC gets `/experimental:c11atomics`. |
| Q570 | Increment 2: `Mutex` | `std::sys` declares `pthread_mutex_*` and `pthread_cond_*` on Unix, and the `SRWLOCK` and `CONDITION_VARIABLE` functions on Windows. `Mutex` keeps its lock and condition in fields of its own: on Unix, 64 bytes aligned to 8 each, room for `pthread_mutex_t` and `pthread_cond_t` of every supported C library (glibc, bionic and Apple's are at most 64 and 48 bytes); on Windows, one pointer each. C changes that memory through `&raw const` turned into `*mut`, which the generated C allows: its fields are never `const`. The thread holding the lock is an `Atomic<usize>` with the thread's id (`pthread_self`, `GetCurrentThreadId`), for the panic on locking a mutex the thread holds. `pthread_mutex_destroy` and `pthread_cond_destroy` are not called: Sagitario has no destructors, and the default mutexes and conditions of these C libraries hold nothing to free. `Channel`, written on the lock of `Mutex`, follows it. |
| Q571 | Increment 3: `std::thread` | `std::sys` starts and joins threads with `pthread_create` and `pthread_join`, or `CreateThread` and `WaitForSingleObject`, and counts processors with `sysconf`, `sysctlbyname` or `GetActiveProcessorCount`. What every thread does when it starts and ends (the frees it defers, its tasks, its statics and the count of running threads for the leak check) belongs to the runtime's core, which stays C until S3: the core gives `std` one function that prepares a thread from its body and one that runs it, and the entry of the thread, an `extern "C" fn` of `std::sys`, calls the second. |
| Q572 | Increments | **1** `Atomic<T>`: the resolver, the checks of Q565–Q567, the methods of Q568, SIR and the C of Q569, and the six atomics of `std::sync` written on it, which the tests of `std::sync` exercise; the runtime's atomic cells are deleted. **2** `Mutex` and `Channel` (Q570); the runtime's mutex is deleted. **3** `std::thread` (Q571). |
| Q573 | The editor | `Atomic` exists only for `std`, so the editor offers nothing new; the public API of `std::sync` does not change (CLAUDE.md). |

### Example

```sag
// In compiler/std/sync.sag.
use std::sys::Atomic;

pub class AtomicI64 {
  prv const value: Atomic<i64>;

  pub fn new(self, value: i64) {
    self.value = Atomic::new(value);
  }

  pub fn fetch_add(self, n: i64) -> i64 {
    self.value.fetch_add(n)
  }
}
```

## Alternatives considered

- **Intrinsics on raw pointers, as Rust's:** they act on any memory, but C11 defines atomic
  operations only on `_Atomic` objects, and converting a `uint64_t *` to `_Atomic uint64_t *` is
  not strictly conforming, which the generated C must be (CLAUDE.md).
- **Keeping a runtime cell per atomic (today):** no change to the language, but the atomics stay
  C, against S1, and each one costs an allocation and a call.
- **A public `Atomic<T>` in `std::sync`:** programs could put atomics in their own classes
  without an allocation each, but it is public API to specify and keep; the private type can
  become public later by a record of its own.
- **Orderings (`Relaxed`, `Acquire`, `Release`, `AcqRel`) now:** faster in some code, with more
  rules to check; `std::sync` needs only the sequential consistency it promises (spec 11).

## Consequences

- `std::sync` and `std::thread` leave the runtime, except the start and end of each thread,
  which S3 moves with the rest of the core.
- An atomic of `std::sync` is one allocation, its instance, instead of two.
- A `Mutex` is never destroyed for its C library: right for the default mutexes of glibc, bionic
  and Apple's C library, which allocate nothing; a C library that did would leak it.
- The compiler gains its first intrinsic type, the start of the low-level layer of S3 (Q543).

## Notes

- Increment 1 (`Atomic<T>`) is done. The resolver gives `std::sys::Atomic` as a builtin of
  `std` (`StdItem::Atomic`, `TypeRes::Atomic`, `Atomic::new`); the type checker has
  `TypeKind::Atomic`, lowered only for a `const` field of a class (`Checker::atomic_field`), and
  reports the expressions of that type not used through a method or as the value of their field
  when a function ends (`atomic_uses`). SIR initializes the field with `atomic_init` and lowers
  the methods to C11's generic operations (`atomic_load`, `atomic_compare_exchange_strong`...),
  which the C calls as it calls the runtime. The six atomics of `std::sync` hold an
  `Atomic<T>`, and the runtime's atomic cells (about 80 lines) are deleted.
  `tests/run-pass/sync.sag` and `threads.sag` exercise them, with the sanitizers.
- Increment 2 (`Mutex` and `Channel`) is done. `std::sys::Lock` holds the lock, the condition
  and the thread holding it, as Q570 says: `pthread_mutex_*` and `pthread_cond_*` over buffers of
  64 bytes on Unix, `SRWLOCK` and `CONDITION_VARIABLE` on Windows. The thread's id is
  `pthread_self` (an integer or a pointer of a pointer's size on every supported system) or
  `GetCurrentThreadId`. `Mutex` and `Channel` hold a `Lock`, and the runtime's mutex (about 110
  lines) is deleted; the core keeps the declarations of `SRWLOCK` that its own locks use (the
  inbox of `spawn_blocking`, the statics). A relocked mutex now panics at the line of `std::sys`
  that checks it, as Rust's panics point into its `std`.
- Increment 3 (`std::thread`) is done, and with it S1. `std::sys` starts threads with
  `pthread_create` or `CreateThread`, whose entry is `sag_thread_entry`, an `extern "C" fn` of
  `std::sys`, joins them with `pthread_join` or `WaitForSingleObject`, and counts processors with
  `sysconf(_SC_NPROCESSORS_ONLN)` (84 in glibc, 0x61 in bionic, 58 on Apple systems) or
  `GetActiveProcessorCount`. The core gives `sag_thread_prepare`, `sag_thread_run` and
  `sag_thread_finish` instead of `sag_thread_start`, `sag_thread_join` and
  `sag_thread_available_parallelism`; it keeps a thread start of its own, static, for the jobs of
  `task::spawn_blocking`, which belong to the executor of the core until S3. SIR now makes
  instances only of the `extern "C" fn`s of `std` with an `@export` name, which the core calls:
  the entry of threads is made only for a program that starts one, so other programs never
  reference the core's thread functions. `<sys/sysctl.h>` leaves the runtime.
- With locks and threads in `std::sys`, the C file of the standard library's declarations
  (`sag_std_ffi.c`) declares only the C functions the program calls or takes as values
  (`codegen_c::used_externs`), instead of all of `std::sys`: a program without threads does not
  link `pthread_*`, which glibc before 2.34 keeps in `libpthread`, and one that never runs a
  program does not need `posix_spawn_file_actions_addchdir_np`. A "hello, world" declares three
  C functions, its clocks.
