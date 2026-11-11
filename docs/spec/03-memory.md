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
  does not keep its target alive (see [classes](04-classes.md#weak-references)).
- When the last reference to an instance goes away, its fields are released; there are no
  destructors in the MVP. Chains of instances (a long linked list) are destroyed iteratively,
  without exhausting the stack.
- Class instances are compared with `==`/`!=` by identity (the same instance).
- Reference counting is **non-atomic**, except in programs using `std::thread` (F4), where
  every count is atomic ([ADR-0045](../adr/0045-threads-and-send.md)).

## Bindings: `const` and `var` (F1)

```sag
const x = 5;     // cannot be reassigned
var total = 0;   // can be reassigned
total += x;
```

- `const` at module level declares a constant the compiler computes
  ([below](#module-level-constants)).
- There is no `mut`, and `let` only appears in `if let` and `while let`
  ([control flow](06-control-flow.md#if-let-and-while-let)).
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

## Module-level constants

Rationale: [ADR-0049](../adr/0049-self-hosting-plan-and-language-prerequisites.md).

```sag
pub const MAX_USERS: i32 = 100;
const TIMEOUT_MS: u64 = 60 * 1_000;
const LIMIT: i64 = MAX_USERS as i64 * 2;
const GREETING: String = "hello, " + "world";
const RUNTIME: String = include_str("../runtime/runtime.c");
```

- A constant is an item: `pub` exports it, `use` imports it, and `limits::MAX_USERS` names it
  in full. Its type is always written, and is an integer, a float, `bool`, `char` or `String`.
- The compiler computes its value from literals, other constants, unary `-` and `!`, the
  arithmetic operators (`+` also joins strings), comparisons, `&&`, `||`, `as` between primitive
  types and `include_str`. A constant cannot need its own value.
- A value outside its type, an integer division by zero, a float literal too large for its type
  (`1e39` as `f32`), or any other expression (a call, a lambda, `new`) is an error.
- Float arithmetic follows IEEE-754: a constant may be infinite or NaN (`1.0 / 0.0`, `0.0 / 0.0`),
  and every comparison with NaN is false except `!=`
  ([ADR-0076](../adr/0076-std-math-and-methods-of-numbers.md)).
- `include_str("path")` is the text of a file, as a `String`. It is only valid in the value of
  a module-level constant; the path is a string literal relative to the directory of the
  source file, and the file must be UTF-8. The file is read when the program is compiled.

## Global state (G1)

Rationale: [ADR-0083](../adr/0083-global-state.md).

```sag
use std::sync::{AtomicI64, Mutex};

pub static WIDTH: i32 = 640;
static FRAMES: AtomicI64 = new AtomicI64(0);
static NAMES: Mutex<String> = new Mutex(load_names());

fn frame() {
  FRAMES.fetch_add(1);
  NAMES.update((names) => names + ",");
}
```

- `static NAME: Type = value;` at module level declares one value for the whole program, as
  Rust's `static`. Like a constant it is an item: `pub` exports it, `use` imports it, and
  `config::WIDTH` names it in full. Its type is always written.
- Every thread can read a static, so its type must be `Send`
  ([`Send`](05-interfaces-generics.md#send-f4)): primitives, `String`, structs and enums of `Send`
  values, classes whose fields are all `const` and `Send`, `Mutex<T>`, the atomics and `Channel<T>`.
  A `List`, a `Map` or a class with `var` fields is an error (`E0332`).
- A static cannot be assigned (`E0307`), nor its fields, nor can a method that modifies `self`
  be called on it (`E0312`, `E0313`). Its value changes only through its own methods, such as
  those of an atomic or a `Mutex`: `FRAMES.fetch_add(1)`.
- The value is any expression, computed **on the first use** of the static, exactly once even
  when several threads reach it at once: the others wait for it. A static that needs its own value
  while it is being computed, directly or through other statics or functions, panics naming it. A
  static computed from literals and constants, of a primitive type or `String`, keeps no state.
- Reading a static gives its value, as reading a `const` binding does: a struct is copied, and a
  class instance or `Mutex` is the same object every time. `&NAME` lends it, and `&raw const NAME`
  gives its address, which is the same for the whole program.
- Its value lives until the program ends and is never released while it runs.
- A static is not a constant: a module-level `const` cannot use one (`E0334`).
- `static var NAME: Type = value;` is a mutable global, as Rust's `static mut`: every thread
  and C can reach it at once, and the compiler cannot tell, so reading it, assigning it
  (`NAME = value`, `NAME += 1`) and changing its parts or calling its methods happen only inside
  `unsafe` (`E0323`). Its type needs not be `Send`. Its value is computed as a static's.
- As in Rust 2024, `&NAME` and `&mut NAME` of a `static var`, or of a part of one, are errors
  (`E0340`): it could change while lent. Copy its value in `unsafe`, or take its address with
  `&raw const NAME` or `&raw mut NAME`, which is safe; using the pointer is not
  ([raw pointers](10-ffi.md#raw-pointers)).

```sag
use std::ffi::c_int;

static var COMPARISONS: i32 = 0;

/// Called by C's `qsort`, which passes no context.
extern "C" fn by_value(a: &i32, b: &i32) -> c_int {
  unsafe { COMPARISONS += 1; }
  *a - *b
}
```

- `@thread_local static var NAME: Type = value;` gives each thread its own value, as Rust's
  `thread_local!`: it is computed on the first use in each thread, as a static's is in the
  program, and read, assigned and changed without `unsafe`, since no other thread sees it. Its
  type needs not be `Send`. A `var` takes the role of Rust's `Cell` and `RefCell`.
- A thread's value is released when the thread ends, and the main thread's when the program
  ends. A thread that C started, outside `std::thread`, does not release its values.
- `&NAME` and `&mut NAME` of a `@thread_local static var`, or of a part of one, are errors
  (`E0340`): any function the thread calls could change it while lent. Its methods are called
  on it in place: `SCRATCH.push(x)`. `&raw const NAME` gives the address of the calling thread's
  value.
- `@thread_local` applies only to `static var` (`E0105`): a `static` never changes, so one
  value serves every thread.

```sag
/// One per thread, used without locks.
@thread_local
static var SCRATCH: List<f32> = new List();

fn frame() {
  SCRATCH.clear();
  SCRATCH.push(0.5);
}
```

## Struct mutability (F1)

- Struct fields do **not** take `const`/`var`. A struct's mutability comes from the binding
  that holds it: everything inside a `const` struct is immutable; a `var` struct is mutable.
- Methods take `self`, never `var self` (ADR-0028). Every method may modify `self`: assign its
  fields and elements, call methods that modify it, or replace it (`self = Point::origin();`).
- The compiler infers which methods **modify `self`**: those that do any of the above to `self`
  or a part of it, directly or through the methods they call on it. They receive the caller's
  value; the others receive a copy.
- A field or array element can be assigned (`p.x = v`, `a[i] += 1`, `p.pos.x = 0`), and a
  method that modifies `self` can be called on it, only if it belongs to a `var` binding, to
  `self`, or to a `var` field of a class instance. `const` bindings, parameters, loop variables,
  pattern bindings and temporaries (`Point::new(0.0).move_by(1.0)`) cannot be modified.
- A `var` modified only through its fields, elements or methods that modify `self` counts as
  reassigned for the unneeded-`var` warning.

```sag
struct Point {
  pub x: f64;
  pub fn move_by(self, dx: f64) { self.x += dx; }   // modifies `self`
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
- A struct literal initializes every field exactly once, in any order; there are no default
  values and no `..base` syntax. `Point { x, y }` is shorthand for `Point { x: x, y: y }`.
- Fields and functions of a struct share one namespace: a field and a method cannot have the
  same name.
- Structs are copied when assigned, passed or returned; copying retains the `String`s they
  contain. There is no move.
- A struct cannot contain itself by value, directly or through other structs or arrays: its
  size would be infinite. Recursive data needs a reference type (`class`, F2).
- Structs are compared with `==` and `<` only when they implement `Eq` and `Ord`, and cannot be
  interpolated (see [types](02-types.md#equality-f1f2)).
- `struct Name impl I1, I2` implements interfaces (F2). Structs never use `extends`. Generic
  structs arrive with generics (F2).

## Static functions (F1)

- A function declared inside a `struct`, `enum` or `class` without a `self` parameter is
  **static** and is called as `Type::name(...)`. The keyword `static` declares module-level
  [statics](#global-state-g1), not members of types.
- Methods are called on a value, `p.len()`; static functions only through the type,
  `Point::origin()`. Calling a static function as `p.origin()` or a method as `Point::len(p)`
  is an error.
- In structs and enums, `new` is an ordinary static function by convention (it returns the
  type and is called `Point::new(...)`). `new` is a keyword, but it may name a struct or enum
  function.
- In classes, `new` is the special initializer, declared `fn new(self, ...)` (see
  [classes](04-classes.md#construction)), and can only be invoked through
  `new ClassName(...)`, never as `ClassName::new(...)`.
- Types have no static fields: a module holds [statics](#global-state-g1) instead, as in Rust.

## Class field mutability (F2)

- Every class field is declared `const` (immutable after construction) or `var` (mutable).
  Struct fields take neither.
- A `var` field can be modified through **any** reference to the object, including a `const`
  binding or a parameter: the binding holds the reference, not the object. The same holds for
  the parts of a `var` field (`obj.pos.x = 1`, `obj.pos.move_by(1.0)`).
- `const` fields must be assigned exactly once in the initializer, before use; afterwards
  neither they nor their parts can change.
- A `var` binding holding an instance counts as reassigned only when the binding itself is
  assigned, not when the instance's fields change.
- Data races are prevented by `Send` (F4, [interfaces](05-interfaces-generics.md#send-f4)):
  a thread can only receive values that no other thread can change, so a class instance with
  `var` fields never reaches two threads.

## References

References ([ADR-0079](../adr/0079-references-and-borrowing.md), increment 1) let a function read
or change a value of its caller without copying it, as in Rust. They live while a call does, so
they need no lifetimes, and ARC, classes and closures keep their rules.

```sag
fn length(v: &Vec2) -> f32 { (v.x * v.x + v.y * v.y).sqrt() }
fn scale(v: &mut Vec2, by: f32) { v.x *= by; v.y *= by; }
fn swap(a: &mut i32, b: &mut i32) { const kept = *a; *a = *b; *b = kept; }

var position = Vec2 { x: 3.0, y: 4.0 };
scale(&mut position, 2.0);
println(`${length(&position)}`);
```

- `&T` and `&mut T` are only the **types of parameters**: of functions, methods, initializers,
  `extern "C" fn` items, lambdas and function types (`fn(&T)`). A field, result, binding, element
  or type argument cannot be a reference (`List<&T>`, `&&T`): an error (`E0339`).
- The caller writes the loan: `f(&x)` lends `x`, `f(&mut x)` lends it to be changed. A value is
  never lent implicitly. `&` of a value that is not a place (`&5`, `&f()`, `&Vec2 { ... }`) lends
  a temporary that lives until the call returns. A reference can only be passed to a parameter of
  a reference type (or of a raw pointer type, for C): storing, returning or otherwise using a
  `&x` or a reference parameter as a value is an error (`E0339`). A type argument is never
  inferred to be a reference.
- **Places.** `&` and `&mut` lend a binding or parameter, a field of a struct, an element of an
  array, `*reference`, `*pointer`, or a field of a class instance whose type holds no ARC values
  (numbers, `bool`, raw pointers, and structs and arrays of those): the instance is kept alive
  until the call returns. An element of a list and a field holding ARC values reached through a
  class instance cannot be lent, since other references could make the list grow or replace the
  field (`E0340`): copy them into a binding first. `&mut` needs a place that can be modified: of
  a `var` binding, of a `&mut` parameter, or a `var` field of a class instance (`E0312`);
  `&mut` of a temporary is an error (`E0340`).
- **Inside the callee.** Fields and methods are reached through a reference directly (`v.x`,
  `v.length()`, `v[i]` for an array); `*v` reads the whole value (a copy) and `*v = value`
  replaces it, both safe. Through a `&mut T`, fields can be assigned and methods that modify
  `self` called; through a `&T`, the value cannot change (`E0312`, `E0313`). A reference to a
  class instance or a list lends the binding that holds the reference: `&mut` lets the callee
  replace the instance (`*player = new Player(...)`); the `var` fields of an instance can be
  changed through either kind, as through any `const` binding.
- A reference parameter is passed on as it is (`g(v)`), and a `&mut T` where a `&T` is expected.
  `&v` of a reference parameter lends what it refers to again. Operators do not reach through a
  reference: write `*a + *b`, `*v == w`, `` `${*name}` ``.
- **Exclusivity.** In one call, a place lent with `&mut` cannot also be lent with `&` or `&mut`,
  or be passed by value, unless the places are disjoint: different fields of the same struct, as
  in Rust (`f(&mut p.first, &p.second)` is fine). Two elements of an array are not disjoint:
  `f(&mut a[i], &a[j])` is an error (`E0341`). The receiver of a method that modifies `self`
  cannot be lent to the same call either (`v.add(&v)`); its arguments by value are copied before
  the call, so they may read from it. Places reached through class instances are not checked, as
  with C#'s `ref`: other references may reach the same instance, but aliasing only shows changes
  and never frees memory in use.
- **Escaping.** A lambda cannot capture a reference parameter, and an `async fn` cannot take one,
  since both outlive the call (`E0339`); copy the value first (`const copy = *v;`).
- [Slices](02-types.md#slices), `&[T]` and `&mut [T]`, follow the same rules: views of a run of
  elements of an array, a list or a slice.
- In C, a `&T` is a `const T *` and a `&mut T` a `T *`, never null: lending costs nothing at run
  time, except keeping a lent class instance alive, and counting the slices of a list. References meet C as raw pointers
  ([FFI](10-ffi.md#raw-pointers)).
