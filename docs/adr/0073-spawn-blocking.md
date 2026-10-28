# 0073. `task::spawn_blocking`

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0072](0072-async-await-increments.md) (increment 2 of phase F8, Q418)

## Context

Since increment 1 tasks wait for other tasks and for timers, all in the thread that started them
([ADR-0072](0072-async-await-increments.md)). Work that blocks, such as reading a large file or a
long computation, stops every task of the thread while it runs. [ADR-0048](0048-async-await.md)
(Q206) planned `task::spawn_blocking(f)`, which runs `f` on a thread, with the rules of
`thread::spawn` for what it captures, and returns a `Task<T>`. Increment 2 adds it, and with it
the first wake-up of an executor from another thread.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q431 | The function | `task::spawn_blocking<T>(body: fn() -> T) -> Task<T>` starts a new thread that runs `body`, and returns at once a task that finishes with what `body` returns. As for `thread::spawn`, what `body` returns needs not be `Send`: it passes whole to the thread of the task. |
| Q432 | Its argument | The rules of `thread::spawn` ([ADR-0045](0045-threads-and-send.md)): a lambda whose captures are all `Send` (`E0332`), or the name of a function (`E0333` otherwise); `spawn_blocking` cannot be used as a value (`E0333`). The compiler checks it where code outside the standard library calls it. |
| Q433 | Waking the executor | Each executor has an inbox, shared with the threads of its `spawn_blocking`, guarded by a lock and a condition variable. When `body` returns, its thread puts its job in the inbox and wakes the executor, which joins the thread and completes the task: its waiters become ready, in the order the jobs arrived. While jobs are running and nothing else is ready, the executor waits on the inbox, until the next timer at most. |
| Q434 | Nothing left | `block_on` panics only when no task is ready, no timer is pending and no job is running. |
| Q435 | Ending | When the program (or the thread of the executor) ends with jobs still running, it does not wait for them, as with threads nobody joins; the tasks waiting for them are released, and a job that finishes later only releases what it holds. The leak check only runs when no thread is running, as before. A panic in `body` ends the program. |
| Q436 | The runtime | `sag_task_spawn_blocking(body, done)` runs `body` (which stores the result) on a thread of `std::thread`'s runtime (`sag_thread_start`), and `done` (which completes the task) on the executor once the thread is joined. A program that uses `spawn_blocking` is built with threads, so ARC counts are atomic (`SAG_THREADS`). |
| Q437 | Tests | `run-pass` cases, under ThreadSanitizer: jobs interleaved with `sleep` and other tasks, managed results, several jobs at once, and a job still running when `main` ends; `compile-fail` cases for its argument (`E0332`, `E0333`). |
| Q438 | Documents | This record, `docs/spec/12-async.md` (`std::task`), `docs/spec/11-standard-library.md` (the check of the argument), `docs/architecture.md` (the inbox) and the roadmap. |

## Alternatives considered

- **A pool of threads, as Tokio's blocking pool:** fewer threads under load, but a pool needs a
  size, a queue and threads that outlive the work; one thread per job is what `thread::spawn`
  already does, and a pool can replace it without changing the language.
- **Polling the jobs from the executor:** simpler, but it either spins or adds latency; a
  condition variable wakes it exactly when a job ends or a timer is due.
- **Requiring a `Send` result:** the result is only reachable from the job's thread until it
  finishes, as for `thread::spawn` (ADR-0045).

## Consequences

- Blocking work runs beside the tasks of a thread without stopping them.
- `std::task` gains the first use of threads by the executor; tasks themselves still never
  change thread.
- Increment 3 (`async` methods, generic `async fn`, `async fn` as values) remains.
