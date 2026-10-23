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

- Fields and methods are declared inside the class body. There are no `impl` blocks. Every
  field is `const` or `var` (see [mutability](03-memory.md#class-field-mutability-f2)).
- Methods take `self` explicitly as their first parameter, never `var self`; inside them,
  members are accessed through `self`. `self` is a reference to the instance: methods can
  modify its `var` fields, and `self` itself cannot be reassigned.
- Fields and functions of a class share one namespace.

## Visibility of members

- Every class member (field or method) **must** declare `pub` or `prv`.
- `pub`: visible wherever the class is visible.
- `prv`: visible only inside the declaring class. Subclasses cannot access it.
- There is no `protected` level in the MVP (open for reconsideration later).

## Construction

- The constructor is `pub fn new(...)` (or `prv fn new`). It is an **initializer**: `self`
  is implicit and already allocated, and `new` declares neither `self` nor a return type.
- Objects are created with `new ClassName(args)`, which allocates and calls `new`. A `prv`
  initializer can only be used inside the class.
- A class without fields may omit `new`; it then has an implicit `new()` without parameters. A
  class with fields must declare it. Fields have no initializers in their declaration.
- Every field, `const` and `var`, must be assigned before `new` returns: at the end of its body
  and at every `return`. A `const` field is assigned exactly once. No field is read before it is
  assigned, and `self` cannot be used otherwise (calling its methods, passing it along) until
  every field is assigned.
- In a subclass, `zod.new(...)` **must be the first statement** of `new` when an ancestor has
  an initializer; it runs the nearest one, after which the inherited fields count as assigned.
  It is an error anywhere else. A subclass without fields of its own may omit `new` when that
  inherited initializer takes no arguments; it then runs implicitly. Initializers are not
  inherited: `new Dog(...)` always runs `Dog`'s.
- While a parent's initializer runs, the instance behaves as the parent: methods called on
  `self` (directly or through other code) run the parent's implementations, never an
  `override` of the subclass, whose fields are not assigned yet. Calling an abstract method at
  that point panics.
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
- A subclass inherits the fields and methods of its ancestors; `prv` ones stay accessible only
  inside the class that declares them. Static functions are not inherited: `Animal::create()`
  is never `Dog::create()`. A class cannot redeclare a name it inherits, except to override a
  public method.
- An overriding method **must** be marked `override`. Overriding without `override`, or
  marking `override` without a parent method, is an error. The override has exactly the same
  parameters and return type, and is `pub`; `prv` methods cannot be overridden.
- `zod.method(...)` calls the parent implementation directly. `zod` exists only in the methods
  and initializer of a subclass, as the receiver of `zod.method(...)` or `zod.new(...)`.
- Only classes can be extended, and a chain of `extends` cannot loop. A class may extend a
  class of another module if it is visible.
- Dispatch: methods are virtual by default. The compiler sees the whole program and must
  devirtualize calls to methods that no subclass overrides.

## Subtyping

- An instance can be used wherever one of its ancestors is expected: as an argument, in an
  assignment, as a result, in a field, and as an element of an array literal or the value of
  `Some(...)` whose type is known (`const animals: [Animal; 2] = [dog, cat];`).
- The branches of an `if` or `match` take the expected type, or the ancestor when one
  branch's class is an ancestor of the other's.
- Containers have no variance: an existing `Option<Dog>` or `[Dog; 2]` is not an
  `Option<Animal>` or `[Animal; 2]`.
- `==`/`!=` compare instances of related classes by identity. There is no downcast in the
  MVP.

## Values and destruction

- Class values are references: assigning or passing one shares the instance (ARC).
- `==` and `!=` compare identity. Classes cannot be interpolated or compared with `<`.
- Instances are destroyed when their last strong reference goes away (see
  [memory](03-memory.md#automatic-reference-counting-f2)). There are no destructors in the MVP.

## Weak references

```sag
class Person {
  pub weak var team: Option<Team>;
}
```

- `weak` marks a `var` field of a class with type `Option<C>`, where `C` is a class (or, from
  F2 increment 4, an interface). It does not keep the instance alive. Struct fields, `const`
  fields and other types cannot be `weak`.
- Like every field, a `weak` field is assigned in `new` (usually `None`).
- Reading the field gives a strong `Option<C>`: `None` once the instance has been destroyed.
  Assigning `Some(x)` makes the field refer weakly to `x`.
- An instance's fields are released when its last strong reference goes away; its memory is
  freed once no weak reference to it remains.
- There are no `weak` local bindings in the MVP, and no warnings about cycles between class
  fields. `weak` captures in closures are post-MVP.

## Abstract classes

- `abstract class` cannot be instantiated with `new`, but it may declare `new` for its
  subclasses (`zod.new(...)`) and it can be used as a type.
- `abstract fn` has no body (`pub abstract fn speak(self) -> String;`), is a public method, and
  must be implemented (with `override`) by the first concrete subclass.
- A class with any abstract method must itself be `abstract`.

## Interfaces on classes

`class Dog extends Animal impl Named, Display { ... }`: `extends` names at most one class;
`impl` names one or more interfaces separated by commas. See
[interfaces](05-interfaces-generics.md).
