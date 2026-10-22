# 6. Expressions and control flow (F1)

## Blocks are expressions

The last expression of a block, written without a trailing `;`, is the value of the block:

```sag
fn square(x: i32) -> i32 { x * x }
```

`return expr;` returns early from a function.

## Conditionals

```sag
const sign = if x < 0 { -1 } else { 1 };
```

- The condition must be `bool`.
- `if` used as a value must have an `else` branch, and both branches must have the same type.

## Loops

```sag
while n > 0 { n -= 1; }
loop { if done() { break; } }
for item in items { ... }
```

- `for` iterates over ranges and arrays (F1) and, from F2, over any iterable.
- `break` and `continue` apply to the innermost loop.

## Ranges (F1)

```sag
for i in 0..10 { ... }    // 0 to 9
for i in 1..=10 { ... }   // 1 to 10
```

- `a..b` is half-open (includes `a`, excludes `b`); `a..=b` is closed.
- Both bounds must be integers of the same type (no implicit conversions).
- If `a > b` the range is empty; it does not panic.
- F2: `(0..10).step_by(2)` and `(0..10).rev()` through iterators.
- Open ranges (`..b`, `a..`) are post-MVP.

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

Arithmetic (`+ - * / %`), comparison (`== != < <= > >=`), logical (`&& || !`) and compound
assignment (`+= -= *= /= %=`). Operands must have identical types (no implicit conversions).

## Function calls and methods

- `f(a, b)`, `value.method(a)`.
- Paths use `::`: `Shape::Circle(1.0)`, `std::io::println`.
