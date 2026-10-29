# 0075. The acceptance of F8

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0072](0072-async-await-increments.md) (increment 4 of phase F8, Q418)

## Context

Increments 1 to 3 ([ADR-0072](0072-async-await-increments.md), [ADR-0073](0073-spawn-blocking.md),
[ADR-0074](0074-async-methods-generics-and-values.md)) built tasks, `spawn_blocking` and `async`
across the language, each with its own tests. The roadmap left the acceptance of F8 to increment
4 without defining it. As for F2 and F4, it should be one program that uses the features together,
as real code would, rather than one more case per rule.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q448 | The program | `tests/run-pass/reference_async/`, a program of two modules: `async fn main() -> Result<(), io::Error>` writes 6 files and starts one task per file. Each task reads its file on a thread (`spawn_blocking`) through an `async fn` passed as a value, and summarizes it through an interface whose `async` methods are implemented by a class (which counts the words on a thread and adds them to one of its fields) and by a struct (which waits on a timer). A generic `async fn` gathers the results in order. `?` after `await` passes the error of a missing file. Timers started out of order print in the order of their deadlines, and a task never awaited is released when `main` ends. |
| Q449 | How it runs | As a `run-pass` case under ThreadSanitizer (`// sanitize: thread`) where the C compiler has it, with ASan and UBSan elsewhere, and the leak check. Its output does not depend on how the threads are scheduled. |
| Q450 | Acceptance | F8 is done when the program passes on Windows, Linux and macOS. |
| Q451 | Closing F8 | The roadmap marks F8 done; the version of Sagitario becomes 0.10.0. What F8 left out stays for later phases: `async` lambdas, cancellation, a multi-threaded executor and asynchronous I/O ([ADR-0048](0048-async-await.md), Q203, Q205–Q207; ADR-0074, Q445). |
| Q452 | Documents | This record, the roadmap and `docs/testing.md`. |

## Alternatives considered

- **A program with timers only:** deterministic and simple, but it leaves out `spawn_blocking`
  and the executor waiting on threads, the riskiest part of F8.
- **Printing as each task progresses:** shows the interleaving, but with threads the order would
  depend on the scheduler; the program prints what it gathered, in a fixed order, and keeps the
  order of timers, which the executor fixes (ADR-0072, Q422).

## Consequences

- F8 is complete: tasks, `await`, `std::task` and `async` in functions, methods and function
  values.
- The next work is the rest of F7 and roadmap v2.
