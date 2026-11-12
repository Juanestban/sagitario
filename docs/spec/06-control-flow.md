# 6. Expressions and control flow (F1)

## Blocks are expressions

The last expression of a block, written without a trailing `;`, is the value of the block:

```sag
fn square(x: i32) -> i32 { x * x }
```

- Block-like expressions (`if`, `match`, `while`, `loop`, `for`, `unsafe { ... }`, `{ ... }`)
  used as statements need no `;`; their value must then be `()`.
- As in Rust, a statement, final expression or match arm body that starts with a block-like
  expression ends at its `}`, unless `.` or `?` continues it: after `unsafe { n += 1; }`, a line
  `*a - *b` is the next expression, not a multiplication. Another operator there is an error
  (`E0100`); parentheses use the value: `(if c { 1 } else { 2 }) + 1`.
- A function with `-> T` must produce a `T` on every path: through its final expression or
  `return expr;`. A function without `-> T` uses `return;`.

## Conditionals

```sag
const sign = if x < 0 { -1 } else { 1 };
```

- The condition must be `bool`.
- `if` without `else` has type `()`. `if` used as a value of another type must have an `else`
  branch, and both branches must have the same type.

## `if let` and `while let`

Rationale: [ADR-0049](../adr/0049-self-hosting-plan-and-language-prerequisites.md).

```sag
if let Some(user) = find(id) {
  println(user.name);
} else {
  println("not found");
}

while let Some(token) = tokens.pop() {
  handle(token);
}
```

- `if let pattern = value { ... } else ...` runs the block when `value` matches the pattern,
  with its bindings; otherwise the `else` branch, which may be another `if` or `if let`. It is
  `match value { pattern => { ... }, _ => else }`, with the rules of `match` and of `if`.
- `while let pattern = value { ... }` evaluates `value` before each iteration and stops when
  it does not match. It is `loop { match value { pattern => { ... }, _ => break } }`; `break`
  and `continue` work as in any loop.

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
  maps (their entries) and iterators (`Iter<T>`), and over the values of any type implementing
  [`Iterable<T>`](05-interfaces-generics.md#iteration-and-collection), whose `iter()` it runs
  over ([ADR-0095](../adr/0095-collections.md), Q645). The loop variable is a new `const` binding on
  each iteration, or a tuple pattern whose parts always match, which binds each of its names:
  `for (index, c) in text.char_indices() { ... }`. An array is iterated through a copy taken before the loop, so assigning to it
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
  | `(p1, p2, ...)` | a tuple with as many elements, each matching its sub-pattern |
  | `(p1, .., pn)` | a tuple, `..` standing once for any number of elements |
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
| 2 | unary `-x`, `!x`, `await x` ([async](12-async.md#await): its operand ends before the first `?`) | — |
| 3 | `as` | left |
| 4 | `*` `/` `%` | left |
| 5 | `+` `-` | left |
| 6 | `<<` `>>` | left |
| 7 | `&` | left |
| 8 | `^` | left |
| 9 | `\|` | left |
| 10 | `==` `!=` `<` `<=` `>` `>=` | none: `a < b < c` is an error |
| 11 | `&&` | left, short-circuit |
| 12 | `\|\|` | left, short-circuit |
| 13 | `..` `..=` | none |
| 14 | `=` `+=` `-=` `*=` `/=` `%=` `&=` `\|=` `^=` `<<=` `>>=` | statements, not expressions |

- Operands of binary operators must have identical types (no implicit conversions), except for
  [operators of other types](05-interfaces-generics.md#operators), whose right operand has the
  type their implementation names.
- Arithmetic applies to integers and floats, and to the types that implement the
  [operator interfaces](05-interfaces-generics.md#operators). Integer overflow panics in debug builds and wraps
  in release builds (see [types](02-types.md#integer-arithmetic)). `/` and `%` by zero always
  panic. `%` is the remainder with the sign of the dividend.
- Unary `-` applies to signed integers and floats, and to types implementing `Neg`; it is an
  error on unsigned types. A negative literal such as `-128i8` is a single value and fits its
  type.
- `&&` and `||` apply only to `bool`. `!` is the logical not of a `bool`, and flips every bit of
  an integer (`!0u8` is `255`), as in Rust.
- The bitwise operators ([ADR-0088](../adr/0088-bitwise-operators-and-integer-methods.md)):
  `&`, `|` and `^` (and, or, exclusive or) take two integers of the same type, or two `bool`s,
  both evaluated. `a << n` and `a >> n` shift an integer `a` by `n` bits, an integer of any type
  (`u32` when it is an unsuffixed literal), and keep the type of `a`. `<<` drops the bits
  shifted out, also of signed numbers; `>>` is arithmetic on signed numbers and logical on
  unsigned ones (`-16 >> 2` is `-4`). A shift by a negative amount, or by the width of the type
  or more, panics in debug builds (`attempt to shift left with overflow`) and shifts by the
  amount modulo the width in release builds. Their precedence is Rust's, so `flags & MASK == 0`
  is `(flags & MASK) == 0`. They are not overloadable yet.
- `>>` is two `>` with nothing between them, so the `>>` that closes `List<List<T>>` stays two;
  `a > > b` is not a shift. `~` is not an operator: the bitwise not is `!`.
- `==` and `!=` apply to integers, floats, `char`, `bool` and `String`; `<` `<=` `>` `>=` to
  integers, floats, `char` and `String` (byte-wise lexicographic order). `+` also concatenates
  `String`s.
- On structs, enums and type parameters, `==` and `!=` call `equals` of `Eq`, and `<` `<=` `>`
  `>=` use `compare` of `Ord` ([equality and order](05-interfaces-generics.md#equality-order-and-hashing-f2));
  class instances compare by identity.
- Assignments (`=` and compound forms) are statements: they have no value, so `x = y = 0` is
  an error. Their target is a `var` binding, or a field or array element of one (see
  [struct mutability](03-memory.md#struct-mutability-f1)).

## Function calls and methods

- `f(a, b)`, `value.method(a)`, `Type::function(a)`; fields are read with `value.field` and
  array elements with `array[index]`.
- Paths use `::`: `Shape::Circle(1.0)`, `std::io::println`.
