# 0094. Conversions, defaults, generic `min` and `max`, and indexing (S2)

- Status: Accepted
- Date: 2026-11-12
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: record 5 of the
  audit of S2), [ADR-0031](0031-interfaces.md) ("there are no static functions in interfaces"),
  [ADR-0081](0081-operator-overloading.md) ("indexing is not overloadable"), docs/spec/07-errors.md
  (`?` through `From` was post-MVP) and [ADR-0093](0093-everyday-methods.md) (Q619:
  `unwrap_or_default` and `mem::take` wait for `Default`)

## Context

Record 5 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)) is Rust's `convert`,
`default` and the rest of `cmp` and `ops`: `From`, `Into`, `TryFrom`, `Default`, the generic
`min` and `max`, and `Index`. Sagitario has none of them. Conversions are named functions
(`List::from`, `String::from_utf8`) or `as`, `?` converts an error only into an interface it
implements or a `String` into `Error`, and nothing gives a type a value to start from.

Three rules of the language stand in the way of Rust's forms:

- `From::from`, `TryFrom::try_from` and `Default::default` take no `self`, and interfaces have
  no static functions (spec 5).
- `Into` is implemented in Rust by one generic `impl<T, U: From<T>> Into<U> for T`, and
  Sagitario has no blanket implementations: an `impl ... for` block names one type of its own
  module.
- `Index::index` returns `&Output` and `IndexMut::index_mut` returns `&mut Output`, and a
  reference in Sagitario is only the type of a parameter
  ([ADR-0079](0079-references-and-borrowing.md), Q488): it cannot be a result, so there are no
  lifetimes.

