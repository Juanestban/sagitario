# 4. Classes (F2)

## Declaration

```sag
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

const dog = new Dog("Rex");
```

- Fields and methods are declared inside the class body. There are no `impl` blocks.
- Methods take `self` explicitly as their first parameter; inside them, members are accessed
  through `self`.

## Visibility of members

- Every class member (field or method) **must** declare `pub` or `prv`.
- `pub`: visible wherever the class is visible.
- `prv`: visible only inside the declaring class. Subclasses cannot access it.
- There is no `protected` level in the MVP (open for reconsideration later).

## Construction

- The constructor is `pub fn new(...)` (or `prv fn new`). It is an **initializer**: `self`
  is implicit and already allocated, and `new` has no return type.
- Objects are created with `new ClassName(args)`, which allocates and calls `new`.
- The compiler verifies that every `const` field is assigned exactly once in `new` and that
  no field is read before it is assigned.
- In a subclass, `zod.new(...)` **must be the first statement** of `new` when the parent
  declares a constructor.
- `new` cannot be called as `ClassName::new(...)`. Other functions without `self` are static
  and are called as `ClassName::name(...)`:

```sag
class Dog extends Animal {
  pub fn new(name: String) { zod.new(name, 0); }
  pub fn puppy() -> Dog { new Dog("Puppy") }
}

const d = Dog::puppy();
```

## Inheritance

- `class A extends B`: single inheritance only. There is no multiple class inheritance.
- Every class can be inherited and every method can be overridden. There is no `open` and no
  `final` keyword.
- An overriding method **must** be marked `override`. Overriding without `override`, or
  marking `override` without a parent method, is an error.
- `zod.method(...)` calls the parent implementation.
- Dispatch: methods are virtual by default. The compiler sees the whole program and must
  devirtualize calls to methods that no subclass overrides.

## Abstract classes

- `abstract class` cannot be instantiated with `new`.
- `abstract fn` has no body and must be implemented (with `override`) by the first concrete
  subclass.
- A class with any abstract method must itself be `abstract`.

## Interfaces on classes

`class Dog extends Animal impl Named, Display { ... }`: `extends` names at most one class;
`impl` names one or more interfaces separated by commas. See
[interfaces](05-interfaces-generics.md).
