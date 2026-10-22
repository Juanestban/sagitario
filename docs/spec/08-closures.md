# 8. Closures (F2)

## Syntax

```sag
const double = (x) => x * 2;
const add = (a: i32, b: i32) => { a + b };
const evens = nums.filter((n) => n % 2 == 0);
```

Parameter types may be omitted when they can be inferred from context.

## Function types

- The type of functions and closures is written `fn(A, B) -> R`, e.g.
  `fn apply(f: fn(i32) -> i32, x: i32) -> i32`.
- Named functions and closures share the same type and are interchangeable.

## Captures

- Closures capture **by value**: captured structs are copied; captured class references are
  retained (the object is shared, the binding is not).
- Captured variables are **read-only** inside the closure. Assigning to a captured variable
  is a compile error; the diagnostic suggests `fold`/`reduce` or a class instance.

```sag
var total = 0;
nums.for_each((x) => total += x);           // error: cannot mutate captured `total`
const total = nums.fold(0, (acc, x) => acc + x);   // ok
```

## Reference cycles

A closure stored in a class field that captures `self` creates a cycle that ARC cannot
reclaim. The compiler **must emit a warning** for this pattern. `weak` capture syntax is
post-MVP.

## Representation

A closure is a function pointer plus an ARC-managed environment. Closures with no captures
must not allocate.
