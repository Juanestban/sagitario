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
- Explicit conversion uses `as`: `x as i64`. `as` never panics, in any build mode.
- Conditions of `if` and `while` must have type `bool`; there is no "truthiness".

| Conversion | Result |
|---|---|
| integer → narrower integer | truncates to the low bits: `300 as u8` is `44` |
| signed ↔ unsigned of the same size | reinterprets the bits: `-1i32 as u32` is `4294967295` |
| integer → float | rounds to nearest |
| float → integer | **saturates**: truncates toward zero, clamps out-of-range values to the type's min/max, `NaN` becomes `0` |
| `f64` ↔ `f32` | rounds to nearest (values beyond `f32` range become infinity) |
| `bool` → integer | `true` is `1`, `false` is `0` |
| `char` → integer | the Unicode scalar value (`'A' as u32` is `65`) |
| `u8` → `char` | allowed (always a valid character); no other integer converts to `char` with `as` |
| anything else | compile error (e.g. `i32 as bool`; write `x != 0`) |

- `char::from_u32(code: u32) -> Option<char>` is the character with that code point, or `None`
  for a surrogate (`0xD800`–`0xDFFF`) or a value above `0x10FFFF`
  ([ADR-0050](../adr/0050-lexer-port-and-token-dumps.md)).

## Build modes

| | Debug (default) | Release (`--release`) |
|---|---|---|
| Integer overflow | panics | wraps |
| C optimization | `-O0 -g` | `-O2` |

Division by zero, out-of-bounds indexing and every other panic behave the same in both modes.

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
- Template strings interpolate any expression with `${expr}`:
  `` `total = ${a + b * 2}` ``. Interpolable types and their format:

  | Type | Format |
  |---|---|
  | integers | decimal (`-42`) |
  | `f32`, `f64` | the shortest decimal that converts back to the same value, without exponent: `1.5`, `2` (for `2.0`), `0.30000000000000004`; `NaN`, `inf`, `-inf` |
  | `bool` | `true` / `false` |
  | `char` | the character |
  | `String` | as is |

  User-defined types (structs, enums, classes) cannot be interpolated until a `Display`
  interface exists (post-MVP).
- Byte indexing (`s[i]`) is not allowed; use `s.chars()` or `s.bytes()`.
- `a + b` creates a new `String`; `s += t` is `s = s + t`. `==`, `!=`, `<`, `<=`, `>`, `>=`
  compare the bytes lexicographically.
- Methods (F1). Indices and lengths count **bytes**; an index outside the string or inside a
  UTF-8 character panics (`byte index 2 is not a char boundary`):

  | Method | Result |
  |---|---|
  | `len()`, `is_empty()` | `usize` bytes, `bool` |
  | `contains(p)`, `starts_with(p)`, `ends_with(p)` | `bool` |
  | `find(p)` | `Option<usize>`: byte index of the first occurrence |
  | `trim()`, `trim_start()`, `trim_end()` | `String` without leading/trailing Unicode whitespace |
  | `to_ascii_uppercase()`, `to_ascii_lowercase()` | `String`; only ASCII letters change |
  | `replace(from, to)`, `repeat(n)`, `substring(start, end)` | `String` |
  | `parse_i32()` (every integer type), `parse_f32()`, `parse_f64()` | `Result<T, String>` with Rust's messages (`invalid digit found in string`) |

