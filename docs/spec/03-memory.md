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
- The type annotation is optional: `const x: i64 = 5;` or `const x = 5;`.
- A binding may be declared without a value (`var x: i32;`) if the compiler can prove it is
  assigned before every use (*definite assignment*); a `const` declared this way must be
  assigned exactly once. Reading a possibly unassigned binding is an error.
- **Shadowing** is allowed, also in the same block: `const x = 5; const x = x + 1;` declares a
  new binding.
- Scope is lexical: a binding lives until the end of the block that declares it.
- Parameters behave like `const`. Assigning to a `const` or a parameter is an error.
- Warnings: a `var` that is never reassigned (suggests `const`), and an unused binding unless
  its name starts with `_`.

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

## Struct members (F1)

```sag
struct Point impl Display {
  pub x: f64;
  pub y: f64;

  pub fn new(x: f64, y: f64) -> Self { Self { x: x, y: y } }
  pub fn origin() -> Self { Self { x: 0.0, y: 0.0 } }
  pub fn len(self) -> f64 { (self.x * self.x + self.y * self.y).sqrt() }
}
```

- Fields and methods are declared inside the struct body; every member **must** declare
  `pub` or `prv`, as in classes. `prv` means visible only inside the struct.
- A struct literal `Point { ... }` is only valid where **all** its fields are accessible.
  Structs with `prv` fields are built from outside through a static function.
- `struct Name impl I1, I2` implements interfaces (F2). Structs never use `extends`.

## Static functions (F1)

- A function declared inside a `struct`, `enum` or `class` without a `self` parameter is
  **static** and is called as `Type::name(...)`. There is no `static` keyword.
- In structs and enums, `new` is an ordinary static function by convention (it returns the
  type and is called `Point::new(...)`).
- In classes, `new` is the special initializer (see [classes](04-classes.md#construction))
  and can only be invoked through `new ClassName(...)`, never as `ClassName::new(...)`.
- There are no static fields and no mutable global variables in the MVP; use module-level
  `const`.

## Class field mutability (F2)

- Every class field is declared `const` (immutable after construction) or `var` (mutable).
- A `var` field can be modified through **any** reference to the object.
- `const` fields must be assigned exactly once in the constructor, before use.
- Protection against data races is delegated to the future concurrency model
  (`Send`/`Sync`, post-MVP); in the MVP there is a single thread.
