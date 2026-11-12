# 0092. Tuples (S2)

- Status: Accepted
- Date: 2026-11-12
- Refines: docs/spec/02-types.md ("There are no general tuples in the MVP; they may be added
  later") and [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: the audit of S2
  puts the language gaps first)

## Context

The everyday methods of the audit of S2 ([`docs/std-audit.md`](../std-audit.md), record 4)
return two values at once in Rust: `enumerate`, `zip`, `unzip`, `partition`, `split_once`,
`char_indices`, and `Map::iter` itself. Sagitario has no tuples, so a library would answer with
a named struct for each (`Entry<K, V>` is the one it has), and those names would stay even if
tuples came later. Functions that return two results (`divmod`, a parser's value and the rest of
its input) declare a struct for it today, and a map cannot be keyed by two values without one.

Rust's tuples are values with positional elements, compared, ordered and hashed when their
elements are, and taken apart by patterns. Sagitario's structs are already values with the same
rules of copying and mutability (spec 3), so tuples are anonymous structs with positional
fields.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q608 | The type | `(T1, T2, ..., Tn)` with two or more elements, any number of them, a trailing comma allowed: `(i32, String)`. `()` stays the unit type, and `(T)` is `T`. There are no tuples of one element: `(T,)` is an error whose help suggests `T`. |
| Q609 | Values | `(a, b)` makes a tuple, and `t.0`, `t.1`... read its elements, a decimal index without a suffix (`t.0.1` is two reads). A tuple is a value as a struct is (spec 3): copied on assignment, its references counted, and mutable through a `var` binding, `self` or a `var` field, as a struct's fields are (`t.0 = 1`). An index past the last element is an error. |
| Q610 | Patterns | `(p1, p2, ...)` matches a tuple with as many elements, each matching its sub-pattern, and `..` once in it stands for any number of elements (`(first, ..)`). Tuple patterns go wherever patterns do (`match`, `if let`, `while let`) and, when every part always matches (names, `_`, `..` and tuple patterns of them), in `const` and `var` declarations and in `for`: `const (q, r) = divmod(a, b);`, `for (index, c) in text.char_indices() { ... }`. A pattern that might not match there is an error, and an annotation is the type of the whole: `var (low, high): (i32, i32) = (0, 9);`. |
| Q611 | Interfaces | A tuple implements `Eq`, `Ord` and `Hash` when every element does, comparing and ordering element by element, the first one first; so `==`, `<`, sorting, and maps and sets keyed by tuples work. It is `Send` when every element is. It has a debug form, `(1, "a")`, and no `Display`, as in Rust. No interface can be implemented for a tuple type by a program. |
| Q612 | What stays out | Patterns in the parameters of functions and lambdas (`((index, value)) => ...`) and destructuring assignment (`(a, b) = (b, a);`), each for a record of its own; tuple structs (`struct Meters(f64);`); tuples in `extern` declarations, which take `@repr(C)` structs (an error, as other types that cannot cross to C). |
| Q613 | C | Each tuple type a program uses becomes one C struct, its elements fields `_0`, `_1`..., emitted once as the instances of generic structs are; its references are retained and released as a struct's fields. |
| Q614 | Diagnostics | A tuple of one element, an index past the end, a pattern of the wrong number of elements and a pattern that might not match in `const`, `var` or `for` are errors with codes of their own or the existing ones for types and fields that do not match, each with a `compile-fail` case. |
| Q615 | The editor | The grammar highlights the index of `t.0` as a field; the completion of `sag lsp` offers the indices after `t.`, with their types, and its semantic tokens mark the names a tuple pattern binds as variables; the editor's README lists tuples. |
| Q616 | Increments | **1** Tuple types and values, reading their elements, `Eq`, `Ord`, `Hash`, `Send`, the debug form and the C (Q608, Q609, Q611, Q613). **2** Tuple patterns (Q610). **3** The standard library returning tuples, in the record of the everyday methods. |

### Example

```sag
fn divmod(a: i32, b: i32) -> (i32, i32) {
  (a / b, a % b)
}

fn main() {
  const (q, r) = divmod(17, 5);
  const pair = ("Ada", 36);
  const seen = new Map<(i32, i32), bool>();

  seen.insert((0, 1), true);
  println(f"{q} {r} {pair.0} is {pair.1} {pair:?}");   // 3 2 Ada is 36 ("Ada", 36)

  match divmod(10, 2) {
    (_, 0) => println("exact"),
    (q, r) => println(f"{q} rest {r}"),
  }
}
```

## Alternatives considered

- **Named structs in the prelude** (`Pair<A, B>`, `Indexed<T>`): no change of the language, but
  one name per shape, fields to remember (`first`, `index`), and the names would outlive the
  arrival of tuples.
- **Tuples of one element, as Rust's `(T,)`:** they exist in Rust mostly for macros and generic
  code over arities; Sagitario has neither, and `(x,)` beside `(x)` invites mistakes.
- **Patterns in parameters and destructuring assignment now:** both are useful
  (`.map(((i, x)) => ...)`, swapping two values), but each touches a part of the parser and the
  checker of its own; they wait for their records so that tuples land first.

## Consequences

- `enumerate`, `zip`, `split_once` and the other methods of record 4 can have Rust's signatures.
- Tuples bring a second kind of positional access, `t.0`, which the lexer must split from a
  float literal after a `.`.
- Generic code over tuples is monomorphized as everything else: each tuple type is a C struct,
  and the compiler gives each one its `Eq`, `Ord`, `Hash` and debug form.
