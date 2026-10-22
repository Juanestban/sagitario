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
- `for` iterates over ranges and arrays (F1) and, from F2, over any iterable. The loop
  variable is a new `const` binding on each iteration.

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
- F2: `(0..10).step_by(2)` and `(0..10).rev()` through iterators.
- Open ranges (`..b`, `a..`) are post-MVP. In F1, ranges are only valid as the iterable of `for`.

## `match`

```sag
match s {
  Shape::Circle(r) => PI * r * r,
  Shape::Rect { w, h } => w * h,
}
```

- `match` is an expression.
- It **must be exhaustive**; a missing case is a compile error.
- Patterns destructure enum variants (positional and named fields), literals and `_`.

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
- `==` and `!=` apply to integers, floats, `char` and `bool`; `<` `<=` `>` `>=` to integers,
  floats and `char`. Comparing `String`s arrives with the rest of the `String` API.
- Assignments (`=` and compound forms) are statements: they have no value, so `x = y = 0` is
  an error. Their target must be a `var` binding.

## Function calls and methods

- `f(a, b)`, `value.method(a)`.
- Paths use `::`: `Shape::Circle(1.0)`, `std::io::println`.
