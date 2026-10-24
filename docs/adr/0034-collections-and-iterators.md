# 0034. Collections, iterators, and equality, order and hashing

- Status: Accepted
- Date: 2026-10-23
- Refines: [ADR-0017](0017-resolution-of-open-questions-q1-q11.md) (Q5, lists),
  [ADR-0032](0032-generics.md) (Q53, primitives and bounds),
  [ADR-0013](0013-primitive-semantics.md) (equality)

## Context

The roadmap's F2 increment 7 adds `List<T>`, `Map<K, V>` and iterators with `map` and
`filter`. The spec already fixed several points:
- `List<T>` is a growable ARC reference, created with `new List<i32>()` or `List::from([...])`;
- `[1, 2, 3]` is always a fixed array;
- arrays get lazy iterator methods;
- `for` iterates over any iterable;
- ranges get `step_by` and `rev`;
- strings get `split` and `lines`.

There are no tuples. Q53 had left equality and order in generic code to this increment.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q67 | Equality, order and hashing | Prelude interfaces `Eq { fn equals(self, other: Self) -> bool; }`, `Ord extends Eq { fn compare(self, other: Self) -> Ordering; }` and `Hash extends Eq { fn hash(self) -> u64; }`, with `enum Ordering { Less, Equal, Greater }`. Integers, `bool`, `char` and `String` implement all three. Programs implement them with `impl`. Generic code calls `a.equals(b)`; `==` does not change (operator overloading stays post-MVP). |
| Q68 | Floats | Implement none of them, as in Rust, since `NaN != NaN`. `x.total_compare(y)` orders them by the IEEE 754 total order. |
| Q69 | `List<T>` | `len`, `is_empty`, `push`, `pop`, `get`, `first`, `last` (the last four give `Option<T>`), `insert`, `remove`, `clear`, `reverse`, `sort_by(f)`, and `iter()`. With `T: Eq`: `contains`, `index_of`. With `T: Ord`: `sort()`, which is stable. On `List<String>`: `join(sep)`. A missing bound is E0330. |
| Q70 | Indexing | `list[i]` reads a copy and panics out of bounds. `list[i] = v` writes through any binding, `const` included, because the list is a reference. `list[i].x = v` on a struct element is an error (E0307), since the element read is a copy. |
| Q71 | `Map<K, V>` | `new Map<K, V>()` with `K: Hash` (and so `Eq`). `insert` (returns the previous value), `get` (a copy), `contains_key`, `remove`, `len`, `is_empty`, `clear`, `keys()`, `values()`, `entries()`. No `map[k]`. Iteration follows insertion order. Entries are the prelude struct `Entry<K, V> { key, value }`. |
| Q72 | Iterator type | One lazy class `Iter<T>` in the prelude, built from a step function `fn() -> Option<T>`: `new Iter(() => ...)` makes custom iterators. Each adapter allocates and calls through a function value. |
| Q73 | Iterator methods | Lazy: `map`, `filter`, `take`, `skip`, `step_by`. Consuming: `collect` (a `List<T>`), `count`, `fold`, `for_each`, `any`, `all`, `find`. Lists and arrays have the same methods, as shorthands for `.iter().method(...)`. No `enumerate`/`zip` (no tuples) and no `sum` (no numeric interface). |
| Q74 | `for` | Iterates over ranges, arrays (a copy), `Iter<T>`, `List<T>` and `Map<K, V>` (its entries). A list is visited by position, reading its length at every step: changes made by the body are seen, and the loop never leaves the bounds. The loop variable owns its value. |
| Q75 | Ranges outside `for` | `a..b` used as a value is an `Iter<T>`. `rev()` is only valid written directly on a range. In `for`, `(a..b).rev()` and `(a..b).step_by(n)` compile to counting loops without iterators. `step_by(0)` panics. |
| Q76 | Strings | `split(sep)` and `lines()` give `List<String>`. `chars()` and `bytes()` outside of `for` give `Iter<char>` and `Iter<u8>`. |
| Q77 | Equality and interpolation | `List`, `Map` and `Iter` are not compared with `==` or interpolated. |

Points the table left open, decided while implementing:

- **`Hash extends Eq`.** A key type needs both. An interface without `Self` could also be a
  type, which would make primitives become interface values.
- **`split("")`** splits into characters, as in JavaScript.
- **`lines()`** removes `\n` and `\r\n` and ignores a final line break, as in Rust.
- **Type inference from lambda results.** In a generic call, a lambda whose parameter types are
  known infers type parameters from its result: `iter.map(n => n * 2)` infers `U`.

### Representation

- **`List<T>`** is built into the compiler (`Type::List`): a runtime object (`sag_list`)
  holding the elements, with a function releasing one element when they are managed. An
  element is a place behind the list (`Projection::ListElem`). Like a class field, reading a
  managed element takes an owned copy.
- **Written in Sagitario:** `Iter`, `Map` (an insertion-ordered open-addressing hash table over
  lists), `Entry`, `Ordering`, the three interfaces, and the functions behind the methods of
  lists, strings and ranges. The driver adds one range function per integer type.
- **Lazy prelude:** prelude functions are lowered only when used, so programs contain only the
  parts they need.
- **Primitives** implement `Eq`, `Ord` and `Hash` without methods: their calls are lowered to
  comparisons and to the runtime's `sag_hash_u64` and `sag_string_hash`.

## Alternatives considered

- **`List` and `Map` written in Sagitario:** `List` needs storage whose elements a generic
  destructor can release, which the language cannot express. `Map` can, on top of `List`.
- **A Rust-style `Iterator<T>` interface with one adapter type per method:** faster, but the
  types (`Filter<Map<ListIter<i32>>>`) leak into signatures and messages. It can be added later
  without breaking `Iter<T>`.
- **`compare` returning an `i32`, as in Java and JavaScript:** `(a, b) => a - b` overflows,
  which panics in Sagitario.
- **Unordered maps:** iteration order would depend on hashes, which makes output and tests
  unstable.

## Consequences

- No new diagnostic codes: E0330 covers missing `Eq`, `Ord` and `Hash`, and E0307 covers
  assigning to part of a list element.
- Ranges stop being an error outside `for`.
- The `Comparable` of earlier examples is the prelude's `Ord`.
