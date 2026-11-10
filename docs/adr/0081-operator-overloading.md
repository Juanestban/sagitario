# 0081. Operator overloading through interfaces

- Status: Accepted
- Date: 2026-11-10
- Refines: [ADR-0013](0013-primitive-semantics.md) (decision 6, no operator overloading in the
  MVP), [ADR-0017](0017-resolution-of-open-questions-q1-q11.md) (Q7b, per-interface blocks),
  [ADR-0032](0032-generics.md) (a generic interface implemented once per type),
  [ADR-0034](0034-collections-and-iterators.md) (Q67, `==` unchanged by `Eq`; Q73, no numeric
  interface), [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) (operator overloading in G1
  of roadmap v2)

## Context

The acceptance of G1 computes the matrices of a camera with operators: `projection * view *
model * position`, `eye + forward * speed`. Today arithmetic exists only on numbers, so vectors and
matrices are written with methods (`a.add(b.scale(2.0))`), and `==` does not compare structs.
ADR-0013 left operator overloading after the MVP, and the specification plans it "through
interfaces such as `Add`, `Eq`, `Ord`" (docs/spec/02-types.md).

Rust's model is that plan: an operator is a method of a prelude trait (`Add<Rhs>`, `Mul<Rhs>`),
and generic code bounds a type by it (`T: Add<T>`). Its operators need two things Sagitario does
not have. First, one type implements the same generic interface several times: a `Mat4` is
multiplied by a `Mat4` and by a `Vec4`, so it implements `Mul<Mat4>` and `Mul<Vec4>`, while
ADR-0032 allows each generic interface once per type. Second, the methods of those
implementations share a name (`mul`), and the methods of a type all live in its body, where two
`mul` would clash. Rust puts each implementation in its own `impl Trait for Type` block, which
ADR-0017 (Q7b) had declined, since `impl` already names the interfaces of a type in its header.
Rust also gives each implementation an associated type, `Output`, for the result.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q507 | Interface blocks | Besides its header (`struct P impl I`), a type implements an interface in a block at module level, `impl I for T { ... }`, as in Rust, in the module that declares `T`. The block holds the methods of `I` and nothing else, without `pub` (they are public, as an interface's members are); they reach the private members of `T` as its own methods do. Fields of `I` stay members of `T`. A generic type writes its parameters on the block, in order and without bounds, which are the type's: `impl<T> Add<Vec2<T>, Vec2<T>> for Vec2<T>`. |
| Q508 | Several implementations | A type may implement a generic interface several times with different type arguments (`Mul<Mat4, Mat4>` and `Mul<Vec4, Vec4>`), each in its own block; implementing the same instantiation twice is an error. The header still names each interface once, so a second instantiation is written as a block. Subclasses inherit the blocks of their parents, as they inherit interfaces. |
| Q509 | Calling block methods | A method of a block is called by name when the type has one method of that name (`m.mul(v)`), and is otherwise ambiguous: an error that lists the implementations and suggests the operator. Through an interface value or a bound, the method of that instantiation is called. There is no qualified call syntax (`<Mat4 as Mul<Vec4, Vec4>>::mul`) yet. |
| Q510 | The interfaces | The prelude gains `Add<Rhs, Out>`, `Sub<Rhs, Out>`, `Mul<Rhs, Out>`, `Div<Rhs, Out>` and `Rem<Rhs, Out>`, each with one method (`fn add(self, other: Rhs) -> Out`, `sub`, `mul`, `div`, `rem`), and `Neg<Out>` with `fn neg(self) -> Out`. The result is the second type argument, not an associated type: `impl Mul<Vec4, Vec4> for Mat4`. |
| Q511 | Arithmetic operators | `a + b` is `a.add(b)` when `a` is not a number: the implementation of `Add` on the type of `a` whose `Rhs` is the type of `b`, with the type of its `Out`. Only the type of the left operand is searched: `v * 2.0` can be implemented, `2.0 * v` cannot. When the type of `a` implements the interface once, the right operand is checked against its `Rhs`, so literals take that type (`v * 2.0` with `Mul<f32, Vec3>`); with several implementations, it is typed alone and must match one exactly, as there are no implicit conversions. `-a` is `a.neg()`. Operands are evaluated left to right and passed by value, as for any call. `&&`, `\|\|`, `!`, bitwise operators, shifts and indexing are not overloadable. |
| Q512 | Compound assignment | `a += b` (and `-=`, `*=`, `/=`, `%=`) is `a = a + b`, allowed when the `Out` of the implementation is the type of `a`; there is no `AddAssign`. `a` must be assignable, as for numbers. |
| Q513 | Numbers and strings | Integers and floats implement the five interfaces with `Rhs` and `Out` equal to their type, and signed integers and floats `Neg<Self>`; `String` implements `Add<String, String>`. Their operators keep their meaning (overflow panics in debug builds, division by zero panics), so a generic `fn sum<T: Add<T, T>>(a: T, b: T) -> T { a + b }` works with `i32`, `f64` and `Vec3`. No implicit conversions are added: `i32 + i64` stays an error. |
| Q514 | Equality and order | `==` and `!=` on structs and enums that implement `Eq` call `equals` (`!=` negates it); `<`, `<=`, `>` and `>=` on types that implement `Ord` call `compare`. Both operands have the same type, as `Eq` and `Ord` use `Self`. A type parameter bounded by `Eq` or `Ord` allows them too. Classes keep `==` as identity even if they implement `Eq`: changing it would change existing programs; they call `equals`. Floats keep their IEEE comparisons. |
| Q515 | Errors | An operator without an implementation for the operand types is the existing error of invalid operands (`E0310`), with a `help` naming the interface to implement (`implement \`Mul<Vec3, Vec3>\` for \`Mat4\``) and, if the type of `a` has other implementations, their `Rhs`. A second implementation of the same instantiation, a block outside the module of its type, a block method not in the interface, and ambiguous calls get new codes. |
| Q516 | The C code | An operator compiles to a direct call of the monomorphized method, which the C compiler can inline; operators on numbers compile as today. |
| Q517 | Increments | **1** interface blocks and several implementations of a generic interface (Q507–Q509); **2** the arithmetic interfaces and operators, compound assignment, numbers and strings (Q510–Q513); **3** equality and order through `Eq` and `Ord` (Q514), then the G1 sample of a camera. Each updates the spec, with `run-pass` cases and a `compile-fail` case per rule. |
| Q518 | Documents | This record, `docs/spec/02-types.md` (equality, operators), `docs/spec/04-classes.md`, `docs/spec/05-interfaces-generics.md` (blocks, several implementations, the prelude interfaces), `docs/spec/06-control-flow.md` (operators), and `docs/roadmap-v2.md`. |

## Alternatives considered

- **Operators declared as members without names (`operator *(self, other: Vec4)`, as C# and
  Kotlin):** several `*` coexist without blocks or generic interfaces, but generic code could not
  bound a type by an operator, and `sum<T>` would need a separate mechanism.
- **Associated types (`type Output`, as Rust):** the most idiomatic form, but a new concept in
  the parser, the checker and generics (`T::Output`); a second type argument expresses the same
  with what exists. Associated types can come later.
- **Several methods of the same name in the body, chosen by the argument type:** keeps all
  methods in the body, but it is method overloading, which Sagitario has avoided; blocks keep
  every name unique within one implementation.
- **Searching the right operand too (`2.0 * v`):** convenient for scalars, but each operator
  would look in two types and could find two implementations; `v * 2.0` is enough.
- **Changing `==` on classes to `equals`:** what C# and Kotlin do, but it would silently change
  the meaning of existing programs.

## Consequences

- Vectors, matrices, quaternions and colors read as math: `projection * view * model * v`.
- Generic numeric code becomes possible (`T: Add<T, T>`), which Q73 had left without `sum`.
- Interface blocks also allow implementing `From<T>` several times (ADR-0017, Q10), left for its
  own record.
- The compiler gains blocks in the parser and the resolver, several implementations per
  interface in the checker and in dispatch, and the lowering of operators to calls.

## Implementation notes

- Increment 1 (interface blocks and several implementations) is done:
  `docs/spec/05-interfaces-generics.md#impl--for-blocks`. The parameters of a block carry no
  bounds of their own: the type's apply, since a block for only some arguments of a type would be
  a conditional implementation, which Q507 does not plan. A block also holds the methods of the
  parents of its interface. The methods of a block are not virtual, so a subclass does not
  override them. A default method of an interface implemented twice is ambiguous by name, as
  methods are (Q509). Codes: `E0342` (the same instantiation twice, or one interface twice in a
  header), `E0343` (a block whose type or parameters are wrong), `E0344` (an ambiguous name);
  a method of a block outside its interface is `E0329`, as other mismatches of an
  implementation are.
- Increment 2 (the arithmetic operators) is done: `docs/spec/05-interfaces-generics.md#operators`.
  An operator is recorded as a call of the interface's method with the instantiation's type
  arguments, so implementations are found as for any interface method (blocks, the header,
  itables, bounds), and on numbers and strings reached through a bound it lowers to the operator
  itself. Besides Q511, an unsuffixed literal on the right picks the one implementation taking a
  number of its kind; a number on the left of a type with operators is reported with the help of
  writing it on the right; several implementations of `Neg` make `-a` ambiguous (`E0344`). A
  method of an operator that modifies `self` gets a copy of the left operand, as operands are
  values.
