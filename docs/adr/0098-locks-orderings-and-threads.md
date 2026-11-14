# 0098. Lock guards, condition variables, atomic orderings and threads (S2)

- Status: Accepted
- Date: 2026-11-13
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: record 9 of the
  audit of S2), [ADR-0046](0046-mutex-and-atomics.md) (Q189: `Mutex` without guards, which
  needed destructors), [ADR-0087](0087-atomic-storage-and-std-sync.md) (Q565: the types of
  `std::sys::Atomic`; Q568: sequential consistency only), [ADR-0047](0047-channels-and-thread-utilities.md)
  (`Channel`), docs/spec/11-standard-library.md (`std::sync`, `std::thread`)

## Context

Record 9 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) is the rest of Rust's `sync`
and `thread`. Sagitario's `Mutex` takes and releases its lock in each of `get`, `set` and
`update(f)`, since ADR-0046 had no destructors for a guard to unlock with; so there is no
`try_lock`, and no `Condvar`, which waits with a lock held. `RwLock` and `Barrier` are missing.
`Channel` has no bound, and nothing that receives without waiting or waits for a time. The
atomics are sequentially consistent only, of six types, with `fetch_add` and `fetch_sub` as
their only arithmetic. Threads have no name, no identifier, no `Builder`, and cannot park.

[ADR-0096](0096-streams-files-and-drop.md) gave classes `Drop`, which runs when the last strong
reference of an instance goes: for a local, at the end of its scope. That is what a guard needs,
so the record follows Rust in each part, with three differences of the language:

- A panic ends the whole program (spec 07), so no lock is ever poisoned: what Rust returns as a
  `LockResult` is the guard itself.
- A guard cannot give a `&mut T`, since references live in parameters only
  ([ADR-0079](0079-references-and-borrowing.md)); it gives a copy of the value and replaces it,
  as `Mutex` does today. A `Mutex<T>` still needs `T: Send`, so the value is never a `List`.
- A module of `std` is one file, so Rust's `std::sync::atomic::Ordering` is `std::sync::Ordering`
  and `std::sync::mpsc` is the `Channel` Sagitario has.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q671 | Lock guards | `Mutex::lock() -> MutexGuard<T>`, which waits for the lock, and `try_lock() -> Result<MutexGuard<T>, TryLockError>`, whose only error is `TryLockError::WouldBlock`, as Rust's without poisoning. A guard has `get() -> T` (a copy of the value) and `set(value)`, and implements `Drop`, which unlocks: a guard in a local unlocks at the end of its scope, as in Rust, and one kept in a field or a list keeps the lock until it goes. `get`, `set` and `update(f)` of `Mutex` stay, each locking for one operation. A thread locking a mutex it holds panics, as today. The guards of `std::sync` are not `Send`: the compiler's list of its classes, which are `Send` by name (ADR-0046, Q190), leaves them out, since a lock must be released by the thread that took it. |
