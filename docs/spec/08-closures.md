# 8. Closures (F2)

Decisions: [ADR-0012](../adr/0012-closures.md), [ADR-0033](../adr/0033-closures-details.md);
iterators: [ADR-0034](../adr/0034-collections-and-iterators.md).

## Syntax

Closures are arrow functions:

```sag
const double = (x: i32) => x * 2;
const add = (a: i32, b: i32) => { a + b };
const evens = nums.filter(n => n % 2 == 0);
```

- A single parameter may be written without parentheses: `n => n > 0`. `match` guards do not
  parse lambdas.
- The body is an expression or a block.
- The return type is inferred from the body: the type of the first `return` inside it, or else
  of the body's value. A lambda has no return annotation; annotate the binding instead
  (`const f: fn(i32) -> i32 = (n) => n + 1;`).
- Parameter types may be omitted when the expected type gives them: a parameter of a function
  type, a binding annotation, a field or a return type. Otherwise they are required:
  `const f = (a, b) => a + b;` is an error (E0311).
- In a call of a generic function, lambda arguments are checked after the others, so
  `apply((n) => n + 1, 5)` infers `T = i32` from `5`.

## Function types

- The type of functions and closures is written `fn(A, B) -> R`, e.g.
  `fn apply(f: fn(i32) -> i32, x: i32) -> i32`. `fn(A)` without an arrow returns `()`.
- Named functions and closures share the same type and are interchangeable.
- Function types are equal only when their parameters and results are identical; there is no
  variance.
- Function values are called like functions, `f(x)`, and cannot be compared with `==` or
  interpolated.
- `value.name(args)` calls a field `name` of a function type when the type of `value` has no
  method `name`; methods take precedence.

```sag
class Button {
  pub var on_click: fn(Counter);

  pub fn new(self, handler: fn(Counter)) {
    self.on_click = handler;
  }
}

button.on_click(counter);
```

## Functions as values

- Module functions and static functions (`Type::f`) are values: `const f = double;`.
- A generic function used as a value needs its type arguments, written (`first<i32>`) or taken
  from the expected type (`const f: fn(i32) -> i32 = identity;`).
- Methods bound to an instance (`obj.method` without a call) are not values (E0316, which
  suggests a lambda), nor are C functions and prelude functions such as `println` (E0314).
  Wrap them in a lambda: `() => obj.method()`, `(s) => println(s)`.

## Captures

- Closures capture **by value** when they are created: captured structs are copied; captured
  class references are retained (the object is shared, the binding is not). A later change to
  the captured binding is not seen by the closure.
- Captured variables are **read-only** inside the closure. Assigning to a captured variable,
  including compound assignment and assignment to fields of a captured struct, is a compile
  error (E0331); the diagnostic suggests `fold`/`reduce` or a class instance.
- `var` fields of a captured class instance may be modified: that changes the shared object,
  not the capture.
- `self` is captured like any other binding.

```sag
var total = 0;
nums.for_each((x) => total += x);           // error E0331: cannot mutate captured `total`
const total = nums.fold(0, (acc, x) => acc + x);   // ok

const counter = new Counter();
const tick = () => { counter.count += 1; };  // ok: changes the shared instance
```

## Control flow

- `return` inside a lambda returns from the lambda, not from the enclosing function.
- `break` and `continue` do not cross a lambda: inside one, they refer only to loops in its own
  body.
- `?` inside a lambda uses the lambda's return type.
- A lambda cannot refer to itself; recursion uses a named function.

## Reference cycles

A closure stored in a class field that captures `self` creates a cycle that ARC cannot
reclaim. The compiler emits warning **W0008** when a closure that captures `self`, or the
binding of the object itself, is assigned to a field of that object:

```sag
self.on_tick = () => { self.count += 1; };   // warning W0008
widget.callback = () => widget.redraw();      // warning W0008
```

Cycles through other paths are not detected. `weak` capture syntax is post-MVP.

## Closure methods of `Option` and `Result`

| Method | On | Result |
|---|---|---|
| `map(f)` | `Option<T>`, `Result<T, E>` | `f` applied to the `Some`/`Ok` value |
| `map_err(f)` | `Result<T, E>` | `f` applied to the `Err` value |
| `and_then(f)` | `Option<T>`, `Result<T, E>` | `f(value)`, itself an `Option`/`Result`, for `Some`/`Ok` |
| `unwrap_or_else(f)` | `Option<T>`, `Result<T, E>` | the value, or `f()` (`f(error)` for `Result`) |
| `filter(f)` | `Option<T>` | the value if `f(value)` is `true`, else `None` |

`None` and `Err` pass through unchanged where no function applies.

## Iterators

```sag
const squares = List::from([1, 2, 3, 4]).filter(n => n % 2 == 0).map(n => n * n).collect();
const total = (1..=10).fold(0, (sum, n) => sum + n);
```

- `Iter<T>` is a prelude class: a lazy sequence that computes each value when `next()` is
  called, giving `None` once it is exhausted. Iterators are references; consuming one through
  any copy advances it.
- Lists, arrays, ranges outside of `for`, `s.chars()`, `s.bytes()` and the `keys()`, `values()`
  and `entries()` of maps are iterators or give them. `list.iter()` reads the list by position
  at every step, like `for`.
- `new Iter(step)` makes a custom iterator from a function `step: fn() -> Option<T>`. Its state
  lives in class instances it captures, since captures cannot be assigned.
- Lists and arrays have the methods below directly, as shorthands for `.iter().method(...)`.

  | Method | Result |
  |---|---|
  | `map(f)` | `Iter<U>`: `f(value)` for each value |
  | `filter(f)` | `Iter<T>`: the values for which `f` returns `true` |
  | `take(n)`, `skip(n)` | `Iter<T>`: the first `n` values, or those after them |
  | `step_by(n)` | `Iter<T>`: the first value and every `n`-th after it; `n == 0` panics |
  | `collect()` | `List<T>` with the remaining values |
  | `count()` | `usize` |
  | `fold(init, f)` | `A`: `f(f(init, v1), v2)...` |
  | `for_each(f)` | `()` |
  | `any(f)`, `all(f)` | `bool`; stop at the first value that decides |
  | `find(f)` | `Option<T>`: the first value for which `f` returns `true` |

- The first five are lazy: nothing runs until the iterator is consumed by one of the others or
  by `for`.
- Iterators are not compared with `==` and cannot be interpolated.

## Representation

A closure is a function pointer plus an ARC-managed environment holding the captures.
Closures with no captures, and named functions used as values, do not allocate.
