# 5. Interfaces and generics (F2)

## Interfaces

Rationale: [ADR-0031](../adr/0031-interfaces.md).

```sag
interface Named {
  var name: String;                                                // required field
  fn display_name(self) -> String;                                 // required method
  fn greet(self) -> String { `Hello, ${self.display_name()}` }    // default method
}

interface Pet extends Named {
  fn owner(self) -> String;
}

class Dog impl Pet {
  pub var name: String;

  pub fn new(self, name: String) { self.name = name; }
  pub fn display_name(self) -> String { self.name }
  pub fn owner(self) -> String { "Ada" }
}
```

- Like a TypeScript interface, an interface describes the fields and methods a type must have.
  Its members take no `pub`/`prv`: they are public.
- **Fields** are declared `const` (they can only be read through the interface) or `var` (they
  can also be assigned through it).
- **Methods** take `self`. A method without body is required. A method with a body is a
  default, used by implementing types that do not define it. There are no static functions in
  interfaces.
- `interface A extends B, C` composes interfaces; `extends` cannot loop, and an interface cannot
  redeclare a method or field of a parent.
- `class`, `struct` and `enum` implement interfaces with the same form:
  `class A extends B impl I1, I2`, `struct P impl I1`, `enum E impl I1`.
- A type implementing an interface (and its ancestors) provides:
  - for each field, a `pub` field with the same name and type; in a class, a `var` field when
    the interface's is `var`. Enums have no fields, so they only implement interfaces without
    fields.
  - for each method without a default, a `pub fn` with the same name and signature, with
    `Self` replaced by the type. Defining a method that has a default replaces the default.
    These methods do not take `override`, which is for class inheritance.
- A subclass inherits its parent's interfaces, and inherited fields and methods count. An
  `abstract` class may leave required methods to its concrete subclasses.
- Inside an interface, `Self` names the implementing type:

```sag
interface Clone {
  fn clone(self) -> Self;
}
```

- In a default method, `self` has type `Self`: the implementing type, bounded by the interface,
  whose fields and methods it reaches. Default methods are compiled for each implementing type.

## Interfaces as types

An interface can be used as a type (`const shapes: [Shape; 2] = [square, circle];`). Such values
are references with their type's methods and use **dynamic dispatch**, like interface types in
Java/C# or `dyn Trait` in Rust, but without extra syntax.

- A class, struct or enum converts implicitly to an interface it implements, and an interface
  to its ancestors, wherever that type is expected (arguments, assignments, results, fields,
  elements, `Some(...)`, the branches of `if` and `match`). There is no conversion back.
- A class instance becomes an interface value as a new reference to the same instance. A struct
  or enum is **copied** into a box: the interface value refers to the copy.
- Interface values are references: copying one shares the instance or box, and assigning a
  `var` field through it changes that instance or box.
- Interface values cannot be compared with `==` in the MVP.
- An interface that uses `Self` in a parameter, result or field, itself or through a parent, or
  that has methods with type parameters, cannot be used as a type (e.g. `[Clone; 2]` is an
  error): the value could not know the concrete type or the instances. It can still be a bound.

## The `Error` interface (F2)

```sag
interface Error {
  fn message(self) -> String;
}
```

- `Error` is in the prelude. Error types implement it (`struct ParseError impl Error`).
- `Error("text")` creates a basic error whose `message()` is `text`:
  `return Err(Error("file not found"));` in a function returning `Result<T, Error>`.
