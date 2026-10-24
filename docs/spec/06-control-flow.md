# 6. Expressions and control flow (F1)

## Blocks are expressions

The last expression of a block, written without a trailing `;`, is the value of the block:

```sag
fn square(x: i32) -> i32 { x * x }
```

- Block-like expressions (`if`, `while`, `loop`, `for`, `{ ... }`) used as statements need no
  `;`; their value must then be `()`.
- A function with `-> T` must produce a `T` on every path: through its final expression or
  `return expr;`. A function without `-> T` uses `return;`.

## Conditionals

```sag
const sign = if x < 0 { -1 } else { 1 };
```

- The condition must be `bool`.
- `if` without `else` has type `()`. `if` used as a value of another type must have an `else`
  branch, and both branches must have the same type.

## Loops

```sag
while n > 0 { n -= 1; }
const found = loop { if done() { break 42; } };
for i in 0..10 { ... }
```

- `while` and `for` have type `()`; inside them `break` takes no value.
- `loop` may produce a value with `break value;`; every `break` of one `loop` must agree on
  the type. A `loop` without any `break` never finishes.
- `break` and `continue` apply to the innermost loop. Labels are post-MVP.
- `for` iterates over ranges, arrays and `s.chars()`/`s.bytes()` (F1) and, from F2, over lists,
  maps (their entries) and iterators (`Iter<T>`). The loop variable is a new `const` binding on
  each iteration. An array is iterated through a copy taken before the loop, so assigning to it
  inside the body does not change the iteration. A list is read by position at every step, so
  changes made by the body are seen (see [lists](02-types.md#lists-f2)).
- The condition of `if`/`while` and the iterable of `for` cannot be a struct literal written
  directly, because `{` starts the body; wrap it in parentheses:
  `if (Point { x: 0.0, y: 0.0 }).len() > 1.0 { ... }`.

## Divergence

`return`, `break`, `continue`, a `loop` without `break`, and panics never produce a value.
Their type is an internal *never* type, compatible with every type, so
`const x = if ok { 1 } else { return; };` is valid. Programs cannot write this type.

Statements that can never run (e.g. after `return`) produce an *unreachable code* warning.

## Ranges (F1)

```sag
for i in 0..10 { ... }    // 0 to 9
for i in 1..=10 { ... }   // 1 to 10
```

- `a..b` is half-open (includes `a`, excludes `b`); `a..=b` is closed.
- Both bounds must be integers of the same type (no implicit conversions).
- If `a > b` the range is empty; it does not panic.
- Outside of `for`, a range is an iterator (F2): `(1..=5).map((n) => n * n).collect()`.
- `(a..b).rev()` counts down from the last value; `rev()` is only valid written directly on a
  range. `step_by(n)` keeps the first value and every `n`-th after it; a step of 0 panics. In a
  `for`, both compile to a counting loop without an iterator, and never step past the type's
  limits: `for b in (0u8..=255u8).rev()` visits 256 values.
- Open ranges (`..b`, `a..`) are post-MVP.

## `match`

```sag
match s {
  Shape::Circle(r) => PI * r * r,
  Shape::Rect { w, h } => w * h,
}
```

- `match` is an expression. Arms are separated by commas, optional after a `{ ... }` body.
  All arms must have the same type, as the branches of `if`.
- It **must be exhaustive**; a missing case is a compile error that names the values not
  covered. An arm that can never match because earlier arms cover its values is a warning.
- Patterns:

  | Pattern | Matches |
  |---|---|
  | `_` | anything |
  | `name` | anything, binding it to `name` |
  | `0`, `-1`, `'a'`, `true`, `"quit"` | that integer, character, boolean or string |
  | `None`, `Shape::Empty` | that variant without data |
  | `Some(p)`, `Shape::Circle(p)` | that variant, with its fields matching the sub-patterns |
  | `Shape::Rect { w, h: p, .. }` | that variant by field name; `w` is shorthand for `w: w`, `..` ignores the rest (otherwise every field is listed) |
  | `p1 \| p2` | either pattern |

- A guard, `pattern if condition => ...`, only takes the arm when the condition holds; guarded
  arms do not count for exhaustiveness.
- Bindings are new immutable bindings holding a copy of the matched part. Every alternative of
  `|` must bind the same names with the same types.
- Float literals are not patterns; range and struct patterns are post-MVP.

## Operators

| Precedence (high → low) | Operators | Associativity |
|---|---|---|
| 1 | calls `f()`, methods `.m()`, indexing `[i]`, `?` | left |
| 2 | unary `-x`, `!x` | — |
| 3 | `as` | left |
| 4 | `*` `/` `%` | left |
| 5 | `+` `-` | left |
| 6 | `==` `!=` `<` `<=` `>` `>=` | none: `a < b < c` is an error |
| 7 | `&&` | left, short-circuit |
| 8 | `\|\|` | left, short-circuit |
| 9 | `..` `..=` | none |
| 10 | `=` `+=` `-=` `*=` `/=` `%=` | statements, not expressions |

- Operands of binary operators must have identical types (no implicit conversions).
- Arithmetic applies to integers and floats. Integer overflow panics in debug builds and wraps
  in release builds (see [types](02-types.md#integer-arithmetic)). `/` and `%` by zero always
  panic. `%` is the remainder with the sign of the dividend.
- Unary `-` applies to signed integers and floats; it is an error on unsigned types. A negative
  literal such as `-128i8` is a single value and fits its type.
- `!`, `&&` and `||` apply only to `bool`. There are no bitwise operators in the MVP.
- `==` and `!=` apply to integers, floats, `char`, `bool` and `String`; `<` `<=` `>` `>=` to
  integers, floats, `char` and `String` (byte-wise lexicographic order). `+` also concatenates
  `String`s.
- Assignments (`=` and compound forms) are statements: they have no value, so `x = y = 0` is
  an error. Their target is a `var` binding, or a field or array element of one (see
  [struct mutability](03-memory.md#struct-mutability-f1)).

## Function calls and methods

- `f(a, b)`, `value.method(a)`, `Type::function(a)`; fields are read with `value.field` and
  array elements with `array[index]`.
- Paths use `::`: `Shape::Circle(1.0)`, `std::io::println`.
