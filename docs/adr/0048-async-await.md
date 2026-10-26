# 0048. `async`/`await`, for phase F6

- Status: Accepted (not implemented: planned for F8, which [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) moved it to, after [ADR-0057](0057-editor-tooling-and-independence-from-rust.md) had moved it to F7)
- Date: 2026-10-25
- Refines: [ADR-0045](0045-threads-and-send.md) (Q183)

## Context

ADR-0008 planned `async`/`await` after threads, and ADR-0045 (Q183) left their design to the last
increment of F4. Threads, `Send`, `std::sync` and channels exist now; `async` and `await` have
been reserved keywords since the MVP. This record settles how they will work, Q202–Q208, and
closes F4 with its acceptance program, Q209–Q210. Nothing of `async` is implemented here.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q202 | Model | Stackless coroutines: each `async fn` is lowered in SIR to a state machine, as in Rust, C# and JavaScript. Stackful coroutines (as in Go) need context switching through assembly or `ucontext`, which strict C11 does not have. |
| Q203 | Syntax | `async fn f() -> T`, and `await expr` as a prefix operator, as in TypeScript and C#, rather than Rust's `.await`. `await` is valid only inside an `async fn`. `await f()?` means `(await f())?`. `async` lambdas come later. |
| Q204 | Type | Calling an `async fn` returns a `Task<T>` and starts it at once, like JavaScript's promises and C#'s tasks, rather than Rust's lazy futures. `await task` waits until it finishes and gives its result. |
| Q205 | Executor | One single-threaded executor per thread, in the runtime. `async fn main` runs it; synchronous code runs a task to its end with `task::block_on(t)`. Tasks never change thread, so they need not be `Send`. A multi-threaded executor comes later. |
| Q206 | What can be awaited first | Other tasks, `task::sleep(d)`, and `task::spawn_blocking(f)`, which runs `f` on a thread (with the rules of `thread::spawn` for what it captures) and returns a `Task<T>`. Asynchronous file and network I/O waits until `std` has networking and an event loop per system (epoll, kqueue, IOCP). |
| Q207 | Errors and cancellation | Errors are values: `Task<Result<T, E>>`, with `?` after `await`. There is no cancellation at first. A panic ends the process, as everywhere. |
| Q208 | When | After F5 (self-hosting), as F6: the compiler written in Sagitario does not need `async`, and doing it first would delay that goal. `async` and `await` stay reserved. |
| Q209 | F4 acceptance program | `tests/run-pass/reference_wordcount/`, run with ThreadSanitizer (`// sanitize: thread`). It writes 12 files, and a producer thread sends their names through a `Channel<String>`. Workers (as many as `available_parallelism`, from 2 to 4) count the words of each file into their own `Map`, which `join` returns. An `AtomicI64` tracks progress and a `Mutex<i64>` the bytes read; `main` merges the maps and prints the words by frequency. The output does not depend on the number of threads, and the case passes the leak check. |
| Q210 | Closing F4 | This record; the roadmap marks F4 done and plans `async`/`await` as F6; `docs/testing.md` and the index of records. |

## Alternatives considered

- **Stackful coroutines or green threads:** simpler to write programs with, but not
  expressible in strict C11, and every task needs its own stack.
- **Lazy futures polled by an executor (Rust):** precise control, but surprising for the
  TypeScript and C# programmers Sagitario addresses.
- **A multi-threaded executor from the start:** tasks would have to be `Send`, and every
  `await` would be a point where the thread may change.
- **`async` before self-hosting:** it would delay F5 without helping it.

## Consequences

- F6 will need a state-machine transformation in SIR that keeps the bindings living across an
  `await` in the task's frame, with ARC retaining them there.
- The executor is part of the runtime. Its timers and its wake-ups from other threads use the
  locks and condition variables of `std::sync`.
- Until F6, using `async` or `await` is the reserved keyword error `E0101`, as since the MVP.
