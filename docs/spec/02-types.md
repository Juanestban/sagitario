# 2. Types and values

## Static typing

Sagitario is statically typed with **local type inference**:

- Local bindings may omit their type: `const x = 5;`.
- Function and method signatures must be fully annotated (parameters and return type).
- `-> T` declares the return type; a function without `->` returns the unit type.

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

Enums are sum types; each variant may carry data, positionally or with named fields:

```sag
enum Shape {
  Circle(f64),
  Rect { w: f64, h: f64 },
}
```

Values are inspected with an exhaustive `match` (see [control flow](06-control-flow.md)).

## `Option<T>` and `Result<T, E>` (F1)

- There is **no `null`**. Absence is expressed only with `Option<T>`: `Some(x)` or `None`.
- There is no `T?` sugar and no `?.` / `??` operators. Use `match`, `?`, or library methods
  such as `unwrap_or`, `map` and `and_then`.
- `Result<T, E>` is `Ok(x)` or `Err(e)`. See [errors](07-errors.md).