| Q672 | `Condvar` | `new Condvar()`; `wait(guard) -> MutexGuard<T>`, which releases the lock while it waits and takes it again; `wait_while(guard, condition: fn(T) -> bool) -> MutexGuard<T>`, which waits while `condition` holds for the value; `wait_timeout(guard, duration)` and `wait_timeout_while(guard, duration, condition)`, which give `(MutexGuard<T>, WaitTimeoutResult)`, whose `timed_out() -> bool` says whether the time ran out; `notify_one()` and `notify_all()`. `wait` and `wait_timeout` may return without a notification, as Rust's. A `Condvar` waits with one mutex: waiting with another panics, as Rust's may. |
| Q673 | `RwLock` | `RwLock<T: Send>`, which many threads read at once and one writes: `read() -> RwLockReadGuard<T>` (`get`), `write() -> RwLockWriteGuard<T>` (`get`, `set`), `try_read()` and `try_write()` with `TryLockError`, and `get`, `set` and `update(f)` as `Mutex`'s. A thread taking a lock it holds for writing panics; reading again while it reads may wait forever if a writer is waiting, as Rust's says. |
| Q674 | `Barrier` | `new Barrier(n)`; `wait() -> BarrierWaitResult` waits until `n` threads have called it, and `is_leader() -> bool` is true for one of them; it can be used again. `n` of 0 is as 1, as Rust's. |
| Q675 | `Channel` | `Channel::bounded(capacity) -> Channel<T>`, whose `send` waits while it holds `capacity` values; with 0, `send` waits until a receiver takes the value, as Rust's `sync_channel(0)`. `try_send(value) -> Result<(), TrySendError<T>>` (`Full(value)`, `Closed(value)`), `try_receive() -> Result<T, TryReceiveError>` (`Empty`, `Closed`) and `receive_timeout(duration) -> Result<T, ReceiveTimeoutError>` (`Timeout`, `Closed`), as Rust's `TrySendError`, `TryRecvError` and `RecvTimeoutError`. `new Channel<T>()` stays without a bound, `send` on a closed channel still panics, and `receive` still gives `None` once it is closed and empty. |
| Q676 | Atomic orderings | `std::sync::Ordering` (`Relaxed`, `Release`, `Acquire`, `AcqRel`, `SeqCst`), and every operation takes it, as Rust's: `load(order)`, `store(value, order)`, `swap(value, order)`, `compare_exchange(current, new, success, failure)`, `compare_exchange_weak` (which may fail although the value is `current`), the `fetch_*` operations, and `fetch_update(set_order, fetch_order, f: fn(T) -> Option<T>) -> Result<T, T>`. The orderings Rust rejects panic, as Rust's: `load` with `Release` or `AcqRel`, `store` with `Acquire` or `AcqRel`, and a `failure` ordering of `Release` or `AcqRel`. `fence(order)` is Rust's `atomic::fence` (`Relaxed` panics). It is written `sync::Ordering`, or imported with `use std::sync::Ordering`, which hides the prelude's `Ordering` of comparisons in that file, as both exist in Rust. The C is C11's `atomic_*_explicit` with the `memory_order` of each. This changes every call of an atomic in existing programs. |
| Q677 | Atomic types and operations | Besides the six: `AtomicI8`, `AtomicU8`, `AtomicI16`, `AtomicU16`, `AtomicIsize`, and `AtomicPtr<T>` with `load`, `store`, `swap`, `compare_exchange`, `compare_exchange_weak` and `fetch_update` on `*mut T`, as Rust's; `std::sys::Atomic` may also hold a raw pointer (Q565), whose C is `_Atomic(T *)`. On integers: `fetch_add`, `fetch_sub`, `fetch_and`, `fetch_or`, `fetch_xor`, `fetch_nand`, `fetch_max` and `fetch_min`; on `AtomicBool`: `fetch_and`, `fetch_or`, `fetch_xor`, `fetch_nand` and `fetch_not`. Each returns the previous value; the arithmetic wraps around, as now. |
| Q678 | `thread::Builder` | `new Builder()`, `name(name) -> Builder`, `stack_size(bytes) -> Builder`, and `spawn(body) -> Result<JoinHandle<T>, io::Error>`, an error when the system cannot start the thread. Its `body` is checked as `thread::spawn`'s: a lambda or the name of a function, capturing `Send` values only. A name holding a NUL panics, as Rust's. The system gets the name where it takes one (`pthread_setname_np`, `SetThreadDescription`), cut to what it holds (15 bytes on Linux and Android); `name()` gives it whole. |
| Q679 | `Thread` and `ThreadId` | `thread::current() -> Thread` and `JoinHandle::thread() -> Thread`. A `Thread` has `id() -> ThreadId`, `name() -> Option<String>` (`Some("main")` for the main thread, `None` for a thread without one) and `unpark()`, and is `Send`, by name as the classes of `std::sync`. A `ThreadId` is a struct, compared, hashed and shown as Rust's (`ThreadId(1)`), unique among the threads of the program and never reused; the main thread's is 1. A thread that C started gets an identifier, and no name, when it first asks. |
| Q680 | `park` and `yield_now` | `thread::park()`, `park_timeout(duration)` and `Thread::unpark()`, with Rust's token: an `unpark` before the `park` makes it return at once, and `park` may return without an `unpark`. `yield_now()` lets the system run another thread (`sched_yield`, `SwitchToThread`). |
| Q681 | The editor | `sag lsp` completes the new types and functions as any of `std`; the README lists them. |
| Q682 | Increments | **1** The atomics: `Ordering` and `fence`, the new types and operations, `Atomic` of pointers, and the existing programs, tests and docs moved to orderings (Q676, Q677). **2** `MutexGuard` and `try_lock`, `Condvar`, `RwLock` and `Barrier` (Q671–Q674). **3** `Channel` (Q675). **4** `std::thread` (Q678–Q680). Each one with its tests, the spec and the editor, and the rows of the audit marked. |

### Example

```sag
use std::sync::{AtomicUsize, Condvar, Mutex, Ordering};
use std::thread;

fn main() {
  const ready = new Mutex(false);
  const changed = new Condvar();
  const started = new AtomicUsize(0);
  const worker = new thread::Builder().name("worker").spawn(() => {
    started.fetch_add(1, Ordering::Relaxed);

    const guard = ready.lock();

    guard.set(true);
    changed.notify_one();
  }).unwrap();

  {
    const guard = changed.wait_while(ready.lock(), (ready) => !ready);

    println(f"ready: {guard.get()}");
  }                                            // unlocked

  println(f"{worker.thread().name():?} {started.load(Ordering::Relaxed)}");
  worker.join();
}
```

## Alternatives considered

- **No guards** (`try_get`, `try_update(f)`, and `cv.wait_while(mutex, condition)`): no class
  that holds a lock, but a section of several statements under one lock is only a lambda, and
  Rust's form, which the record follows, is the guard.
- **`try_lock() -> Option<MutexGuard<T>>`:** shorter without poisoning, but Rust's code
  (`if let Ok(guard) = m.try_lock()`) would not read the same.