Sagitario has no `derive` either ([ADR-0089](0089-display-debug-and-format-specifiers.md) gives
every type its debug form instead).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q631 | Static functions in interfaces | An interface may declare functions without `self`: `fn from(value: T) -> Self;`, required, or with a body, a default. A type provides them as it provides methods, as a `pub fn` of the same signature without `self` in its header's body or in an `impl ... for` block, with `Self` replaced by the type. They are called through a type, `Celsius::from(f)`, or through a type parameter bounded by the interface, `T::default()`, which monomorphization resolves; not through the interface (`Default::default()` is an error that asks for the type), since there is no value to dispatch on. An interface with static functions, itself or through a parent, cannot be used as a type, as one that uses `Self` (spec 5). Static functions of interfaces are not inherited by subclasses, as the static functions of classes (spec 4). |
| Q632 | `From` | A prelude interface `From<T> { fn from(value: T) -> Self; }`. A type implements it once per source type, in its header or in blocks ([ADR-0081](0081-operator-overloading.md)), in the module that declares it (`E0343`), so the block is written beside the target, as Rust's orphan rule allows for local types. `U::from(x)` chooses the implementation whose `T` is the type of `x`, as an operator chooses by its `Rhs`: an unsuffixed literal takes the one implementation of its kind of number, and otherwise the type of `x` must match one `T` exactly. The prelude implements Rust's lossless conversions: each integer from the narrower integers of the same signedness and from the narrower unsigned ones (`i64::from(5i32)`, `u32::from(7u8)`, `i32::from(7u8)`), except that `usize` and `isize` take only those of 8 and 16 bits, whose width every target holds, `f64` from `f32` and from the integers of up to 32 bits, `f32` from those of up to 16 bits, `u32` and `u64` from `char`, `char` from `u8`, `String` from `char`, and `List<T>` from `[T; N]` (the `List::from` there is today, which becomes this implementation). `Map::from` keeps its list of pairs, as an implementation of `From<List<(K, V)>>`. Every type `T` is `From<T>`, as Rust's reflexive implementation, without a block. |
| Q633 | `Into` | A prelude interface `Into<U> { fn into(self) -> U; }` the compiler implements: a type `T` implements `Into<U>` when `U` implements `From<T>`, `x.into()` is `U::from(x)`, and `U` comes from the type expected, as `collect` (Q628), or from a bound. Programs do not implement `Into` in a header or a block (an error that asks for `From`), so the two cannot disagree. A bound `T: Into<String>` accepts every type `String` is `From`, and `String` itself. |
| Q634 | `TryFrom` and `TryInto` | `TryFrom<T, E> { fn try_from(value: T) -> Result<Self, E>; }`, with the error as a type argument, as `Add<Rhs, Out>` has its result: interfaces have no associated types. `TryInto<U, E>` is given by the compiler from it, as `Into` from `From`. The prelude implements, between every two integer types without a lossless `From`, `TryFrom<A, String>`, whose error is Rust's message, `out of range integral type conversion attempted`, and `char` from `u32` (`converted integer out of range for `char``). A `From` does not give a `TryFrom`: Rust's blanket implementation with an error that never happens needs a never type. |
| Q635 | `?` | `Err(e)` of a type `X` becomes the error `E` of the function through `E::from(e)` when `E` implements `From<X>`, as Rust's `?`. The two conversions there are (an error into an interface it implements, a `String` into `Error`) stay and are tried first; with neither nor a `From`, the error is as today, and its `help` now also says to implement `From<X>` for `E`. |
| Q636 | `Default` | A prelude interface `Default { fn default() -> Self; }`. Numbers are `0` or `0.0`, `bool` `false`, `char` `'\0'`, `String` `""`, `List`, `Map` and `StringBuilder` empty, `Option` `None`, and tuples and arrays the default of each element when they all have one, as Rust's. A struct or enum implements it by hand, or with the attribute `@default` before its declaration, as Rust's `#[derive(Default)]`: on a struct, every field takes its default, and each must have one (an error that names the field); on an enum, `@default` is also written before the variant without fields that is the default, and a variant must be marked. A class implements it by hand, since its instances are built by `new`. `@default` takes no arguments. |
| Q643 | Struct update | As Rust's, a struct literal may end with `..base`, an expression of the same struct type, after the fields written and a comma: `Config { verbose: true, ..Config::default() }`. The fields not written are those of `base`, which is evaluated after the fields written, once; a literal with `..base` may write no field (`Config { ..other }`), and writing every field with `..base` is allowed, as in Rust, with `base` still evaluated. Every field left to `base` must be visible where the literal is, as for the fields written (spec 3: a struct with `prv` fields is built from outside through a static function). It is only for structs, not for the variants of enums with fields, as in Rust. `..` is the token of ranges; after the fields of a literal it is always the base. |
| Q637 | With `Default` | `Option::unwrap_or_default()` and `Result::unwrap_or_default()` (`T: Default`), `mem::take(&mut place) -> T` (`T: Default`), and, without it, `mem::replace(&mut place, value) -> T` and `mem::swap(&mut a, &mut b)`, as Rust's, which Q619 named. |
| Q638 | `min`, `max` and `Ordering` | Prelude functions `min<T: Ord>(a: T, b: T) -> T` and `max`, which give `a` when the two are equal for `min` and `b` for `max`, as Rust's, and `clamp(self, low, high)` on every `Ord` type (a default method of `Ord`), which panics when `low > high`. `Ordering` gains `reverse()`, `then(other)`, `then_with(f)`, `is_eq`, `is_ne`, `is_lt`, `is_gt`, `is_le` and `is_ge`. `Reverse` waits for structs with unnamed fields; `sort_by((a, b) => b.compare(a))` stands for it. Floats keep their own `min` and `max` and are not `Ord`. |
| Q639 | `Index` and `IndexMut` | `Index<Idx, Out> { fn index(self, index: Idx) -> Out; }` and `IndexMut<Idx, Out> extends Index<Idx, Out> { fn index_set(self, index: Idx, value: Out); }`, in the prelude, with Rust's names. Since a reference cannot be a result (ADR-0079, Q488), `index` gives the value (a copy of a struct, the same instance of a class) and the compiler writes the rest: on a struct, enum, class or type parameter `a`, `a[i]` is `a.index(i)`; `a[i] = v` is `a.index_set(i, v)`; and `a[i] += v`, `a[i].x = v` and `a[i][j] = v` read with `index`, change that value and write it back with `index_set`, with `a` and `i` evaluated once, left to right. Assigning needs an `IndexMut`, as in Rust (`E0310`, naming it). As for operators, the implementation is chosen by the type of `i`, and a type may implement `Index` once per index type. Lists, arrays, slices and strings keep their own indexing and implement neither; `Map` gains `Index<K, V>`, which panics without the key (`no entry found for key`), as Rust's. |
| Q640 | `AsRef` and the rest of `convert` | `AsRef` and `AsMut` lend a view, which needs references as results: ➖, a `&T` parameter does their work. `FromStr` stays as the `parse_*` methods; `str::parse::<T>()` waits for a record on parsing. |
| Q641 | The editor | `@default` and `..base` in the grammar and the completion of attributes; snippets for `impl From<T> for` and `impl Default for`; `sag lsp` completes static functions of interfaces on a type and on a bounded type parameter, `into`, `try_into`, `unwrap_or_default` and the methods of `Ordering`; the editor's README lists them. |
| Q642 | Increments | **1** Static functions in interfaces (Q631). **2** `From`, `Into`, `TryFrom`, `TryInto`, the conversions of the prelude and `?` (Q632–Q635). **3** `Default`, `@default`, `unwrap_or_default` and `std::mem` (Q636, Q637), and struct update (Q643). **4** `min`, `max`, `clamp` and `Ordering` (Q638). **5** `Index` and `IndexMut` (Q639). Each one with its tests, the spec and the editor, and the rows of the audit marked. |

