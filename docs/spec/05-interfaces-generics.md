# 5. Interfaces and generics (F2)

## Interfaces

Rationale: [ADR-0031](../adr/0031-interfaces.md).

```sag
interface Named {
  var name: String;                                                // required field
  fn display_name(self) -> String;                                 // required method
  fn greet(self) -> String { `Hello, ${self.display_name()}` }    // default method
}

interface Pet extends Named {
  fn owner(self) -> String;
}

class Dog impl Pet {
  pub var name: String;

  pub fn new(self, name: String) { self.name = name; }
  pub fn display_name(self) -> String { self.name }
  pub fn owner(self) -> String { "Ada" }
}
```

- Like a TypeScript interface, an interface describes the fields and methods a type must have.
  Its members take no `pub`/`prv`: they are public.
- **Fields** are declared `const` (they can only be read through the interface) or `var` (they
  can also be assigned through it).
- **Methods** take `self`. A method without body is required. A method with a body is a
  default, used by implementing types that do not define it. An interface may also declare
  [static functions](#static-functions), without `self`.
- `interface A extends B, C` composes interfaces; `extends` cannot loop, and an interface cannot
  redeclare a method or field of a parent.
- `class`, `struct` and `enum` implement interfaces with the same form:
  `class A extends B impl I1, I2`, `struct P impl I1`, `enum E impl I1`.
- A type implementing an interface (and its ancestors) provides:
  - for each field, a `pub` field with the same name and type; in a class, a `var` field when
    the interface's is `var`. Enums have no fields, so they only implement interfaces without
    fields.
  - for each method or static function without a default, a `pub fn` with the same name and
    signature, with `Self` replaced by the type. Defining one that has a default replaces the
    default. These methods do not take `override`, which is for class inheritance.
- A subclass inherits its parent's interfaces, and inherited fields and methods count. An
  `abstract` class may leave required methods to its concrete subclasses.
- Inside an interface, `Self` names the implementing type:

```sag
interface Clone {
  fn clone(self) -> Self;
}
```

- In a default method, `self` has type `Self`: the implementing type, bounded by the interface,
  whose fields and methods it reaches. Default methods are compiled for each implementing type.

### `impl ... for` blocks

Rationale: [ADR-0081](../adr/0081-operator-overloading.md), increment 1. Besides its header, a
type implements an interface in a block at module level, as in Rust. Each block holds the methods
of one interface, so a type can implement a generic interface several times, with different type
arguments:

```sag
interface Scale<By, Out> {
  fn scale(self, by: By) -> Out;
}

struct Vec2 {
  pub x: f64;
  pub y: f64;
}

impl Scale<f64, Vec2> for Vec2 {
  fn scale(self, by: f64) -> Vec2 { Vec2 { x: self.x * by, y: self.y * by } }
}

impl Scale<Vec2, Vec2> for Vec2 {
  fn scale(self, by: Vec2) -> Vec2 { Vec2 { x: self.x * by.x, y: self.y * by.y } }
}
```

- A block is written in the module that declares its type, a struct, enum or class (`E0343`).
  Neither the block nor its methods take `pub` or `prv`: the methods are public, and they reach
  the private members of the type as its own methods do.
- A block holds the methods of its interface and of the interface's parents, and nothing else
  (`E0329`); methods with a default may be left out. The fields of the interface are fields of the
  type, as for the header.
- A generic type writes its parameters after `impl` and after the type, in order and without
  bounds, which are the type's: `impl<T> Named for Box<T>` (`E0343`). The names may differ from
  the type's. A block cannot implement an interface only for some arguments (`Box<i32>`).
- The header names each interface once; another instantiation of a generic interface goes in a
  block. Implementing the same instantiation twice, in the header or in blocks, is an error
  (`E0342`).
- A method of a block is reached through a value of the interface, through a type parameter
  bounded by it, and by name when the type has no other method of that name
  (`v.scale(2.0)` above is ambiguous, `E0344`). A default method of an interface the type
  implements twice is ambiguous by name too.
- A subclass inherits the blocks of its classes. Their methods are not virtual: a subclass does
  not override them.

### Static functions

Rationale: [ADR-0094](../adr/0094-conversions-and-defaults.md), Q631. A function without `self`
in an interface is a static function of the types implementing it, as Rust's associated
functions of traits:

```sag
interface Zero {
  fn zero() -> Self;
  fn label() -> String { "zero of " + Self::kind() }   // a default
  fn kind() -> String;
}

struct Point impl Zero {
  pub x: i32;
  pub y: i32;

  pub fn zero() -> Point { Point { x: 0, y: 0 } }
  pub fn kind() -> String { "Point" }
}

fn zeros<T: Zero>() -> (T, String) {
  (T::zero(), T::label())
}

const origin = Point::zero();
const pair: (Point, String) = zeros();   // (Point { x: 0, y: 0 }, "zero of Point")
```

- A type provides each one as its methods: a `pub fn` without `self` in its body, or a `fn` in an
  `impl ... for` block, with `Self` replaced by the type (`E0329` when it is missing, takes
  `self`, or is private).
- It is called through a type: `Point::zero()`, `Self::kind()` inside the interface or the
  type, `Wrap<i32>::zero()` with the type's arguments written or inferred from the expected
  type, and `T::zero()` for a type parameter bounded by the interface, whose instance calls the
  implementation of its type argument. Through the interface itself (`Zero::zero()`) it is an
  error (`E0314`): no value chooses the implementation. A static function declared by two
  interfaces of a type parameter is ambiguous (`E0344`).
- It is not a method: `point.zero()` is an error (`E0314`), and so is using `T::zero` as a value
  (`E0314`); a lambda wraps it, `() => T::zero()`.
- A subclass does not inherit the static functions of the interfaces of its classes, nor those of
  their blocks, as the static functions of classes: a concrete subclass declares its own, unless
  the interface gives a default.

## Interfaces as types

An interface can be used as a type (`const shapes: [Shape; 2] = [square, circle];`). Such values
are references with their type's methods and use **dynamic dispatch**, like interface types in
Java/C# or `dyn Trait` in Rust, but without extra syntax.

- A class, struct or enum converts implicitly to an interface it implements, and an interface
  to its ancestors, wherever that type is expected (arguments, assignments, results, fields,
  elements, `Some(...)`, the branches of `if` and `match`). There is no conversion back.
- A class instance becomes an interface value as a new reference to the same instance. A struct
  or enum is **copied** into a box: the interface value refers to the copy.
- Interface values are references: copying one shares the instance or box, and assigning a
  `var` field through it changes that instance or box.
- Interface values cannot be compared with `==` in the MVP.
- An interface that uses `Self` in a parameter, result or field, itself or through a parent,
  that has methods with type parameters, or that has static functions, cannot be used as a type,
  in a declaration or inside a function body (e.g. `[Clone; 2]` is an error, `E0329`): the value
  could not know the concrete type, the instances or the implementation. It can still be a bound.

## The `Error` interface (F2)

```sag
interface Error {
  fn message(self) -> String;
}
```

- `Error` is in the prelude. Error types implement it (`struct ParseError impl Error`).
- `Error("text")` creates a basic error whose `message()` is `text`:
  `return Err(Error("file not found"));` in a function returning `Result<T, Error>`.
- `Result<T, E>` admits any error type, `String` included, but `String` does not implement
  `Error`. See [errors](07-errors.md#the--operator) for how `?` converts errors.
- `Display` is a separate prelude interface, for the text of a value
  ([ADR-0089](../adr/0089-display-debug-and-format-specifiers.md)); `Error` does not extend it.
- Post-MVP: `source()` for error chains and backtraces.

## Generics

```sag
fn max<T: Ord>(a: T, b: T) -> T { ... }

fn describe<T, U>(a: T, b: U) -> String
where
  T: Named + Display,
  U: Display,
{ ... }

class Box<T> {
  pub const value: T;
  pub fn new(self, value: T) { self.value = value; }
}
```

Rationale: [ADR-0032](../adr/0032-generics.md).

- Type parameters go in `<...>` on functions, methods, classes, structs, enums and interfaces.
- Bounds are interfaces: `T: I1 + I2`. Long bounds may move to a `where` clause, whose bounds
  join those after `:`.
- **Type arguments** are written TypeScript-style after a name: `max<i32>(a, b)`,
  `new Box<i32>(1)`, `Pair<i32, String>::new(1, "a")`, `Maybe<String>::Nothing`, and in types
  `Box<i32>`. A generic type always takes its arguments in a type (`Box` alone is an error).
- **Inference** is local: type arguments omitted in a call, `new` or literal come from the
  arguments (or fields) and the expected type: `const b: Box<i32> = new Box(1);`. If one stays
  unknown, the compiler asks for it. Inside a generic type, `Self::f()` keeps the type's own
  arguments.
- Generic code is checked once, where it is written. Without bounds a `T` can be stored,
  copied, passed, returned and put inside other types. Its bounds add their methods and fields.
  There is no `==`, interpolation or arithmetic on an unbounded `T`; bounded by `Eq`, `Ord` or
  an [operator interface](#operators), `T` has the operators they give.
- A type argument must implement the bounds of its parameter. Classes, structs and enums
  implementing an interface satisfy it, and so do the interface type itself and type parameters
  bounded by it. Integers, `bool`, `char` and `String` implement the prelude's `Eq`, `Ord` and
  `Hash`, and no other interface; tuples, `Option` and `Result` implement each of them when all
  the values they hold do.
- In a call, a lambda argument whose parameter types are known infers type parameters from its
  result: `iter.map(n => n * 2)` infers the `U` of `map<U>(f: fn(T) -> U)`.
- Generic interfaces (`interface Source<T>`) can be implemented several times by one type, with
  different arguments, in [`impl ... for` blocks](#impl--for-blocks), and, instantiated, can be
  types (`Source<i32>`). Generic classes extend and are extended with arguments
  (`class IntBox extends Box<i32>`). There is no variance: `Box<Dog>` is not a `Box<Animal>`.
- Methods with their own type parameters cannot be overridden or be abstract.
- Generics are **monomorphized**: each concrete instantiation produces its own specialized
  code. There is no boxing and no runtime type information for generics. Generic code whose
  instances would keep growing without end is an error.
- Not in the MVP: higher-kinded types, specialization, variadic generics, default type
  arguments, constant generics.

## `Send` (F4)

Rationale: [ADR-0045](../adr/0045-threads-and-send.md).

`Send` is the prelude's interface of the values that can cross to another thread: those no
other thread can change. Sagitario has no move, so a value given to a thread is shared with
the thread that gave it; `Send` is the only marker, and `Sync` stays reserved.

- The compiler infers it; no type names it after `impl`, no interface extends it, and it is not
  a type (`const x: Send` is an error). It is a bound: `fn f<T: Send>(value: T)`.
- These types are `Send`: integers, floats, `bool`, `char`, `()`, `String`, `CString`; arrays,
  tuples, `Option` and `Result` of `Send` types; structs and enums whose fields are all `Send`; type
  parameters bounded by `Send`; classes whose fields, their own and inherited, are all
  `const` and `Send`, if every subclass is `Send` as well (a reference to a class can refer to
  an instance of a subclass); and the classes of `std::sync` (`Mutex<T>` when `T` is `Send`),
  which guard their own state.
- These are not: `List`, `Map` and other classes with a `var` or `weak` field, `StringBuilder`,
  function values, raw pointers and interface values.
- `thread::spawn` and `Mutex<T>` ask for `Send` values
  ([threads](11-standard-library.md#stdthread), [sync](11-standard-library.md#stdsync)).

## Equality, order and hashing (F2)

Rationale: [ADR-0034](../adr/0034-collections-and-iterators.md).

```sag
enum Ordering { Less, Equal, Greater }

interface Eq {
  fn equals(self, other: Self) -> bool;
}

interface Ord extends Eq {
  fn compare(self, other: Self) -> Ordering;
}

interface Hash extends Eq {
  fn hash(self) -> u64;
}
```

- The four are in the prelude. Integers, `bool`, `char` and `String` implement the three
  interfaces, with their usual equality and order (strings by bytes): `5.compare(3)` is
  `Ordering::Greater`.
- Floats implement none of them, since `NaN` is not equal to itself. `x.total_compare(y)`
  orders them by the IEEE 754 total order (`-NaN < -inf < ... < -0.0 < 0.0 < ... < inf < NaN`).
- Programs implement them like any interface: `struct Version impl Ord` with `equals` and
  `compare`. Equal values must have the same `hash`.
- `==` and `!=` call `equals` on structs, enums and type parameters implementing `Eq`, and `<`,
  `<=`, `>`, `>=` use `compare` on those implementing `Ord`: `a < b` is
  `a.compare(b) == Ordering::Less`, `a <= b` is `a.compare(b) != Ordering::Greater`
  ([ADR-0081](../adr/0081-operator-overloading.md), Q514). On class instances `==` stays identity,
  even when the class implements `Eq`; floats keep their IEEE comparisons. A comparison of a type
  without the interface is an error (`E0310`) naming it.
- `List::contains`, `List::index_of` and `List::sort` need `Eq` or `Ord`, and the keys of a
  `Map` need `Hash`.
- Rust's helpers of `cmp` ([ADR-0094](../adr/0094-conversions-and-defaults.md), Q638):
  - the prelude functions `min<T: Ord>(a: T, b: T) -> T` and `max`, which give `a` when the
    two are equal for `min` and `b` for `max`; floats, which are not `Ord`, have their own
    `min` and `max` methods;
  - `clamp(self, low: Self, high: Self) -> Self`, a default method of `Ord`: the value restricted
    to `low..=high`, panicking unless `low <= high` (`` `clamp` needs min <= max ``); numbers
    have their own `clamp`;
  - the methods of `Ordering`: `reverse()`, `then(other)` (this order, or `other` when it is
    `Equal`), `then_with(f)` (the same, calling `f` only then), and `is_eq`, `is_ne`, `is_lt`,
    `is_gt`, `is_le`, `is_ge`. `sort_by((a, b) => b.compare(a))` sorts in reverse, which Rust's
    `Reverse` does.

## Operators

Rationale: [ADR-0081](../adr/0081-operator-overloading.md), increment 2. The prelude declares
one interface per arithmetic operator, whose type arguments are the type of the right operand and
the type of the result:

```sag
interface Add<Rhs, Out> { fn add(self, other: Rhs) -> Out; }   // a + b
interface Sub<Rhs, Out> { fn sub(self, other: Rhs) -> Out; }   // a - b
interface Mul<Rhs, Out> { fn mul(self, other: Rhs) -> Out; }   // a * b
interface Div<Rhs, Out> { fn div(self, other: Rhs) -> Out; }   // a / b
interface Rem<Rhs, Out> { fn rem(self, other: Rhs) -> Out; }   // a % b
interface Neg<Out> { fn neg(self) -> Out; }                     // -a
```

```sag
impl Mul<Mat4, Mat4> for Mat4 { fn mul(self, other: Mat4) -> Mat4 { ... } }
impl Mul<Vec4, Vec4> for Mat4 { fn mul(self, v: Vec4) -> Vec4 { ... } }

const clip = projection * view * model * position;   // Mat4 * Mat4 * Mat4 * Vec4
```

- On a struct, enum, class, interface value or type parameter `a`, `a + b` calls the `add` of the
  implementation of `Add` by the type of `a` whose `Rhs` is the type of `b`, and has the type of
  its `Out`. Only the type of the left operand is searched: `v * 2.0` can be implemented,
  `2.0 * v` is an error (`E0310`).
- When the type of `a` implements the interface once, `b` is checked against its `Rhs`, so an
  unsuffixed literal takes that type. With several implementations, an unsuffixed literal takes
  the one implementation of its kind of number, and other operands must have one `Rhs` exactly.
  An operator without an implementation for the operands is an error (`E0310`) that names the
  interface to implement, or the right operands the type accepts. `-a` with several
  implementations of `Neg` is ambiguous (`E0344`).
- `a += b` (and `-=`, `*=`, `/=`, `%=`) is `a = a + b`, with `a` evaluated once; the `Out` of the
  implementation must be the type of `a` (`E0310`). There is no `AddAssign`.
- Operands are evaluated left to right and passed by value: a method of the operator that
  modifies `self` changes a copy of the left operand.
- Integers and floats implement the five binary interfaces with their own type as `Rhs` and
  `Out`, signed integers and floats implement `Neg<Self>`, and `String` implements
  `Add<String, String>`. Their operators keep their meaning (overflow and division by zero), and
  generic code uses them through bounds: `fn sum<T: Add<T, T>>(a: T, b: T) -> T { a + b }`
  works for `i32`, `f64`, `String` and a `Vec2` implementing `Add<Vec2, Vec2>`. As with `Ord`,
  numbers and strings satisfy these bounds but do not convert to values of the interfaces.
- `&&`, `||`, `!`, the bitwise operators and shifts are not overloadable; comparisons go through
  [`Eq` and `Ord`](#equality-order-and-hashing-f2), and indexing through
  [`Index` and `IndexMut`](#indexing).
- A block for an interface that extends another one the type implements itself, such as
  `impl Ord for P` beside `impl Eq for P`, holds only its own methods.

## Iteration and collection

Rationale: [ADR-0095](../adr/0095-collections.md), Q645 and Q646. Two prelude interfaces give
`for` and `collect()` to any type, as Rust's `IntoIterator` and `FromIterator`:

```sag
interface Iterable<T> { fn iter(self) -> Iter<T>; }
interface FromIterator<T> { fn from_iter(iter: Iter<T>) -> Self; }

class Bag impl Iterable<i32>, FromIterator<i32> {
  pub const items: List<i32>;

  pub fn new(self, items: List<i32>) { self.items = items; }
  pub fn iter(self) -> Iter<i32> { self.items.iter() }
  pub fn from_iter(iter: Iter<i32>) -> Bag { new Bag(iter.collect()) }
}

const bag: Bag = (1..4).collect();   // Bag::from_iter
for n in bag { ... }                 // over bag.iter()
```

- `for x in value` runs over `value.iter()` when the type of `value` implements `Iterable<T>`,
  and binds `x` to each `T`. Lists, arrays and iterators implement it, so a bound
  `T: Iterable<i32>` takes them; a `Map` does not, since its `iter()` gives pairs and `for` gives
  its entries.
- `collect()` makes the type expected when it implements `FromIterator` of the elements, through
  its `from_iter`; lists, maps, strings (of `char` or `String`) and `Result<List<T>, E>`
  implement it, as [`collect`](08-closures.md#iterators) makes them, so a bound
  `C: FromIterator<i32>` takes them, and `C::from_iter(iter)` is called as any static function.

## Indexing

Rationale: [ADR-0094](../adr/0094-conversions-and-defaults.md), Q639. The prelude declares Rust's
`Index` and `IndexMut`. A reference cannot be a result
([ADR-0079](../adr/0079-references-and-borrowing.md)), so `index` gives the value, and the
compiler writes a changed element back with `index_set`:

```sag
interface Index<Idx, Out> { fn index(self, index: Idx) -> Out; }
interface IndexMut<Idx, Out> extends Index<Idx, Out> { fn index_set(self, index: Idx, value: Out); }

struct Grid { pub cells: List<i32>; pub width: usize; }

impl Index<(usize, usize), i32> for Grid {
  fn index(self, at: (usize, usize)) -> i32 { self.cells[at.1 * self.width + at.0] }
}

impl IndexMut<(usize, usize), i32> for Grid {
  fn index_set(self, at: (usize, usize), value: i32) { self.cells[at.1 * self.width + at.0] = value; }
}

grid[(1, 1)] = 5;    // grid.index_set((1, 1), 5)
grid[(1, 1)] += 2;   // the element read with `index`, changed, and written back
```

- On a struct, enum, class, interface value or type parameter `a`, `a[i]` calls the `index` of the
  implementation of `Index` whose `Idx` is the type of `i`, and has the type of its `Out`; a type
  may implement `Index` once per index type, chosen as the conversions of `From` are. Without one
  it is an error (`E0310`).
- `a[i] = v` calls `index_set(i, v)`. `a[i] += v`, `a[i].x = v`, `a[i][j] = v` and other
  assignments through `a[i]` read the element with `index` into a copy, assign to the copy, and
  write it back with `index_set`; `a` and `i` are evaluated once, before the value. Assigning
  through an index needs the `IndexMut` with the same arguments (`E0310`), and its `index_set` is
  a method of `a`: when it modifies `self`, a struct or enum `a` must be `var` (`E0313`), as for
  any such method.
- `index` gives a copy of a struct and the same instance of a class, so a change through a method
  that modifies `self`, or `&mut a[i]`, would be lost: both are errors (`E0313`, `E0340`).
- Lists, arrays, slices and strings keep their own indexing, and implement neither interface. A
  `Map<K, V>` implements `Index<K, V>`, which panics without the key.

## Conversions

Rationale: [ADR-0094](../adr/0094-conversions-and-defaults.md), Q632–Q634. The prelude declares
Rust's conversion interfaces, with the error of `TryFrom` as a type argument:

```sag
interface From<T> { fn from(value: T) -> Self; }
interface Into<U> { fn into(self) -> U; }
interface TryFrom<T, E> { fn try_from(value: T) -> Result<Self, E>; }
interface TryInto<U, E> { fn try_into(self) -> Result<U, E>; }
```

```sag
struct Celsius { pub degrees: f64; }
struct Fahrenheit { pub degrees: f64; }

impl From<Celsius> for Fahrenheit {
  fn from(value: Celsius) -> Fahrenheit { Fahrenheit { degrees: value.degrees * 1.8 + 32.0 } }
}

const hot = Fahrenheit::from(Celsius { degrees: 100.0 });   // 212
const also: Fahrenheit = Celsius { degrees: 0.0 }.into();    // 32
const small = u8::try_from(300i32);                         // Err("out of range integral type conversion attempted")
```

- A type implements `From<T>` once per source type, in its header or in
  [blocks](#impl--for-blocks), in the module that declares it. `U::from(x)` calls the
  implementation whose `T` is the type of `x`; with one, `x` is checked against its `T`, and with
  several, an unsuffixed literal takes the one `T` of its kind of number (`AppError::from(5)` with
  `From<i32>` and `From<String>`). A type that implements none for `x` is an error (`E0330`) that
  names the sources it has. `TryFrom<T, E>` is chosen the same way.
- Every type is `From` itself: `U::from(u)` gives `u`. A `From` gives no `TryFrom`.
- `Into<U>` and `TryInto<U, E>` are given by the compiler: `T` implements `Into<U>` when `U`
  implements `From<T>`, and `TryInto<U, E>` when `U` implements `TryFrom<T, E>`. Programs
  implement `From` and `TryFrom` instead; implementing `Into` or `TryInto` is an error (`E0329`).
- `x.into()` converts into the type expected (`const f: Fahrenheit = c.into();`), or into the
  `U` of a bound `T: Into<U>` of the type of `x`; without either it is an error (`E0311`), and so
  is a type that does not convert (`E0330`). `x.try_into()` expects a `Result<U, E>`, or a bound
  `TryInto<U, E>`; without one, its error is `String`, that of the prelude's conversions. A type
  that declares a method `into` or `try_into` keeps it.
- Bounds use them as Rust's: `fn label<T: Into<String>>(value: T) -> String` accepts `char` and
  `String`, and `fn make<T: From<i32>>(n: i32) -> T { T::from(n) }` every type with that
  `From`.
- The prelude implements Rust's conversions between numbers, characters and strings:

  | Into | `From` | `TryFrom<_, String>` |
  |---|---|---|
  | an integer | the narrower integers of its signedness and the narrower unsigned ones; `isize` and `usize` only those of 8 and 16 bits | every other integer |
  | `u32`, `u64` | also `char` | |
  | `f64` | `f32`, and integers of up to 32 bits except `isize` and `usize` | |
  | `f32` | integers of up to 16 bits | |
  | `char` | `u8` | `u32` |
  | `String` | `char` | |

  `try_from` between integers fails when the value does not fit, with Rust's message `out of
  range integral type conversion attempted`, and `char::try_from` with `converted integer out of
  range for `char``. They are called as `i64::from(x)`, `u8::try_from(x)`, `f64::from(x)`,
  `char::from(b)`, `String::from(c)`. `as` keeps its conversions, which never fail.

## Defaults

Rationale: [ADR-0094](../adr/0094-conversions-and-defaults.md), Q636, Q637 and Q643. `Default`
is the prelude's interface of the value a type starts from, as Rust's:

```sag
interface Default { fn default() -> Self; }

@default
struct Config {
  pub name: String;
  pub retries: u32;
  pub level: Level;
}

@default
enum Level {
  Quiet,
  @default
  Normal,
}

const config = Config::default();                          // Config { name: "", retries: 0, level: Normal }
const loud = Config { retries: 3, ..Config::default() };    // the other fields from the default
```

- The prelude's types implement it: numbers are `0`, `bool` is `false`, `char` is `'\0'`,
  `String` is `""`, `StringBuilder`, `List` and `Map` are empty, `Option` is `None`, and tuples and
  arrays are the default of each element when every element has one.
- A struct or enum implements it by hand (`impl Default for Port { fn default() -> Port { ... } }`)
  or with `@default` before its declaration, as Rust's `#[derive(Default)]`. On a struct, every
  field takes its default; a field whose type has none is an error (`E0330`) naming it, so a
  generic struct bounds its parameters (`@default struct Wrap<T: Default>`). On an enum, `@default`
  is also written before the variant without fields that is the default; an enum that marks none,
  a marked variant with fields, two marked variants, or a marked variant of an enum without
  `@default` are errors (`E0105`). `@default` takes no arguments, does not apply to classes,
  unions or other items (`E0105`), and a type with `@default` that also implements `Default` by
  hand is an error (`E0342`). A class implements it by hand, since `new` builds its instances.
- `Type::default()` and `T::default()`, for a type parameter bounded by `Default`, give it;
  `i32::default()`, `f64::default()`, `char::default()` and `String::default()` too.
- `Option::unwrap_or_default()` and `Result::unwrap_or_default()` give the value or the default
  of its type, and `std::mem` has `take(&mut place)`, which leaves the default in the place,
  `replace(&mut place, value)` and `swap(&mut a, &mut b)`.
- **Struct update.** A struct literal may end with `..base`, after its fields and a comma: an
  expression of the same struct type (`E0345` otherwise) whose values give the fields not written.
  `base` is evaluated after the fields written, once; it may give every field (`Config { ..other }`)
  or none. The fields it gives must be visible where the literal is, as the fields written. It is
  only for structs, not for the variants of enums (`E0345`), and nothing may follow it (`E0100`).

## Standard generic types (F2)

`List<T>` (see [lists](02-types.md#lists-f2)), `Map<K, V>` (see [maps](02-types.md#maps-f2)) and
`Iter<T>` (see [iterators](08-closures.md#iterators)).
