# 3. Memory model and mutability

## Value types and reference types

| | `struct` (F1) | `class` (F2) |
|---|---|---|
| Kind | Value type | Reference type |
| Storage | Inline / stack; copied or moved | Heap, managed by ARC |
| Created with | A literal: `Point { x: 1.0, y: 2.0 }` | `new`: `new Dog("Rex")` |
| Identity | None | Yes (`==` is identity) |
| Inheritance | No (may implement interfaces) | Single inheritance |

`new` is only valid with classes, so `new` always means "ARC heap allocation".

## Automatic Reference Counting (F2)

- Every class instance carries a reference count. The compiler inserts retain/release
  operations; the developer never writes them (outside `unsafe` FFI code).
- An object is destroyed deterministically when its count reaches zero. There is no
  tracing garbage collector and no pauses.
- Reference cycles are not collected. `weak` references break cycles; a `weak` reference
  does not keep its target alive (declaration syntax: open question Q12).
- In the MVP the program is single-threaded and reference counting is **non-atomic**.

## Bindings: `const` and `var` (F1)

```sag
const x = 5;     // cannot be reassigned
var total = 0;   // can be reassigned
total += x;
```

- `const` at module level declares a compile-time constant: `const PI: f64 = 3.14159;`.
- There is no `let` and no `mut`.

## Struct mutability (F1)

- Struct fields do **not** take `const`/`var`. A struct's mutability comes from the binding
  that holds it: everything inside a `const` struct is immutable; a `var` struct is mutable.
- Methods that mutate the struct declare `var self`; they can only be called on a `var`
  binding.

```sag
struct Point {
  pub x: f64;
  pub fn move_by(var self, dx: f64) { self.x += dx; }
}

var p = Point { x: 0.0 };
p.move_by(1.0);   // ok
const q = Point { x: 0.0 };
q.move_by(1.0);   // error: `q` is not `var`
```

## Class field mutability (F2)

- Every class field is declared `const` (immutable after construction) or `var` (mutable).
- A `var` field can be modified through **any** reference to the object.
- `const` fields must be assigned exactly once in the constructor, before use.
- Protection against data races is delegated to the future concurrency model
  (`Send`/`Sync`, post-MVP); in the MVP there is a single thread.
