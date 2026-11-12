# 2. Types and values

## Static typing

Sagitario is statically typed with **local type inference**:

- Local bindings may omit their type: `const x = 5;`.
- Function and method signatures must be fully annotated (parameters and return type).
- `-> T` declares the return type; a function without `->` returns the unit type `()`.

## Unit type

`()` is both a type and its only value. It is the implicit return type of functions without
`->` and is used in `Result<(), E>`. Tuples of two or more values are
[below](#tuples-s2).

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
- The [methods of integers](#methods-of-numbers-g1) `wrapping_*`, `checked_*` and `saturating_*`
  never panic on overflow: they wrap, give `None`, or stop at the limit of the type.
- `i32::MIN` and `i32::MAX` (and those of every integer type) are the limits of the type, as
  constants.
- Integer division by zero always panics, in every build mode.
- The generated C must never rely on undefined behavior (see
  [architecture](../architecture.md#c-code-generation)).

## Methods of numbers (G1)

Rationale: [ADR-0076](../adr/0076-std-math-and-methods-of-numbers.md),
[ADR-0077](../adr/0077-std-math-powers-and-logarithms.md),
[ADR-0080](../adr/0080-std-math-trigonometry.md),
[ADR-0088](../adr/0088-bitwise-operators-and-integer-methods.md). Besides `equals`,
`compare` and `hash` (integers) and `total_compare` (floats), numbers have these methods; their
arguments have the type of the receiver, unless written otherwise.

| Method | Integers | Floats | Result |
|---|---|---|---|
| `abs()` | signed | yes | the absolute value |
| `signum()` | signed | yes | `-1`, `0` or `1`; for floats `1.0` or `-1.0` with the sign of `x` (also `±0.0` and infinities), NaN for NaN |
| `min(other)`, `max(other)` | yes | yes | the smaller or larger value; for floats, if one is NaN, the other |
| `clamp(min, max)` | yes | yes | `x` restricted to `min..=max`; NaN for a NaN `x`; **panics** unless `min <= max` (also when a bound is NaN) |
| `pow(exp: u32)` | yes | | `x` multiplied by itself `exp` times; `0.pow(0)` is `1` |
| `copysign(sign)` | | yes | the magnitude of `x` with the sign of `sign` |
| `lerp(b, t)` | | yes | `(1 - t) * x + t * b`: `x` at `t == 0`, `b` at `t == 1`; `t` is not clamped |
| `floor()`, `ceil()`, `trunc()` | | yes | the integer below, above, or towards zero |
| `round()` | | yes | the nearest integer, halves away from zero (`2.5` → `3`, `-2.5` → `-3`) |
| `fract()` | | yes | `x - x.trunc()` |
| `is_nan()`, `is_infinite()`, `is_finite()` | | yes | `bool` |
| `sqrt()`, `cbrt()` | | yes | the square root (NaN for a negative number, `-0.0` for `-0.0`), the cube root |
| `powi(n: i32)`, `powf(y)` | | yes | `x` to the power `n` or `y`; `1.0` for a zero exponent, also for NaN; NaN for a negative `x` and a fractional `y` |
| `exp()`, `exp2()` | | yes | e or 2 to the power `x` |
| `ln()`, `log2()`, `log10()`, `log(base)` | | yes | the logarithm (NaN for a negative number, negative infinity for zero); `log(base)` is `ln(x) / ln(base)` |
| `hypot(y)` | | yes | √(x² + y²), without overflowing in between |
| `mul_add(a, b)` | | yes | `x * a + b`, rounded once |
| `sin()`, `cos()`, `tan()` | | yes | in radians; NaN for an infinity |
| `asin()`, `acos()`, `atan()` | | yes | the inverse, in `[-Ï€/2, Ï€/2]`, `[0, Ï€]` and `[-Ï€/2, Ï€/2]`; NaN outside `[-1, 1]` for `asin` and `acos` |
| `atan2(x)` | | yes | the angle of the point `(x, y)` with `y` the receiver, in `[-Ï€, Ï€]`, following the signs of zeros (`0.0.atan2(-0.0)` is Ï€) |
| `sinh()`, `cosh()`, `tanh()` | | yes | the hyperbolic functions |
| `to_degrees()`, `to_radians()` | | yes | the angle in degrees or in radians |
| `wrapping_add(other)`, `wrapping_sub(other)`, `wrapping_mul(other)` | yes | | the result modulo the width of the type, in every build |
| `wrapping_neg()` | signed | | `-x` modulo the width: the smallest value stays itself |
| `wrapping_shl(n: u32)`, `wrapping_shr(n: u32)` | yes | | `x << (n % width)`, `x >> (n % width)` |
| `checked_add(other)`, `checked_sub(other)`, `checked_mul(other)`, `checked_div(other)`, `checked_rem(other)` | yes | | `Option<T>`: the result, or `None` on overflow or a division by zero |
| `checked_neg()` | signed | | `Option<T>`: `None` for the smallest value |
| `checked_shl(n: u32)`, `checked_shr(n: u32)` | yes | | `Option<T>`: `None` when `n` is the width of the type or more |
| `saturating_add(other)`, `saturating_sub(other)`, `saturating_mul(other)` | yes | | the result, or the limit of the type it would pass |
| `count_ones()`, `count_zeros()`, `leading_zeros()`, `trailing_zeros()` | yes | | `u32`: the bits set or clear, in all or before the first set bit from the top or the bottom; the width for zero |
| `rotate_left(n: u32)`, `rotate_right(n: u32)` | yes | | the bits moved by `n % width`, those leaving one end entering at the other |
| `swap_bytes()` | yes | | the bytes in the reverse order |
| `is_power_of_two()` | unsigned | | whether `x` is `1`, `2`, `4`...; `false` for `0` |

- `abs()` and `pow()` overflow as arithmetic does: a panic in debug builds, wrapping in release
  builds (the `abs()` of the smallest value of a type is itself). On unsigned integers `abs()`
  and `signum()` do not exist.
- `sqrt` and `mul_add` are exact to the last bit. The other functions of floats from `cbrt` on
  are those of the system's C library, whose results may differ between systems in the last
  digit; `powi` computes in `f64`. `math::PI.sin()` is not exactly zero, since `PI` is not exactly
  Ï€. `to_degrees` and `to_radians` multiply by `180 / Ï€` or `Ï€ / 180`.
- Methods of a literal need no parentheses unless it is negative: `2.5.floor()`, `(-2.5).abs()`
  (`-2.5.abs()` is `-(2.5.abs())`).
- [`std::math`](11-standard-library.md#stdmath) has the constants (`math::PI`) and the methods of
  floats as functions of `f64` (`math::floor(x)`).

## `String` (F1)

- Always valid UTF-8 and **immutable**.
- Managed by ARC: copying a `String` value only increments its reference count.
- `+` and format strings create a new `String`. `StringBuilder` builds strings efficiently.
- Format strings interpolate any expression with `{expr}`: `f"total = {a + b * 2}"`
  ([lexical rules](01-lexical.md#other-literals-f1)); deprecated template strings do the same with
  `` `total = ${a + b * 2}` ``. Interpolable types and their format:

  | Type | Format |
  |---|---|
  | integers | decimal (`-42`) |
  | `f32`, `f64` | the shortest decimal that converts back to the same value, without exponent: `1.5`, `2` (for `2.0`), `0.30000000000000004`; `NaN`, `inf`, `-inf` |
  | `bool` | `true` / `false` |
  | `char` | the character |
  | `String` | as is |

  | types implementing `Display` | the text of their `to_string()` |

  `Display` is a prelude interface, `fn to_string(self) -> String`
  ([ADR-0089](../adr/0089-display-debug-and-format-specifiers.md)): a struct, enum or class
  implementing it, in its body or in an `impl Display for T` block, can be interpolated, and so
  can a type parameter bounded by it or a `Display` value. Numbers, `bool`, `char` and `String`
  implement it with the formats above (`5.to_string()` is `"5"`). Interpolating any other value
  is an error (`E0306`). Lists, maps, options and results do not implement it.
- A format specifier after a `:` shapes the text, with Rust's mini-language
  `[[fill]align][+][#][0][width][.precision][type]`
  ([ADR-0089](../adr/0089-display-debug-and-format-specifiers.md)):

  | Specifier | Meaning | Example |
  |---|---|---|
  | `width` | At least that many characters, filled with spaces: numbers align right, text left | `f"[{42:5}]"` is `[   42]`, `f"[{"Ada":5}]"` is `[Ada  ]` |
  | `<`, `^`, `>` | Left, centered (an odd fill goes right) or right, after an optional fill character | `{name:*^7}` is `**Ada**` |
  | `+` | The sign of every number | `{5:+}` is `+5` |
  | `0` | Zeros after the sign and prefix of a number, without an alignment | `{-42:05}` is `-0042` |
  | `.precision` | That many decimals of a float, rounded to the nearest; the first characters of a text | `{3.14159:.2}` is `3.14`, `{"Ada":.2}` is `Ad` |
  | `x`, `X`, `o`, `b` | An integer in hexadecimal, octal or binary; negative numbers show their two's complement bits | `{255:x}` is `ff`, `{-1i8:x}` is `ff` |
  | `#` | With `x`, `X`, `o`, `b`: the prefix `0x`, `0o`, `0b` | `{255:#06x}` is `0x00ff` |
  | `e`, `E` | A float in scientific notation | `{1500.0:e}` is `1.5e3`, `{1500.0:.2e}` is `1.50e3` |

  Width and alignment apply to the text of any interpolated value, `Display` types included. A
  specifier that does not fit its value (`x` on a float, a precision on an integer, `+` or `0` on
  text, `#` without a base) or is not valid is an error (`E0007`). The widths and precisions are
  literal numbers.
- `{x:?}` writes the debug form of any value, which the compiler gives every type, as Rust's
  derived `Debug`: numbers and `bool` as above, `char` and `String` quoted with Rust's escapes
  (`'a'`, `"tab	"`), `Some(1)`, `None`, `Ok(..)`, `Err(..)`, structs and classes with every
  field (`Point { x: 1, y: 2 }`, those of a parent class first), enum variants by name with their
  fields (`Circle(2.5)`, `Rect { w: 1, h: 2 }`, `Empty`), lists, arrays and slices as `[1, 2]`, maps
  as `{"a": 1}`, a weak field as `(weak)`, a function value as `fn`, a raw pointer as its address
  and an interface value as `dyn Name`. A class instance reached again while its own form is
  being written is `Name { .. }`. `{x:#?}` writes one field or element per line, indented by four
  spaces, each followed by a comma. A width, fill and alignment apply to the debug form too; `+`,
  `0` and a precision do not (`E0007`).
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
  | `rfind(p)` | `Option<usize>`: byte index of the last occurrence |
  | `split_once(p)`, `rsplit_once(p)` | `Option<(String, String)>`: the text before and after the first or last occurrence |
  | `strip_prefix(p)`, `strip_suffix(p)` | `Option<String>`: the text without `p`, if it starts or ends with it |
  | `split_whitespace()` | `List<String>`: the parts between runs of Unicode white space, none empty |
  | `splitn(n, sep)`, `rsplit(sep)` | `List<String>`: at most `n` parts, the last holding the rest; the parts of `split` from the last |
  | `matches(p)` | `List<String>`: the occurrences of `p`, without overlapping |
  | `replacen(from, to, n)` | `String` with the first `n` occurrences replaced |
  | `char_indices()` | `Iter<(usize, char)>`: each character with the byte index where it starts |
  | `to_uppercase()`, `to_lowercase()` | `String` with Unicode's full case mappings, without a locale, as Rust's: `"Straße"` is `"STRASSE"`, and a final `Σ` becomes `ς` |

  `String::from_utf8(bytes: List<u8>)` and `String::from_utf16(units: List<u16>)` give a
  `Result<String, String>` with Rust's messages (`invalid utf-8 sequence of 1 bytes from index 0`),
  and `String::from_utf8_lossy(bytes)` replaces each ill-formed sequence with U+FFFD
  ([ADR-0093](../adr/0093-everyday-methods.md), Q620).

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
- `char` methods (Unicode 17, as Rust's `char`; [ADR-0093](../adr/0093-everyday-methods.md),
  Q621): `is_alphabetic()`, `is_numeric()` (the categories `Nd`, `Nl` and `No`),
  `is_alphanumeric()`, `is_whitespace()`, `is_uppercase()`, `is_lowercase()`, `is_control()`
  (`Cc`), `to_uppercase()` and `to_lowercase()` (a `String` of one to three characters: `'ß'` is
  `"SS"`), `len_utf8()` and `len_utf16()`. `char::from_digit(digit, radix)` gives the character
  of a digit, lowercase past 9, or `None`; a radix past 36 panics.

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
  `clear()`, `to_string()` (a new `String`; the builder stays usable), and Rust's
  `insert(index, text)`, `truncate(len)`, `pop() -> Option<char>`, `capacity()` and
  `reserve(additional)`. An index inside a character, or past the end for `insert`, panics.
- It cannot be compared with `==` or interpolated.

## Equality (F1/F2)

- `==` is defined between primitives, between `String`s, and between data-less enum values.
- Between class instances, `==` is **identity** (same instance).
- Structs and enums compare by value through the prelude interfaces `Eq` and `Ord`
  (F2, [interfaces](05-interfaces-generics.md#equality-order-and-hashing-f2)), which integers,
  `bool`, `char` and `String` also implement: on a type implementing `Eq`, `a == b` calls
  `a.equals(b)` and `a != b` negates it; on one implementing `Ord`, `<`, `<=`, `>` and `>=`
  compare by `a.compare(b)` ([ADR-0081](../adr/0081-operator-overloading.md), Q514). A data-less
  enum implementing `Eq` uses its `equals` too. Classes call `a.equals(b)`, since their `==` stays
  identity.
- Iterators, interface values and function values cannot be compared with `==`. Tuples,
  `Option`, `Result`, arrays, lists and maps compare when the values they hold do
  ([tuples](#tuples-s2), [`Option` and `Result`](#optiont-and-resultt-e-f1),
  [lists](#lists-f2), [maps](#maps-f2)); a map is the one class instance `==` compares by value.
- [Arithmetic operators](05-interfaces-generics.md#operators) (`+`, `-`, `*`, `/`, `%`, unary
  `-`) apply to other types through the prelude interfaces `Add`, `Sub`, `Mul`, `Div`, `Rem` and
  `Neg` ([ADR-0081](../adr/0081-operator-overloading.md)).

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
- Only enums whose variants carry no data support `==` and `!=`. Enums cannot be interpolated,
  and only those with an integer representation (`@repr(i32)`, for C) are converted with `as`,
  to integers ([FFI](10-ffi.md#enums-with-an-integer-representation)).
- Values are inspected with an exhaustive `match` (see [control flow](06-control-flow.md#match)).

## Tuples (S2)

Rationale: [ADR-0092](../adr/0092-tuples.md).

```sag
fn divmod(a: i32, b: i32) -> (i32, i32) {
  (a / b, a % b)
}

const pair = ("Ada", 36);
const (q, r) = divmod(17, 5);
println(f"{pair.0} is {pair.1}, {q} rest {r}, {pair:?}");   // Ada is 36, 3 rest 2, ("Ada", 36)
```

- `(T1, T2, ..., Tn)` is a tuple of two or more values, of any types, with a trailing comma
  allowed. `()` is the unit type and `(T)` is `T`; there are no tuples of one element, and
  `(T,)` is an error.
- `(a, b)` makes a tuple; `t.0`, `t.1`... read its elements, by a decimal index without a
  suffix (`t.0.1` reads element 1 of element 0). An index past the last element is an error.
- Tuples are value types, as structs: copied on assignment, with the references they hold
  counted, and mutable through a `var` binding, `self` or a `var` field of a class
  ([struct mutability](03-memory.md#struct-mutability-f1)): `t.0 = 1`.
- A tuple implements `Eq`, `Ord` and `Hash` when every element does: `==` compares element by
  element, and `<` orders by the first element that differs, the first one first. It is `Send`
  when every element is. It has a debug form, `(1, "a")`, but no `Display`, so `{t:?}`
  interpolates it and `{t}` does not. A program cannot implement interfaces for tuple types.
- Tuple patterns, `(p1, p2)` and `(first, ..)`, take tuples apart in `match`, `if let`,
  `while let`, `const`, `var` and `for` ([control flow](06-control-flow.md#match)).
- Tuples cannot cross to C: `extern` declarations take `@repr(C)` structs.

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
  They cannot be interpolated. They compare with `==` and order with `<` when their elements
  do, element by element, as lists ([ADR-0093](../adr/0093-everyday-methods.md), Q623).
- Arrays have the [methods of slices](#slices) below, lent all their elements: `contains`,
  `index_of`, `binary_search`, `min`, `max`, and, on a `var` array, `sort`, `sort_by`,
  `sort_by_key`, `reverse`, `fill`, `swap` and `copy_from_slice` ([ADR-0093](../adr/0093-everyday-methods.md),
  Q624).
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
  | `resize(n, value)` | `()`: removes the elements past `n`, or adds copies of `value` up to `n` (Rust's `Vec::resize`) |
  | `as_ptr()`, `as_mut_ptr()` | `*const T`, `*mut T` to the first element, for C ([FFI](10-ffi.md#raw-pointers)) |
  | `iter()` and the methods of `Iter<T>` | see [iterators](08-closures.md#iterators) |
  | `extend(values)`, `append(other)` | `()`: adds the values of an `Iter<T>`, or moves every element of `other`, which is left empty |
  | `truncate(n)`, `retain(keep)`, `dedup()` | `()`: keeps the first `n`, those for which `keep(v)` is `true`, or drops each element equal to the one before it (`T: Eq`) |
  | `split_off(at)`, `drain(start, end)` | `List<T>`: the elements from `at`, or from `start` to `end`, removed; past the end panics |
  | `binary_search(v)`, `binary_search_by(f)` | `Result<usize, usize>`: where `v` is in a sorted list (`T: Ord`), or `Err` with where it would go |
  | `sort_by_key(key)` | `()`: stable, by `key: fn(T) -> K` with `K: Ord` |
  | `min()`, `max()` | `Option<T>` (`T: Ord`): the first smallest, the last largest |
  | `fill(v)`, `swap(a, b)` | `()` |
  | `windows(n)`, `chunks(n)` | `Iter<List<T>>`: each run of `n` elements, or the elements in lists of `n`, as copies; `n == 0` panics |
  | `concat()` | `List<T>`, only on a `List<List<T>>` |
  | `capacity()`, `reserve(n)` | `usize`, `()`: the room for elements without growing, and room for `n` more |

  `List::with_capacity(n)` is an empty list with room for `n` elements; its element type comes
  from the context. These are Rust's ([ADR-0093](../adr/0093-everyday-methods.md), Q622):
  `drain` takes indices, since ranges are not values, and `windows` and `chunks` give copies,
  since slices are only parameters.

- `for x in list` visits the elements by position and reads the length at every step:
  elements added or removed by the body are seen, and the loop never reads out of bounds.
- A list implements `Eq`, `Ord` and `Hash` when its elements do, as Rust's `Vec`: `==` compares
  their lengths and elements, and `<` orders by the first element that differs, then by length
  ([ADR-0093](../adr/0093-everyday-methods.md), Q623). A list that is the key of a map must not
  change while it is one. Lists cannot be interpolated.

### Slices

A slice `&[T]` or `&mut [T]` ([ADR-0079](../adr/0079-references-and-borrowing.md), increment 2) is a
view of a run of elements of an array, a list or another slice, without copying them. Like a
[reference](03-memory.md#references), it is only the type of a parameter (`[T]` alone is an error,
`E0339`), and it is passed to one, never stored or returned.

```sag
fn sum(values: &[i32]) -> i32 {
  var total = 0;
  for value in values { total += value; }
  total
}

fn double(values: &mut [i32]) {
  for i in 0..values.len() { values[i] *= 2; }
}

const list = List::from([1, 2, 3, 4]);
sum(&list);           // the whole list
sum(&list[1..3]);     // a range: elements 1 and 2
double(&mut list);
```

- **Making one.** Where a slice is expected, `&array`, `&list` and `&slice` lend all the elements,
  and `&items[a..b]` or `&items[a..=b]` a range of them (`&mut` too). `&[1, 2, 3]` lends a
  temporary array. Inside a function, `slice[a..b]` is a part of a slice parameter, passed on as a
  slice of the same kind. A range of an array or a list without `&` is an error (`E0310`). A
  range out of bounds panics with Rust's messages (`range end index 5 out of range for slice of
  length 3`, `slice index starts at 3 but ends at 2`).
- **What it allows.** `len()`, `is_empty()`, `s[i]` checked as for arrays (`index out of
  bounds`), `for value in s`, `iter()` (an `Iter<T>` over a copy of the elements, which may
  outlive the call), `as_ptr()` and, through a `&mut [T]`, `as_mut_ptr()` for C. Through a
  `&mut [T]`, `s[i] = value`, `s[i].field = value`, methods that modify `self` on `s[i]`, and
  `&mut s[i]` change the viewed elements; through a `&[T]` they are errors (`E0312`, `E0313`).
- **Methods** ([ADR-0093](../adr/0093-everyday-methods.md), Q624): `contains(v)`, `index_of(v)`
  (`T: Eq`), `binary_search(v)` (`T: Ord`), `binary_search_by(f)`, `min()` and `max()`; through a
  `&mut [T]`, `sort()`, `sort_by(f)`, `sort_by_key(key)`, `reverse()`, `fill(v)`, `swap(a, b)` and
  `copy_from_slice(source)`, which panics unless `source` has as many elements. Arrays and lists
  have them too, lent as slices of all their elements. `split_at` is `&s[..i]` and `&s[i..]`.
- **Mutability.** `&mut array` needs an array that can be modified (a `var` binding, a `&mut`
  parameter, a `var` field). A list is a reference, so `&mut list` works through any binding, as
  `list[i] = value` does. `&mut slice` needs a `&mut [T]` slice.
- **Borrowed lists** (Q495). A list lent as a slice keeps a count of its slices: while one is
  lent, `push`, `insert`, `pop`, `remove`, `clear`, `resize` and any other change of its length
  panic (`cannot change the length of a list while it is borrowed`), as Rust's `RefCell` does,
  since another reference to the list could otherwise move the elements the slice views.
  Changing elements in place stays allowed. The slice also keeps the list alive until the call
  returns.
- The borrow checker treats a slice as a reference ([exclusivity](03-memory.md#references)): two
  ranges of the same array or list binding are never known to be disjoint, so
  `f(&mut a[0..2], &a[2..4])` is an error (`E0341`).
- In C, a slice is a pointer, a length and the list it views, if any (`sag_slice`).

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
- A map implements `Eq` when its values do: two maps are equal when they hold the same keys with
  equal values, in any order ([ADR-0093](../adr/0093-everyday-methods.md), Q623). Maps are not
  ordered and cannot be interpolated.

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
- Rust's other methods ([ADR-0093](../adr/0093-everyday-methods.md), Q619): on `Option`,
  `ok_or`, `ok_or_else`, `or`, `or_else`, `and`, `xor`, `map_or`, `map_or_else`, `is_some_and`,
  `is_none_or`, `zip` (an `Option` of a tuple), `flatten` (of an `Option<Option<T>>`) and `iter`;
  on `Result`, `ok`, `err`, `and`, `or`, `or_else`, `map_or`, `map_or_else`, `is_ok_and`,
  `is_err_and`, `iter` and `expect_err(message)`, which panics on `Ok`. `take` and `replace`
  change the place that holds the value, which `mem::replace` does.
- `Option<T>` and `Result<T, E>` implement `Eq`, `Ord` and `Hash` when the types they hold do:
  equal when they are the same variant holding equal values, and ordered by variant as declared
  (`None` before `Some`, `Ok` before `Err`), then by those values: `Some(1) == Some(1)`,
  `None < Some(0)`.
