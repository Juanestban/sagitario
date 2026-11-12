# 0095. The collections of Rust's std, `Iterable` and `FromIterator` (S2)

- Status: Accepted
- Date: 2026-11-12
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: record 6 of the
  audit of S2), docs/spec/06-control-flow.md (what `for` iterates),
  [ADR-0093](0093-everyday-methods.md) (Q628: "there is no `FromIterator` interface yet") and
  [ADR-0089](0089-display-debug-and-format-specifiers.md) (Q588: the debug form of maps)

## Context

Record 6 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) is the rest of Rust's
`collections`: `HashSet`, `VecDeque`, `BTreeMap`, `BTreeSet` and `BinaryHeap` (`LinkedList` is
out of scope, ⛔). Sagitario has `List` and `Map`, which the compiler knows by name: `for`
iterates over them, `collect()` makes them (Q628), and their debug forms are written by the
compiler. A collection written in Sagitario has none of that, so each new one would need the
compiler to learn it; Rust gives `for` and `collect` to every type through `IntoIterator` and
`FromIterator`. [ADR-0094](0094-conversions-and-defaults.md) made static functions of interfaces
possible, which `FromIterator` needs.

`List` and `Map` departed from Rust's names (`Vec`, `HashMap`); for the collections of this
record, Rust's names are kept, so that code and knowledge move from Rust as they are.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q644 | Names and places | Rust's names. `HashSet<T: Hash>` is in the prelude, beside `Map`, as common as it; `VecDeque<T>`, `BTreeMap<K: Ord, V>`, `BTreeSet<T: Ord>` and `BinaryHeap<T: Ord>` are in a new module `std::collections`, imported with `use std::collections::BTreeMap;` as in Rust. All are classes written in Sagitario, on lists, as `Map` is. Methods keep Rust's names and meanings, unless a row says otherwise; where Rust says `Vec`, Sagitario says `List` (`into_sorted_list`). |
| Q645 | `Iterable` | A prelude interface `Iterable<T> { fn iter(self) -> Iter<T>; }`, Rust's `IntoIterator`: `for x in value` on a type implementing it runs over `value.iter()`, and a bound `T: Iterable<U>` takes any such collection. `List<T>`, arrays and the collections of this record implement it; `Iter<T>` is iterated as today. A `Map` is not `Iterable`: its `iter()` gives `(K, V)` pairs (Q625) where `for` gives its `Entry` values, and both stay. |
| Q646 | `FromIterator` | A prelude interface `FromIterator<T> { fn from_iter(iter: Iter<T>) -> Self; }`. `collect()` makes the type the context expects when it implements `FromIterator` of the elements; `List`, `Map`, `String` and `Result<List<T>, E>` keep the forms of Q628, and the prelude lists them as implementations so that bounds see them. `Type::from_iter(iter)` is called as any static function. A type of the program implements it as any interface. |
| Q647 | `HashSet<T: Hash>` | On a `Map<T, ()>`, so it keeps insertion order, as `Map` does (Rust's order is unspecified, so code written for it holds). `new`, `with_capacity`, `insert(value) -> bool`, `remove(value) -> bool`, `contains`, `get(value) -> Option<T>`, `take(value) -> Option<T>`, `len`, `is_empty`, `clear`, `iter`, `retain(keep)`, `extend(iter)`, `union`, `intersection`, `difference` and `symmetric_difference` (an `Iter<T>` of copies, since Rust's lend both sets), `is_subset`, `is_superset`, `is_disjoint`, and `HashSet::from(List<T>)`. Two sets are equal with the same elements, in any order; a set implements `Eq` always, since its elements are `Hash`. |
| Q648 | `VecDeque<T>` | A ring buffer in a list. `new`, `with_capacity`, `push_back`, `push_front`, `pop_back`, `pop_front`, `front`, `back`, `get(i) -> Option<T>`, `len`, `is_empty`, `clear`, `contains` (`T: Eq`), `iter` (a `DoubleEndedIter<T>`, front to back), `extend`, `retain`, `truncate`, `swap`, `rotate_left`, `rotate_right`, `make_contiguous` is not needed (the list is not lent), `deque[i]` and `deque[i] = v` through `Index` and `IndexMut` (panicking out of bounds, with Rust's message), and `VecDeque::from(List<T>)`. Equal, ordered and hashed by its elements as `List` (Q623), which the compiler gives as it gives `Map` its `Eq`. `std::sync::Channel` keeps its two lists. |
| Q649 | `BTreeMap<K: Ord, V>` | A B-tree, as Rust's, of nodes of 5 to 11 keys. The methods of `Map` with its names (`insert -> Option<V>`, `get`, `contains_key`, `remove -> Option<V>`, `update`, `upsert`, `get_or_insert`, `retain`, `extend`, `iter`, `keys`, `values`, `len`, `is_empty`, `clear`, `BTreeMap::from(List<(K, V)>)`, `map[key]` through `Index`), in the order of the keys, and Rust's ordered ones: `first_key_value` and `last_key_value` (`Option<(K, V)>`), `pop_first`, `pop_last`, and, since ranges are not values, `range(low, high)` (`low` included, `high` not), `range_from(low)` and `range_to(high)`, each a `DoubleEndedIter<(K, V)>`. `for entry in map` gives `Entry` values, as for `Map`. Equal when its values are `Eq`, as `Map`. |
| Q650 | `BTreeSet<T: Ord>` | On a `BTreeMap<T, ()>`: the methods of `HashSet` in order (the operations of sets give their elements in order), with `first`, `last`, `pop_first`, `pop_last`, `range`, `range_from` and `range_to`. Equal, ordered and hashed by its elements. |
| Q651 | `BinaryHeap<T: Ord>` | A binary heap in a list, the largest first, as Rust's: `new`, `with_capacity`, `push`, `pop -> Option<T>`, `peek -> Option<T>`, `len`, `is_empty`, `clear`, `iter` (in no order), `extend`, `retain`, `into_list` and `into_sorted_list` (ascending, as Rust's `into_sorted_vec`), and `BinaryHeap::from(List<T>)`. Since `Reverse` waits for structs with unnamed fields, `BinaryHeap::with_compare(compare: fn(T, T) -> Ordering)` orders by `compare` instead of `Ord`, the largest by it first: `BinaryHeap::with_compare((a, b) => b.compare(a))` gives the smallest first. |
| Q652 | Debug forms | The compiler writes these classes as Rust's derived `Debug` does, not by their fields: a set as `{1, 2}`, a `VecDeque` and a `BinaryHeap` as `[1, 2]` (the heap in the order of its list), a `BTreeMap` as `{"a": 1}`, through their `iter()`. Other classes of `std::collections` keep the structural form. |
| Q653 | The editor | `sag lsp` completes the new types, their methods with their signatures, and `std::collections` after `std::`; the editor's README lists them; snippets for `impl Iterable` and `impl FromIterator`. |
| Q654 | Increments | **1** `Iterable` and `FromIterator`, in `for` and `collect`, and the module `std::collections` (Q645, Q646). **2** `HashSet` (Q647). **3** `VecDeque` (Q648). **4** `BTreeMap` and `BTreeSet` (Q649, Q650). **5** `BinaryHeap` (Q651). The debug forms (Q652) come with each type. Each one with its tests, the spec and the editor, and the rows of the audit marked. |