- `for c in s.chars()` iterates the characters (`char`) and `for b in s.bytes()` the bytes
  (`u8`) of the string as it was before the loop. Elsewhere they are iterators (F2):
  `Iter<char>` and `Iter<u8>` (see [iterators](08-closures.md#iterators)).
- `split(sep)` (F2) gives a `List<String>` with the parts between occurrences of `sep`; an
  empty `sep` splits into characters (`"héy".split("")` is `h`, `é`, `y`). `lines()` (F2) gives
  the lines without their `\n` or `\r\n`; a final line break does not start a new line.
- `char` methods (ASCII): `is_ascii_digit()`, `is_ascii_alphabetic()`,
  `is_ascii_alphanumeric()`, `is_ascii_whitespace()`, `is_ascii_uppercase()`,
  `is_ascii_lowercase()`, `to_ascii_uppercase()`, `to_ascii_lowercase()`, and
  `to_digit(radix) -> Option<u32>` (panics unless `radix` is 2 to 36).

## `StringBuilder` (F1)

```sag
const builder = new StringBuilder();
builder.append("total: ");
builder.append_char('=');
const text = builder.to_string();
```

- A reference type managed by ARC (a standard library class from F2): copies share the same
  builder, and it can be modified through a `const` binding. `new StringBuilder()` is the only
  `new` expression in F1.
- Methods: `append(s: String)`, `append_char(c: char)`, `len()` (bytes), `is_empty()`,
  `clear()`, `to_string()` (a new `String`; the builder stays usable).
- It cannot be compared with `==` or interpolated.

## Equality (F1/F2)

- `==` is defined between primitives, between `String`s, and between data-less enum values.
- Between class instances, `==` is **identity** (same instance).
- Structs, enums with data and classes compare by value through the prelude interfaces `Eq`
  and `Ord` (F2, [interfaces](05-interfaces-generics.md#equality-order-and-hashing-f2)), which
  integers, `bool`, `char` and `String` also implement: `a.equals(b)`, `a.compare(b)`.
- Arrays, lists, maps, iterators, interface values and function values cannot be compared with
  `==`.
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
  `self`; functions without `self` are static (see
  [static functions](03-memory.md#static-functions)). A method may replace `self`
  (`self = Shape::Empty;`), which makes it modify the caller's value
  (see [struct mutability](03-memory.md#struct-mutability-f1)).
- A variant without data is written without parentheses (`Empty`): `Empty()`, `Empty {}` and
  enums without variants are errors. Variants and functions of an enum share one namespace.
- Variants are always named with their path: `Shape::Circle(1.0)`, `Shape::Rect { w, h }`,
  `Shape::Empty`, or `Self::Empty` inside the enum. Only the prelude's `Some`, `None`, `Ok` and
  `Err` are used without one. Named fields are initialized like a struct literal (every field
  once, shorthand allowed).
- `enum Name impl I1, I2` implements interfaces (F2). Generic enums arrive with generics (F2).
- Enums are value types; their mutability comes from the binding, as for structs. Copying an
  enum retains the `String`s it holds. An enum cannot contain itself by value.
- Only enums whose variants carry no data support `==` and `!=`. Enums cannot be converted with
  `as` or interpolated.
- Values are inspected with an exhaustive `match` (see [control flow](06-control-flow.md#match)).

## Arrays and lists

| | Fixed array `[T; N]` (F1) | `List<T>` (F2) |
|---|---|---|
| Size | Fixed, known at compile time | Growable |
| Semantics | **Value**, like a struct: copied on assignment, stored inline | **ARC reference**, a prelude type: copies share the list |
| Creation | `[1, 2, 3]` is `[i32; 3]`; `[0; 16]` is sixteen zeros | `new List<i32>()`, `List::from([1, 2, 3])` |
| Mutation | Only through a `var` binding | Through any reference: `push`, `list[i] = v`, ... |
| Indexing | `a[i]`, bounds-checked (panics) | `list[i]`, bounds-checked (panics) |
| Length | `a.len()` | `list.len()` |

- The literal `[1, 2, 3]` is always a fixed array; lists are built explicitly.
- All elements have the same type; unsuffixed literals take the element type from context
  (`const a: [u8; 3] = [1, 2, 3];`). An empty literal `[]` needs a type from context.
- In F1 the length `N` of `[T; N]` and `[value; N]` is an integer literal of type `usize`;
  `[value; N]` holds N copies of `value`.
- Indices have type `usize` and `a.len()` returns `usize`. An index out of bounds panics in
  every build mode: `index out of bounds: the len is 3 but the index is 5`.
- Arrays are copied when assigned, passed or returned (copying retains the `String`s inside).
  They are not compared with `==` and cannot be interpolated.
- `for x in a` iterates over a copy of `a` taken before the first iteration (see
  [control flow](06-control-flow.md#loops)).
- From F2, arrays expose iterators: `[1, 2, 3].map((x) => x * 2)` returns an iterator that can
  be consumed by `for` or gathered with `.collect()`. `a.iter()` and the methods of
  [iterators](08-closures.md#iterators) work on a list holding copies of the elements.
- Slices (copy-free views such as `xs[1..3]`) are post-MVP.

### Lists (F2)

Rationale: [ADR-0034](../adr/0034-collections-and-iterators.md).

- The element type of `new List<T>()` is written or comes from the expected type
  (`const names: List<String> = new List();`). `List::from(array)` takes it from the array.
- Reading `list[i]` gives a copy of the element (structs are copied, references retained).
  `list[i] = value` replaces it through any binding, `const` included, since the list is a
  reference. A part of an element cannot be assigned (`list[i].x = 1` is an error): modify a
  copy and store it back. Fields of class instances in a list can be assigned
  (`list[i].count += 1`), since the instance is shared.
- Methods:

  | Method | Result |
  |---|---|
  | `len()`, `is_empty()` | `usize`, `bool` |
  | `push(v)` | `()`: adds `v` at the end |
  | `pop()` | `Option<T>`: removes the last element |
  | `get(i)`, `first()`, `last()` | `Option<T>`: a copy, or `None` |
  | `insert(i, v)` | `()`: moves the elements from `i` on; panics if `i > len()` |
  | `remove(i)` | `T`: removes it, moving the following ones; panics if `i >= len()` |
  | `clear()`, `reverse()` | `()` |
  | `contains(v)`, `index_of(v)` | `bool`, `Option<usize>`; need `T: Eq` |
  | `sort()` | `()`: stable, ascending; needs `T: Ord` |
  | `sort_by(f)` | `()`: stable, by `f: fn(T, T) -> Ordering` |
  | `join(sep)` | `String`; only on `List<String>` |
  | `iter()` and the methods of `Iter<T>` | see [iterators](08-closures.md#iterators) |

- `for x in list` visits the elements by position and reads the length at every step:
  elements added or removed by the body are seen, and the loop never reads out of bounds.
- Lists are not compared with `==` and cannot be interpolated.

### Maps (F2)

```sag
const ages = new Map<String, i32>();
ages.insert("Ada", 36);
const age = ages.get("Ada").unwrap_or(0);
for entry in ages {
  println(`${entry.key}: ${entry.value}`);
}
```

- `Map<K, V>` is a prelude class: a hash map whose keys implement `Hash` (and so `Eq`). It
  keeps the order in which keys were first inserted.
- Methods: `insert(k, v) -> Option<V>` (the previous value), `get(k) -> Option<V>` (a copy),
  `contains_key(k) -> bool`, `remove(k) -> Option<V>`, `len()`, `is_empty()`, `clear()`, and the
  iterators `keys()`, `values()` and `entries()`. There is no `map[k]`.
- `for entry in map` iterates over `entries()`: values of the prelude struct
  `Entry<K, V> { key: K, value: V }`.
- Maps are not compared with `==` and cannot be interpolated.

## `Option<T>` and `Result<T, E>` (F1)

- There is **no `null`**. Absence is expressed only with `Option<T>`: `Some(x)` or `None`.
- There is no `T?` sugar and no `?.` / `??` operators. Use `match`, `?`, or library methods
  such as `unwrap_or`, `map` and `and_then`.
- `Result<T, E>` is `Ok(x)` or `Err(e)`. See [errors](07-errors.md).
- Until user generics exist (F2), `Option` and `Result` are the only generic types; the compiler
  instantiates them for each combination of types used. `Option` and `Result` without type
  arguments are errors.
- The type of `Some(x)` comes from `x`. `None`, `Ok(x)` and `Err(e)` take the missing type from
  the context (a type annotation, the return type, a parameter, the branch before them);
  without one, they are an error: `const empty: Option<i32> = None;`.
- F1 methods: `is_some()`, `is_none()`, `is_ok()`, `is_err()`, `unwrap()`, `expect(message)`,
  `unwrap_or(default)` and, on `Result`, `unwrap_err()`. `unwrap` and `expect` panic when the
  value is `None` or `Err`, with Rust's message (``called `Option::unwrap()` on a `None` value``)
  or the one given to `expect`. Methods taking closures (`map`, `map_err`, `and_then`,
  `unwrap_or_else`, `filter`) are described in [08-closures.md](08-closures.md).
