# 12. `async` and `await` (F8)

Rationale: [ADR-0048](../adr/0048-async-await.md), [ADR-0072](../adr/0072-async-await-increments.md),
[ADR-0073](../adr/0073-spawn-blocking.md), [ADR-0074](../adr/0074-async-methods-generics-and-values.md).

```sag
use std::task;
use std::time::Duration;

async fn later(name: String, millis: u64) -> String {
  await task::sleep(Duration::from_millis(millis));
  `${name} after ${millis} ms`
}

async fn main() {
  const slow = later("slow", 20);
  const fast = later("fast", 10);

  println(await fast);
  println(await slow);
}
```

## `async fn`

- `async` goes after the visibility and before `fn`: `pub async fn f(x: i32) -> T`. Calling it
  returns a `Task<T>`; inside it, `return` and the final expression give a `T`.
- The call runs the function at once, in the calling thread, until its first `await` of a task
  that has not finished, or until its end; it then returns the task.
- The task keeps its parameters until it finishes: the caller may release its own references
  earlier.
- An `async fn` cannot be `extern "C"` or `@export`, nor an initializer `new` (`E0337`).
- Functions of a module, generic functions, and methods of classes, structs, enums and interfaces,
  also `abstract`, `override`, static and default ones, may be `async`. Lambdas cannot.
- An `async fn` returning `T` has the type of a function returning `Task<T>`: used as a value, an
  `async fn` has the type `fn(A, ...) -> Task<T>`, and `async fn f(self) -> T` and
  `fn f(self) -> Task<T>` implement and override each other. An `abstract` or interface method
  declared `async` has no body.

## `self` in `async` methods

- In a class, the task keeps a reference to the instance: an `async` method may modify its fields,
  and sees the changes other code makes to them while it waits.
- In a struct or an enum, and in a default method of an interface, the task keeps a copy of
  `self`, taken when the method is called. The method cannot modify `self`, directly or by calling
  a method that modifies it (`E0338`): the caller's value would not change. In a default method,
  assigning to the fields of `self` is `E0338`; a method of the interface it calls on `self` may
  modify the copy, which the caller does not see.

## `await`

- `await e` requires `e` of type `Task<T>`, and gives a `T`: when the task has finished, at once;
  otherwise the running task is suspended until it finishes.
- `await` is a prefix operator. Its operand is the chain of calls, fields, methods and indexing
  that follows, up to the first `?`; that `?` and anything after it apply to the awaited value:
  `await f()?` is `(await f())?`, and `await a.b(c)?.d()` is `(await a.b(c))?.d()`. Other
  operators bind as for unary operators: `-await f()` and `!await g()` negate the result.
- `await` is allowed anywhere in the body of an `async fn`, also in conditions, loops and `match`,
  but not inside a lambda written in it, nor outside an `async fn` (`E0336`).
- Bindings keep their values across an `await`, as across any call.

## `Task<T>`

- `Task<T>` is a class of the [prelude](09-modules.md#prelude) without public members: it is used
  through `await` and `std::task`, and only calling an `async fn` or `task::sleep` creates one.
  Copying a `Task` copies the reference.
- A task that has finished keeps its result: awaiting it again gives it again, as a copy.
- `Task` is not `Send` ([interfaces](05-interfaces-generics.md#send-f4)): a task belongs to the
  thread that started it.
- Discarding a `Task` (an expression statement whose value is a `Task`) is the warning `W0009`.
  Binding it, also to a name starting with `_` (`const _task = f();`), keeps the task running
  without a warning.

## Running tasks

- Each thread has one executor, which runs the tasks started in it, one at a time.
- A suspended task becomes ready when the task it awaits finishes. Ready tasks run in the order
  they became ready. Timers fire in the order of their deadlines, and in the order of their
  `sleep` for equal deadlines. A program that only uses tasks and `sleep` gives the same output on
  every run.
- `task::block_on(t)` runs the executor of the thread until `t` finishes, and returns its result.
  Called from inside a task, it panics, and so it does when no task, timer or `spawn_blocking`
  job can make `t` finish.
- `async fn main()` and `async fn main() -> Result<(), E>` (see [errors](07-errors.md)) run their
  body with `block_on`.
- Synchronous code that calls an `async fn` without `block_on` only runs it up to its first
  suspension.
- When `block_on` returns, the tasks that have not finished stay for the next `block_on` of the
  thread. When the program ends, they are released without running further.
- A panic in a task ends the program, as everywhere. Tasks cannot be cancelled.

## `std::task`

| Item | Meaning |
|---|---|
| `sleep(duration: Duration) -> Task<()>` | Finishes after at least `duration`; a zero duration lets the ready tasks run first |
| `block_on(task: Task<T>) -> T` | Runs the executor until `task` finishes, and returns its result |
| `spawn_blocking(body: fn() -> T) -> Task<T>` | Runs `body` on a new thread; the task finishes with what it returns |

- `spawn_blocking` is for work that would stop the other tasks of the thread: reading a large
  file, a long computation. Its argument follows the rules of `thread::spawn`
  ([standard library](11-standard-library.md#stdthread)): a lambda capturing only `Send` values,
  or the name of a function; what `body` returns needs not be `Send`. `spawn_blocking` cannot be
  used as a value.
- While a `spawn_blocking` job runs and no task is ready, the executor waits for it (and for the
  next timer). `block_on` panics only when nothing is ready, no timer is pending and no job is
  running.
- When the program ends, it does not wait for jobs still running, as for threads nobody joins.
  A panic in `body` ends the program.
