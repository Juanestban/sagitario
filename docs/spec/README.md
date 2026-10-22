# Sagitario Language Specification

This directory is the **normative** description of the Sagitario language: what is valid,
what it means, and what the compiler must reject. The reasons behind each decision live in
[`../adr/`](../adr/README.md); this specification only states the rules.

## Chapters

1. [Lexical structure](01-lexical.md)
2. [Types and values](02-types.md)
3. [Memory model and mutability](03-memory.md)
4. [Classes](04-classes.md)
5. [Interfaces and generics](05-interfaces-generics.md)
6. [Expressions and control flow](06-control-flow.md)
7. [Errors and panics](07-errors.md)
8. [Closures](08-closures.md)
9. [Modules and packages](09-modules.md)
10. [Foreign function interface](10-ffi.md)

## Conventions

- "must" / "must not" are requirements on programs or on the compiler; a program that
  violates a "must" is rejected with a diagnostic.
- Each chapter marks which milestone delivers each feature: **F1** (procedural core) or
  **F2** (OOP, generics, closures). Features marked **post-MVP** are reserved and must not
  be implemented yet. See [`../roadmap.md`](../roadmap.md).
- Code blocks use the `.sag` syntax and are illustrative unless stated otherwise.

## A complete example

```sag
use std::io;

const PI: f64 = 3.14159;

enum Shape {
  Circle(f64),
  Rect { w: f64, h: f64 },
}

interface Named {
  fn display_name(self) -> String;
  fn greet(self) -> String { "Hello, " + self.display_name() }
}

struct Point {
  pub x: f64;
  pub y: f64;

  pub fn len(self) -> f64 { (self.x * self.x + self.y * self.y).sqrt() }
  pub fn move_by(var self, dx: f64) { self.x += dx; }
}

abstract class Animal impl Named {
  prv const name: String;
  pub var age: i32;

  pub fn new(name: String, age: i32) {
    self.name = name;
    self.age = age;
  }

  pub fn display_name(self) -> String { self.name }
  pub abstract fn speak(self) -> String;
}

class Dog extends Animal {
  pub fn new(name: String) {
    zod.new(name, 0);
  }

  pub override fn speak(self) -> String { `${self.display_name()} says woof` }
}

fn area(s: Shape) -> f64 {
  match s {
    Shape::Circle(r) => PI * r * r,
    Shape::Rect { w, h } => w * h,
  }
}

pub fn main() -> Result<(), io::Error> {
  const dog = new Dog("Rex");
  const p = Point { x: 1.0, y: 2.0 };
  const doubled = [1, 2, 3, 4].map((x) => x * 2).filter((x) => x > 2);
  var total = 0;
  for n in doubled { total += n; }
  io::println(`total = ${total}, ${dog.speak()}, ${p.len()}`)?;
  Ok(())
}
```

## Open questions

These points are **not decided yet**. Do not implement a guess: resolve them (and record
an ADR when the choice is significant) before the feature that needs them.

| # | Question | Needed by |
|---|---|---|
| Q1 | Comment syntax. Examples use `//`; block comments (`/* */`) and doc comments are undecided. | F1 |
| Q2 | Attribute syntax. Both `@export("name")` and `#[repr(C)]` appeared during design; pick one form. | F1 |
| Q3 | Are visibility modifiers mandatory on `struct` members (as on class members)? Examples use `pub`. | F1 |
| Q4 | Range syntax for `for` loops (e.g. `0..10`, inclusive form). | F1 |
| Q5 | Fixed-size array type syntax vs. `List<T>`; type of the literal `[1, 2, 3]`. | F1 |
| Q6 | Numeric literal forms: hex/octal/binary prefixes, `_` separators, suffixes. | F1 |
| Q7 | Methods on `enum`s (there are no `impl` blocks) and how a `struct`/`enum` declares implemented interfaces. | F1/F2 |
| Q8 | Static/associated functions other than `new` (e.g. `Point::origin()`), and whether structs may declare `fn new`. | F1 |
| Q9 | A `Self` type and the unit type spelling (examples use `()`). | F1 |
| Q10 | How `?` converts between error types (a `From`-like interface or exact match only). | F1 |
| Q11 | Members of the `Error` interface. | F1 |
| Q12 | Syntax for declaring `weak` references. | F2 |
