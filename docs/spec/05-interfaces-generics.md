# 5. Interfaces and generics (F2)

## Interfaces

```sag
interface Named {
  fn display_name(self) -> String;                                // required
  fn greet(self) -> String { "Hello, " + self.display_name() }   // default method
}

interface Pet extends Named {
  fn owner(self) -> String;
}
```

- An interface declares method signatures and may provide default implementations.
- `interface A extends B, C` composes interfaces.
- Both `class` and `struct` can implement interfaces. Classes use
  `class X impl I1, I2`; the struct syntax is open question Q7.
- A type implementing an interface must provide every method without a default.

## Interfaces as types

An interface can be used as a type (`const shapes: List<Shape> = ...`). Such values are
references with a vtable and use **dynamic dispatch**, like interface types in Java/C# or
`dyn Trait` in Rust, but without extra syntax.

## Generics

```sag
fn max<T: Comparable>(a: T, b: T) -> T { ... }

fn describe<T, U>(a: T, b: U) -> String
where
  T: Named + Display,
  U: Display,
{ ... }

class Box<T> {
  pub const value: T;
  pub fn new(value: T) { self.value = value; }
}
```

- Type parameters go in `<...>` on functions, methods, classes, structs, enums and interfaces.
- Bounds use `T: I1 + I2`. Long bounds may move to a `where` clause.
- Generics are **monomorphized**: each concrete instantiation produces its own specialized
  code. There is no boxing and no runtime type information for generics.
- Not in the MVP: higher-kinded types, specialization, variadic generics.

## Standard generic types (F2)

`List<T>` and `Map<K, V>`, with iterators supporting `map` and `filter`.
