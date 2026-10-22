# 2. Types and values

## Static typing

Sagitario is statically typed with **local type inference**:

- Local bindings may omit their type: `const x = 5;`.
- Function and method signatures must be fully annotated (parameters and return type).
- `-> T` declares the return type; a function without `->` returns the unit type `()`.

## Unit type

`()` is both a type and its only value. It is the implicit return type of functions without
`->` and is used in `Result<(), E>`. There are no general tuples (`(i32, String)`) in the
MVP; they may be added later.

## `Self`

- Inside a `struct`, `enum` or `class` body, `Self` names the type being declared:
  `pub fn origin() -> Self { Self { x: 0.0, y: 0.0 } }`, `Self::Circle(1.0)`.
- Inside an `interface`, `Self` names the implementing type (`fn clone(self) -> Self;`).
- In a class, `Self` is resolved statically to the class where the code is written, not to
  the runtime subclass.
- An interface that uses `Self` in a parameter or return type cannot be used as a dynamic
  type (see [interfaces](05-interfaces-generics.md#interfaces-as-types)).

## Primitive types (F1)

| Type | Meaning |
|---|---|
| `i8` `i16` `i32` `i64` `isize` | Signed integers |
| `u8` `u16` `u32` `u64` `usize` | Unsigned integers |
| `f32` `f64` | IEEE-754 floats |
| `bool` | `true` / `false` |
| `char` | A Unicode scalar value (32 bits) |

## Conversions

- There are **no implicit numeric conversions**. `i32 + i64` is a compile error.
- Explicit conversion uses `as`: `x as i64`.
- `bool` never converts to or from integers. Conditions of `if` and `while` must have type
  `bool`; there is no "truthiness".

## Integer arithmetic

- Overflow **panics in debug builds** and **wraps in release builds**.
- The standard library provides explicit `wrapping_*`, `checked_*` and `saturating_*` methods.
- Integer division by zero always panics, in every build mode.
- The generated C must never rely on undefined behavior (see
  [architecture](../architecture.md#c-code-generation)).

## `String` (F1)

- Always valid UTF-8 and **immutable**.
- Managed by ARC: copying a `String` value only increments its reference count.
- `+` and template strings create a new `String`. `StringBuilder` builds strings efficiently.
- Byte indexing (`s[i]`) is not allowed; use `s.chars()` or `s.bytes()`.

## Equality (F1/F2)

- `==` is defined between primitives, between `String`s, and between data-less enum values.
- Between class instances, `==` is **identity** (same instance).
- Structs are compared with a method (e.g. `a.equals(b)`) until an `Eq` interface exists.
- Operator overloading is **post-MVP** (planned through interfaces such as `Add`, `Eq`, `Ord`).

## Enums (F1)

Enums are sum types. Variant syntax is exactly Rust's: each variant may carry data,
positionally or with named fields.

```sag
enum Shape impl Display {
  Circle(f64),
  Rect { w: f64, h: f64 },

  pub fn area(self) -> f64 {
    match self {
      Shape::Circle(r) => 3.14159 * r * r,
      Shape::Rect { w, h } => w * h,
    }
  }

  pub fn to_string(self) -> String { `area ${self.area()}` }
}
```

- Variants come first, separated by commas. They take no visibility modifier and are always
  as visible as the enum (so `match` can be exhaustive anywhere), as are their fields.
- Methods follow the variants inside the same body, with mandatory `pub`/`prv`, taking
  `self` or `var self`; functions without `self` are static (see
  [static functions](03-memory.md#static-functions)).
- `enum Name impl I1, I2` implements interfaces (F2).
- Enums are value types; their mutability comes from the binding, as for structs.
- Values are inspected with an exhaustive `match` (see [control flow](06-control-flow.md)).

## Arrays and lists

| | Fixed array `[T; N]` (F1) | `List<T>` (F2) |
|---|---|---|
| Size | Fixed, known at compile time | Growable |
| Semantics | **Value**, like a struct: copied on assignment, stored inline | **ARC reference**: a standard library class |
| Creation | `[1, 2, 3]` is `[i32; 3]`; `[0; 16]` is sixteen zeros | `new List<i32>()`, `List::from([1, 2, 3])` |
| Mutation | Only through a `var` binding | `push`, `pop`, `set` through any reference |
| Indexing | `a[i]`, bounds-checked (panics) | `list[i]`, bounds-checked |
| Length | `a.len()` | `list.len()` |

- The literal `[1, 2, 3]` is always a fixed array; lists are built explicitly.
- From F2, arrays expose iterators: `[1, 2, 3].map((x) => x * 2)` returns an iterator that can
  be consumed by `for` or gathered with `.collect()`.
- Slices (copy-free views such as `xs[1..3]`) are post-MVP.

## `Option<T>` and `Result<T, E>` (F1)

- There is **no `null`**. Absence is expressed only with `Option<T>`: `Some(x)` or `None`.
- There is no `T?` sugar and no `?.` / `??` operators. Use `match`, `?`, or library methods
  such as `unwrap_or`, `map` and `and_then`.
- `Result<T, E>` is `Ok(x)` or `Err(e)`. See [errors](07-errors.md).