- `Result<T, E>` admits any error type, `String` included, but `String` does not implement
  `Error`. See [errors](07-errors.md#the--operator) for how `?` converts errors.
- Post-MVP: `source()` for error chains, backtraces, and a separate `Display` interface
  designed together with interpolation of user types.

## Generics

```sag
fn max<T: Ord>(a: T, b: T) -> T { ... }

fn describe<T, U>(a: T, b: U) -> String
where
  T: Named + Display,
  U: Display,
{ ... }

class Box<T> {
  pub const value: T;
  pub fn new(self, value: T) { self.value = value; }
}
```

Rationale: [ADR-0032](../adr/0032-generics.md).

- Type parameters go in `<...>` on functions, methods, classes, structs, enums and interfaces.
- Bounds are interfaces: `T: I1 + I2`. Long bounds may move to a `where` clause, whose bounds
  join those after `:`.
- **Type arguments** are written TypeScript-style after a name: `max<i32>(a, b)`,
  `new Box<i32>(1)`, `Pair<i32, String>::new(1, "a")`, `Maybe<String>::Nothing`, and in types
  `Box<i32>`. A generic type always takes its arguments in a type (`Box` alone is an error).
- **Inference** is local: type arguments omitted in a call, `new` or literal come from the
  arguments (or fields) and the expected type: `const b: Box<i32> = new Box(1);`. If one stays
  unknown, the compiler asks for it. Inside a generic type, `Self::f()` keeps the type's own
  arguments.
- Generic code is checked once, where it is written. Without bounds a `T` can be stored,
  copied, passed, returned and put inside other types. Its bounds add their methods and fields.
  There is no `==`, interpolation or arithmetic on an unbounded `T`.
- A type argument must implement the bounds of its parameter. Classes, structs and enums
  implementing an interface satisfy it, and so do the interface type itself and type parameters
  bounded by it. Integers, `bool`, `char` and `String` implement the prelude's `Eq`, `Ord` and
  `Hash`, and no other interface.
- In a call, a lambda argument whose parameter types are known infers type parameters from its
  result: `iter.map(n => n * 2)` infers the `U` of `map<U>(f: fn(T) -> U)`.
- Generic interfaces (`interface Source<T>`) are implemented once per type and, instantiated,
  can be types (`Source<i32>`). Generic classes extend and are extended with arguments
  (`class IntBox extends Box<i32>`). There is no variance: `Box<Dog>` is not a `Box<Animal>`.
- Methods with their own type parameters cannot be overridden or be abstract.
- Generics are **monomorphized**: each concrete instantiation produces its own specialized
  code. There is no boxing and no runtime type information for generics. Generic code whose
  instances would keep growing without end is an error.
- Not in the MVP: higher-kinded types, specialization, variadic generics, default type
  arguments, constant generics.

## `Send` (F4)

Rationale: [ADR-0045](../adr/0045-threads-and-send.md).

`Send` is the prelude's interface of the values that can cross to another thread: those no
other thread can change. Sagitario has no move, so a value given to a thread is shared with
the thread that gave it; `Send` is the only marker, and `Sync` stays reserved.

- The compiler infers it; no type names it after `impl`, no interface extends it, and it is not
  a type (`const x: Send` is an error). It is a bound: `fn f<T: Send>(value: T)`.
- These types are `Send`: integers, floats, `bool`, `char`, `()`, `String`, `CString`; arrays,
  `Option` and `Result` of `Send` types; structs and enums whose fields are all `Send`; type
  parameters bounded by `Send`; and classes whose fields, their own and inherited, are all
  `const` and `Send`, if every subclass is `Send` as well (a reference to a class can refer to
  an instance of a subclass).
- These are not: `List`, `Map` and other classes with a `var` or `weak` field, `StringBuilder`,
  function values, raw pointers and interface values.
- Only `thread::spawn` asks for `Send` values today
  ([threads](11-standard-library.md#stdthread)).

## Equality, order and hashing (F2)

Rationale: [ADR-0034](../adr/0034-collections-and-iterators.md).

```sag
enum Ordering { Less, Equal, Greater }

interface Eq {
  fn equals(self, other: Self) -> bool;
}

interface Ord extends Eq {
  fn compare(self, other: Self) -> Ordering;
}

interface Hash extends Eq {
  fn hash(self) -> u64;
}
```

- The four are in the prelude. Integers, `bool`, `char` and `String` implement the three
  interfaces, with their usual equality and order (strings by bytes): `5.compare(3)` is
  `Ordering::Greater`.
- Floats implement none of them, since `NaN` is not equal to itself. `x.total_compare(y)`
  orders them by the IEEE 754 total order (`-NaN < -inf < ... < -0.0 < 0.0 < ... < inf < NaN`).
- Programs implement them like any interface: `struct Version impl Ord` with `equals` and
  `compare`. Equal values must have the same `hash`.
- `==` keeps its meaning (identity for class instances); generic code writes `a.equals(b)`.
- `List::contains`, `List::index_of` and `List::sort` need `Eq` or `Ord`, and the keys of a
  `Map` need `Hash`.

## Standard generic types (F2)

`List<T>` (see [lists](02-types.md#lists-f2)), `Map<K, V>` (see [maps](02-types.md#maps-f2)) and
`Iter<T>` (see [iterators](08-closures.md#iterators)).