### Example

```sag
struct Celsius { pub degrees: f64; }
struct Fahrenheit { pub degrees: f64; }

impl From<Celsius> for Fahrenheit {
  fn from(value: Celsius) -> Fahrenheit { Fahrenheit { degrees: value.degrees * 1.8 + 32.0 } }
}

enum AppError { Parse(String), Io(String) }

impl From<String> for AppError {
  fn from(message: String) -> AppError { AppError::Parse(message) }
}

@default
struct Config {
  pub name: String;
  pub retries: u32;
  pub verbose: bool;
}

struct Grid { prv cells: List<i32>; prv width: usize; }

impl Index<(usize, usize), i32> for Grid {
  fn index(self, at: (usize, usize)) -> i32 { self.cells[at.1 * self.width + at.0] }
}

impl IndexMut<(usize, usize), i32> for Grid {
  fn index_set(self, at: (usize, usize), value: i32) { self.cells[at.1 * self.width + at.0] = value; }
}

fn parse(text: String) -> Result<i32, AppError> {
  Ok(text.parse_i32()?)                        // String into AppError through From
}

fn main() {
  const hot: Fahrenheit = Celsius { degrees: 100.0 }.into();
  const config = Config { verbose: true, ..Config::default() };
  const small = u8::try_from(300i32);          // Err("out of range integral type conversion attempted")
  var grid = Grid { cells: List::from([0, 0, 0, 0]), width: 2 };

  grid[(1, 1)] += 5;
  println(f"{hot.degrees} {config.retries} {small:?} {grid[(1, 1)]} {max(3, 7)}");   // 212 0 Err(..) 5 7
}
```

## Alternatives considered

- **Conversions without static functions** (`value.into()` alone, and `Default` known only to
  the compiler): no change to interfaces, but `T::default()` in generic code, the most common use
  of `Default`, could not be written, and `FromIterator` or `FromStr` later would meet the same
  wall.
- **`Into` implemented by programs, as in Rust:** two implementations could disagree with the
  `From` they mirror, and Rust's own advice is to implement `From`; giving it from `From` is
  what Rust's blanket implementation does.
- **No `Into`:** simpler, but `T: Into<String>` parameters, common in Rust's APIs, would be lost.
- **`Default` inferred for every struct whose fields have one,** as `Send`: nothing to write,
  but it gives a starting value to types whose author never chose one (a `Port` of `0`), and
  changing a field would remove it silently.
- **`Default` by hand only:** no new attribute, but a struct of many fields repeats each of
  them.
- **`index_mut` with a closure** (`fn index_mut(self, i: Idx, f: fn(&mut Out))`): no copy, closer
  to Rust's `&mut`, but it needs lambdas with `&mut` parameters, and every assignment would
  become a closure.