- **Sequential consistency only, or orderings as extra methods** (`load_ordered(order)`): no
  change to existing programs, but not Rust's API; the parameter is what was chosen.
- **`std::sync::mpsc` with `Sender` and `Receiver`:** Rust's exact names, but a second channel
  beside `Channel`, which any thread may send to, receive from and close.
- **Leaving `park` out:** `Condvar` and `Channel` cover its uses, but Rust has it, and a `Thread`
  needs little more to have it.

## Consequences

- Every call of an atomic names its ordering, as in Rust; programs written for S1 must add
  `Ordering::SeqCst` to keep what they did.
- Sections of several statements under one lock, waiting for a condition, and readers sharing a
  lock become possible without a lambda.
- `std::sync` gains its first classes that are not `Send`, its guards.
- The audit's rows of `Mutex`, `RwLock`, `Condvar`, `atomic::*`, `Barrier`, `mpsc`,
  `thread::Builder` and the rest of `thread` are done.

## Notes

- Increment 1 is done. The methods of `std::sys::Atomic` take their orderings last, as numbers
  from 0 (`Relaxed`) to 4 (`SeqCst`), and `Atomic::fence(order)` is a builtin of `std`; the
  runtime turns the number into C11's `memory_order` (`SAG_MEMORY_ORDER`) for `atomic_*_explicit`,
  since the values of `memory_order` are the C library's and include `consume`. A program calling
  `fence` without an atomic is compiled with `SAG_THREADS` too. `std::sync` checks the orderings,
  and makes the `success` ordering of a compare-exchange as strong as its `failure` one, which
  C11 requires and Rust does not: what both ask for is kept. `fetch_nand`, `fetch_max`,
  `fetch_min`, the operations of `AtomicBool` other than `swap` and the exchanges, and
  `fetch_update` are loops of `compare_exchange_weak`, since C11 has no such operations (nor
  `fetch_and` on `atomic_bool`). `Atomic<*mut T>` in a generic class needed the type
  substitution to enter `Atomic`, which it never had to. The existing tests and the spec use
  `Ordering::SeqCst`, or `Relaxed` where only counting matters.
- Increment 2 is done. `std::sys` gains `Condition`, a condition variable that remembers the
  first `Lock` it waits with, by its address, as Rust's `Condvar` checks it (a static count would
  have been an atomic in every program, which `SAG_THREADS` would then have compiled), `RwLock`
  (`pthread_rwlock_t` in 256 bytes, or an `SRWLOCK` taken shared or exclusive), `try_lock`,
  `notify_one` and timed waits on `Lock`, and `Locked<T>` and `ReadWriteLocked<T>`, the lock and
  value a `Mutex` or an `RwLock` shares with its guards: `prv` is private to its type, so
  classes of one module cannot reach each other's members, and a `pub` method naming a private
  type is warned about. For the same reason the initializers of the guards and of the two
  results are public and take types of `std::sys`, which programs cannot name, as
  `Metadata::from_system` does. A timed wait on POSIX systems waits until a time of the system
  clock, `pthread_cond_timedwait`'s, and says whether the time ran out by the monotonic one. The
  compiler keeps the classes of `std::sync` whose names end in `Guard` out of those that are
  `Send` by name, with their own reason. A guard that is a temporary lives until the end of its
  statement, as in Rust, which the spec says.
- Increment 3 is done. A `Channel` keeps its capacity (`None` without a limit), a second
  `std::sys::Condition` that senders wait on, for room or for a receiver, and how many values were
  received: a send on a channel of capacity 0 queues its value, as one of capacity 1, and waits
  until that count reaches it, so `try_receive` takes the value of a sender that waits, as in
  Rust. `try_send` on such a channel succeeds while more receivers wait than values are queued.
  A send waiting for room panics if the channel is closed meanwhile, since its value was never
  sent; one whose value is queued returns. `receive_timeout` measures its time by `Instant`, as
  `Condvar::wait_timeout_while`.
- Increment 4 is done, and with it the record. A `ThreadId` is a struct with a private field;
  the language has no tuple structs and gives every struct the debug form of its fields, so
  `ThreadId(1)` is its `Display` text, and `{id:?}` is `ThreadId { value: 1 }`, which the user
  chose over an enum of one variant, which programs could build. What a `Thread` holds is a
  `std::sys::ThreadData` (its number, its name, and the lock and token of `park`), which a
  `@thread_local` static of `std::thread` keeps for the calling thread; `current()` wraps it, so
  that no method of `Thread` gives programs a type of `std::sys`. The main thread is the one that
  called `sag_env_init`: `std::sys` keeps its number in a plain `static var`, since an atomic
  static would make every program use threads. The compiler checks `Builder::spawn` where a
  method call is checked, and adds `Thread` to the classes that are `Send` by name.
  `std::sys::start_thread` takes the stack size and returns the `errno` value of a failure, and
  the core's `sag_thread_abandon` frees a thread the system did not start. On Windows the size
  only reserves the stack (`STACK_SIZE_PARAM_IS_A_RESERVATION`), as Rust's.