### Example

```sag
use std::collections::{BTreeMap, BinaryHeap, VecDeque};

fn main() {
  const seen = new HashSet<String>();

  for word in "a b a c".split_whitespace() {
    seen.insert(word);
  }

  const scores: BTreeMap<String, i32> = List::from([("bob", 3), ("ada", 5)]).iter().collect();
  const queue = VecDeque::from(List::from([1, 2, 3]));
  const tasks = BinaryHeap::with_compare((a: i32, b: i32) => b.compare(a));

  queue.push_front(0);
  tasks.extend(List::from([5, 1, 3]).iter());

  for (name, score) in scores.iter() {
    println(f"{name} {score}");     // ada 5, then bob 3
  }

  println(f"{seen:?} {queue:?} {tasks.pop():?}");   // {"a", "b", "c"} [0, 1, 2, 3] Some(1)
}
```

## Alternatives considered

- **Sagitario's own names** (`Set`, `Deque`, `SortedMap`, `SortedSet`, `PriorityQueue`), in the
  style of `List` and `Map`: they say what the type does, not how; but Rust's names carry over
  code, documentation and habits, and were chosen.
- **Every new type in the prelude:** no `use`, but the prelude would take five more names from
  every module. `HashSet` is common enough to be beside `Map`.
- **`for` and `collect` taught each type by the compiler,** as `List` and `Map`: no new
  interfaces, but every collection of a package would stay without them.
- **`IntoIterator` with `into_iter`, Rust's names:** Sagitario has no moves, so "into" would say
  nothing a copy or a share does not; `iter()` is the method every collection already has.
- **A `BinaryHeap` that only takes `Ord`,** as Rust's: the smallest first would need a type with
  its order reversed until `Reverse` exists.
- **`BTreeMap` as a sorted list:** simpler, but inserting is linear; a B-tree keeps Rust's costs.

## Consequences

- `for` and `collect` reach any type of a program through two interfaces.
- `std::collections` is the first module of collections outside the prelude.
- The debug forms of Q652 are, as `Map`'s, written by the compiler for types it knows by name.
- The audit's rows `HashSet`, `BTreeMap, BTreeSet`, `VecDeque` and `BinaryHeap` are done.

## Notes

- Increment 1 is done. The prelude's types are listed as implementations of `Iterable` and
  `FromIterator` without a block (`typed.implemented_refs`), so bounds see them, and lowering
  answers their `iter` and `from_iter` with what `list.iter()` and `collect()` already call;
  `Iter<T>` implements `Iterable<T>` in a block of the prelude. `for` over a type implementing
  `Iterable` calls its `iter()` unless the instance's type is iterated by itself, a list, an
  array or an iterator. `collect()` into another type implementing `FromIterator` calls its
  `from_iter`, as `T::from_iter` does.
- Increment 2 is done. `HashSet` is written in the prelude on a `Map<T, ()>`, with `get_key_value`
  and `remove_entry` added to `Map`, as Rust's, so that `get` and `take` give the value the set
  holds. Its `Eq` is an implementation of the class; the checker lets `==` reach it, as it does
  for a map, instead of comparing instances (`typed.compares_by_value`). Its debug form is
  written by codegen from the entries of its map, as that of a map is.