- **References as results with lifetimes,** for Rust's `index` and `index_mut` exactly: the
  most faithful, but it is the larger record ADR-0079 left for later, and it changes the borrow
  checker and the model of memory. If it comes, a record may replace `index_set` with it.
- **Struct update in a record of its own:** it is syntax, but it is how Rust programs use
  `Default`, so it comes with it.
- **Only `Index`, assignment through named methods:** smaller, but `grid[i] = v` is what Rust
  programs write.

## Consequences

- Interfaces gain static functions, which `FromIterator`, `FromStr` and others can use later.
- `?` converts errors as Rust's does, so a program's error enum gathers the errors of its
  libraries with one block each.
- `@default` is the first attribute that makes code; `@repr`, `@cfg`, `@export` and
  `@thread_local` only change how code is built.
- `a[i].x = v` on a user type copies the element out and back, where Rust writes in place: the
  same result, at the cost of a copy of a struct, which the C compiler often removes.
- `List::from` and `Map::from` become implementations of `From`, with the same calls.
- The audit's rows `From, Into, TryFrom, AsRef`, `Default`, `cmp::min, max, Reverse, then_with`
  and `Index` are done or out of scope, and the language gap of conversion interfaces is closed.

## Notes

- Increment 1 is done. The resolver gives `T::name`, `Self::name` inside an interface, and
  `Type::name` for a function the type does not declare but a block or an interface does, a
  `Res::InterfaceFn` with the type and the name; the checker finds the interface among those the
  type implements or is bounded by (`types::interfaces::static_call`) and records the call with
  its `Self`, and lowering, once the instance knows that type, calls the function of the type's
  own block, its own static function, or the default instantiated for it, as it does for the
  methods of interfaces. Making the check of interfaces used as types reach the types written in
  function bodies was needed too: it saw only declarations, and an interface with static
  functions as the type of a binding compiled to an itable that called them with a receiver.
- Increment 2 is done. One relation serves `U::from`, `U::try_from`, `into`, `try_into` and `?`:
  the checker records, for the call or the `?`, the type converted, the type made and, for
  `TryFrom`, the error (`typed::Conversion`); lowering makes it the value itself, a cast or a
  function of the prelude for its conversions, or a call of the `from` or `try_from` of the
  type, found as any static function of an interface. The prelude's conversions are the
  implementations primitives list (`typed::builtin_from`), so bounds see them; `Into`,
  `TryInto` and reflexivity are answered by `implements` without being listed, since they are
  as many as the types. The `try_from` between integers are written by the loader, one per pair,
  as the ranges are. `List<T>` is `From<[T; N]>` and `Map<K, V>` `From<List<(K, V)>>`, with the
  calls `List::from` and `Map::from` had. A function a type declares in its body that one of its
  blocks declares too, as `from` of two instantiations of `From`, is resolved by the checker, so
  that the argument chooses.
- Increment 3 is done. A struct or enum written after `@default` implements `Default` without a
  block: the checker lists it among the type's interfaces (`typed.derived_defaults`, and the
  variant an enum marks), and lowering builds the value, as it builds those of the prelude's
  types (`Lowering::default_value`): a constant, an empty collection, `None`, the defaults of the
  parts of a tuple, an array or a struct, or the marked variant; a type that implements
  `Default` by hand is called. `unwrap_or_default` is written in the prelude, and `take`,
  `replace` and `swap` in `std::mem`, on `&mut` parameters. `..base` reads the fields not written
  from the place of the base, once it is evaluated. In an enum, attributes start a variant only
  when a name follows them, so those of the functions after the variants stay theirs.
- Increment 4 is done. `min`, `max` and the methods of `Ordering` are written in the prelude, and
  `clamp` is a default method of `Ord`, which lowering compiles for each type as other default
  methods; for numbers, `bool`, `char` and `String`, it compiled only `equals`, `compare` and
  `hash` as their own operations, and now leaves the other methods of those interfaces to their
  defaults. Integers and floats keep their `clamp`, which reports the line of its call; that of
  `Ord`, as Rust's, panics in the prelude.
