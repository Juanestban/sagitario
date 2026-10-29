# 0072. `async`/`await`: increments and tasks

- Status: Accepted
- Date: 2026-10-28
- Refines: [ADR-0048](0048-async-await.md) (phase F8)

## Context

[ADR-0048](0048-async-await.md) settled the model of `async`/`await` (Q202–Q208): stackless
coroutines lowered in SIR to state machines, `async fn` and a prefix `await`, a `Task<T>` that
starts at once, one single-threaded executor per thread with `async fn main` and
`task::block_on`, tasks, `task::sleep` and `task::spawn_blocking` as what can be awaited, errors as
values and no cancellation. It left open how F8 is built, and the rules a program meets: which
functions may be `async`, where `await` may appear, what a `Task` is, in which order tasks run,
and what happens to the tasks nobody waits for. This record settles them for the first increment
and plans the others.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q418 | Increments | **1** tasks: `async fn` at module level, `await`, `Task<T>`, the state machine, the executor with timers, `task::sleep`, `task::block_on` and `async fn main`. **2** `task::spawn_blocking`, and waking the executor from another thread. **3** `async` in the rest of the language: methods of classes, structs, enums and interfaces, generic `async fn`, and `async fn` as values of function types `fn(...) -> Task<T>`. **4** the acceptance program and closing F8. Until increment 3, an `async` method, a generic `async fn` or an `async fn` used as a value is `E0900` (not implemented yet). |
| Q419 | `async fn` | `async` goes after the visibility and before `fn`: `pub async fn f(x: i32) -> T`. Calling it returns a `Task<T>`; inside, `return` and the final expression give a `T`. An `async fn` cannot be `extern "C"` or `@export` (`E0337`). Its parameters are kept by the task: managed ones are retained when it starts, since the caller may release them before the task ends. |
| Q420 | `await` | `await e` needs `e` of type `Task<T>` and gives a `T`. Its operand is the chain of calls, fields, methods and indexing that follows, up to the first `?`; that `?`, and anything after it, apply to the awaited value: `await f()?` is `(await f())?`, and `await a.b(c)?.d()` is `(await a.b(c))?.d()`. `await` is allowed in any expression of the body of an `async fn`, also inside `if`, loops, `match` and conditions, but not inside a lambda in it, nor outside an `async fn` (`E0336`). |
| Q421 | `Task<T>` | A class of the prelude, written in Sagitario (`compiler/std/prelude.sag`): its result is an `Option<T>`, and the tasks waiting for it a `List<fn()>` of the functions that resume them. Copying a `Task` copies the reference. A task that has finished keeps its result: awaiting it again gives it again, as a copy. It has no public members: only `await`, `std::task` and the code the compiler generates use it, and `new Task<T>()` outside the standard library is an error (its initializer is `prv`). `Task` is not `Send`, so a closure passed to `thread::spawn` cannot capture one (`E0332`). |
| Q422 | Running | Calling an `async fn` runs it at once, in the calling thread, until its first `await` of a task that has not finished, or until its end. A suspended task resumes when the task it awaits finishes, in the order the executor finds them ready: first in, first out. Timers fire in the order of their deadlines, and in the order of `sleep` for equal ones. The output of a program that only uses tasks and `sleep` is the same on every run. |
| Q423 | The executor | One per thread, in the runtime, running the tasks of that thread. `task::block_on(t)` runs it until `t` finishes and returns its result; called from inside a task it panics (`block_on inside a task`). `async fn main()` (and `-> Result<(), E>`, as `main`) is run with `block_on`. Synchronous code that calls an `async fn` without `block_on` only runs it up to its first suspension. |
| Q424 | Tasks nobody waits for | When `block_on` returns, the other tasks that have not finished stay for the next `block_on` of that thread. When the program ends, they are released without running further: the values they hold are released, and the leak check finds nothing. |
| Q425 | An unused `Task` | A call whose `Task` is discarded (an expression statement of type `Task<T>`) is the warning `W0009`, which says to `await` it or to bind it: `const _task = f();` keeps it running in the background without a warning, as for other unused bindings. |
| Q426 | `std::task` | `task::sleep(d: Duration) -> Task<()>` finishes after at least `d`; `sleep` of a zero duration still lets the ready tasks run first. `task::block_on<T>(t: Task<T>) -> T`. Both are written in Sagitario on the `prv` members of `Task`: the standard library gains a third exception, that its modules may use the `prv` members of the prelude's types, since without re-exports (`pub use`) `Task` cannot live in `std::task`. |
| Q427 | Lowering | An `async fn` becomes two functions. The one callers call creates the `Task<T>` and a frame, and runs the frame once. The frame is the environment of a closure `fn()`, the other function, whose captures are the task and the parameters, and which also holds its state and every local of its SIR, so that they survive a suspension. Its SIR body is that of the `async fn`, where `return` completes the task, and `await` is an `Await` statement: when the task awaited has not finished, it adds the closure of the frame to that task's waiters, records the state, and returns; a `switch` at the start jumps back after it (`goto`, valid in C11 since every value lives in the frame and no local is a variable length array). Each `Await` also records the managed locals that own values at that point (the bindings of the open blocks and the pending temporaries), which are released when an unfinished task is released (Q424). The runtime keeps, per thread, the ready closures, the timers and the suspended frames. |
| Q428 | Diagnostics | `E0336` (`await` outside an `async fn`), `E0337` (an `async fn` that cannot be one), `W0009` (an unused `Task`). `async` and `await` stop being reserved, so `E0101` has no keyword left to reject: it is retired, removed from the registry of codes (which requires a `compile-fail` case for each) and not reused; a future reserved word gets a new code. |
| Q429 | Tests | `run-pass` cases for awaiting finished and unfinished tasks, `await` in `if`, loops and `match`, managed values across `await` with the leak check, the order of tasks and timers, awaiting twice, `?` after `await`, `async fn main` with and without `Result`, `block_on` from synchronous code and tasks left unfinished; `run-panic` for `block_on` inside a task; `compile-fail` for each code; the `types` and `c` dumps of a small `async fn`. |
| Q430 | Documents | This record; a chapter of the specification, `12-async.md`; `std::task` in the standard library; `async`/`await` no longer reserved in the lexical chapter; `Task` in the prelude; `await` in the table of operators; `docs/architecture.md` (the lowering and the executor); the roadmap. |

## Alternatives considered

- **One increment for F8:** the state machine is most of the work; methods and function values
  add their own questions (a struct's `self` modified across an `await`, interfaces returning
  tasks), better settled on a lowering that already works.
- **Waiting for every task at the end of `main`, as Node does:** a forgotten task would keep a
  program running; C# and Rust end with `main`.
- **Silently discarding an unused `Task`:** like a floating promise of TypeScript, it hides a
  missing `await`; binding it to `_task` says that it is meant.
- **A task that can be awaited once, as Rust's `JoinHandle`:** promises and C# tasks can be
  awaited again, which Sagitario's users expect.
- **A control-flow graph in SIR for the state machine:** SIR is structured, and `goto` into the
  C of the same body reaches every `await` without changing the other passes.
- **Locals in the C stack, saved at each `await`:** saving needs the live locals at every
  suspension anyway, and a frame is what a task has to keep.

## Consequences

- `async` and `await` are no longer reserved words; `Task` is a name of the prelude.
- The runtime gains the executor, its timers and the task header; a program that uses no
  `async fn` does not change.
- Increments 2 and 3 add their tables to this record's plan, as F7 did.
