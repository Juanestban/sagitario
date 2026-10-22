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

- `for` iterates over ranges (syntax: open question Q4) and, from F2, over any iterable.
- `break` and `continue` apply to the innermost loop.

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
