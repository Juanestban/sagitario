# 0099. Associated constants and composite constants (S2)

- Status: Accepted
- Date: 2026-11-13
- Refines: [ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md) (module-level
  constants of primitive types and `String`), docs/spec/03-memory.md (module-level constants),
  docs/spec/04-classes.md (members of a class)

## Context

Record 10 of the audit of S2 ([`docs/std-audit.md`](../std-audit.md)), networking, follows Rust's
`std::net`, whose addresses have constants of their types: `Ipv4Addr::LOCALHOST`,
`Ipv4Addr::UNSPECIFIED`, `Ipv4Addr::BROADCAST`, `Ipv6Addr::LOCALHOST` and `Ipv6Addr::UNSPECIFIED`.
Sagitario has neither: a type holds fields and functions, and a constant lives at module level,
where ADR-0049 lets its value be an integer, a float, `bool`, `char` or `String` only, which the
compiler computes. `i32::MAX` and its kind are builtins of the primitive types.

So before the addresses, the language gains what Rust's `const` items have: constants declared in
a type, named through it, and values of structs, enums, arrays and tuples. It is written as an
ADR of its own, since it is a change of the language that ADR-0100 then uses.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q683 | Associated constants | `pub const NAME: Type = value;` or `prv const NAME: Type = value;` in the body of a struct, enum or class: a constant of the type, as Rust's associated `const`. Its value tells it from a field of a class, `pub const name: Type;`, which never has one. Its type is always written, and it declares `pub` or `prv` as every member does; a `prv` one is reached only inside the type. It is named `Type::NAME`, and `Self::NAME` inside the type; through a value (`p.NAME`) it is an error. It shares the namespace of the type's fields, functions and variants (`E0201`). A subclass does not inherit it: it is named through the class that declares it. A generic type cannot declare one yet (a new code), since `Type::NAME` would need the type's arguments. |
| Q684 | Their values | Computed by the compiler with the rules of a module-level constant (spec 03): from literals, other constants, of the module or of a type, and the operators the spec lists; a constant cannot need its own value. A constant of a type can use the others of its type as `Self::NAME`. |
| Q685 | Composite constants | A constant, of a module or of a type, may also be a struct, an enum, an array or a tuple whose parts are constants, written as literals: a struct literal naming every field (`Ipv4Addr { octets: [127, 0, 0, 1] }`, where a literal of that struct is allowed: a private field only inside its type), a variant with or without fields (`Color::Red`, `Some(3)`, `Ok(1)`), `[a, b]` and `[value; count]`, and `(a, b)`. A part of a constant is a constant: `ORIGIN.x`, `PAIR.0`, and `TABLE[2]` with a constant index, an index outside the array being an error. There are no calls, as in Rust without `const fn`. Classes, `List`, `Map`, slices, references, raw pointers and function values stay out (`new` creates them, or they point to memory). Comparisons and arithmetic stay those of the primitive types and `String`. |
| Q686 | Their C | A constant of a primitive type or `String` is written where it is used, as today. A composite one is written once in the C, as a `static const` object of its type, and each use copies it, as any value of a struct does. |
| Q687 | The editor | The grammar colors a constant of a type as a module constant; `sag lsp` completes the constants after `Type::` and `Self::` beside the static functions, gives their tokens the kind of a constant, and shows them on hover and goes to their definition as for a module constant. Their sessions in `tests/lsp/`, the snippets and the README follow. |
| Q688 | Left for later | Constants of interfaces, read as `T::NAME` in a generic function bounded by the interface (Rust's associated constants of traits); constants used as patterns; constants of generic types; and calls in constants (`const fn`). They are recorded as deferred details of the roadmap, to take up once the core is in Sagitario (S3). |
| Q689 | Increments | **1** Composite constants at module level (Q685, Q686). **2** Associated constants, of primitive and composite values, and the editor (Q683, Q684, Q687). Each one with its tests and the spec. |

### Example

```sag
pub struct Ipv4Addr {
  prv octets: [u8; 4];

  pub const LOCALHOST: Ipv4Addr = Ipv4Addr { octets: [127, 0, 0, 1] };
  pub const UNSPECIFIED: Ipv4Addr = Ipv4Addr { octets: [0, 0, 0, 0] };

  pub fn octets(self) -> [u8; 4] {
    self.octets
  }
}

class Server {
  pub const name: String;                 // a field: no value
  pub const DEFAULT_PORT: u16 = 8080;     // a constant of the class

  pub fn new(self, name: String) {
    self.name = name;
  }
}

const ORIGIN: (i32, i32) = (0, 0);
const PRIMES: [u32; 4] = [2, 3, 5, 7];

fn main() {
  println(f"{Ipv4Addr::LOCALHOST.octets():?} {Server::DEFAULT_PORT} {ORIGIN.0} {PRIMES[3]}");
}
```

## Alternatives considered

- **`static` in a type** (`pub static LOCALHOST: Ipv4Addr = ...;`), computed once as a static of
  a module: any expression, calls included, but `static` declares module-level state only
  (spec 03), and Rust's word is `const`.
- **Static functions** (`Ipv4Addr::localhost()`): no change to the language, but not Rust's
  names.
- **Constants of the module** (`net::LOCALHOST_V4`): no change to the language, but not Rust's
  names either.
- **Calls in constants**, as Rust's `const fn`: `Ipv4Addr::new(127, 0, 0, 1)` in a constant, at
  the cost of an interpreter of functions in the compiler; literals cover what `std` needs.

## Consequences

- Types gain constants with Rust's names, and constants gain structs, enums, arrays and tuples,
  at module level too.
- ADR-0100 declares the constants of `Ipv4Addr` and `Ipv6Addr` with them.
- The deferred details of the roadmap list constants of interfaces, constants in patterns,
  constants of generic types and `const fn`.

## Notes

- Increment 1 is done. A constant whose type is not primitive, or whose value builds or reads a
  part of a struct, an enum, an array or a tuple, is first checked as an expression of its type,
  outside any type, which gives its literals the checks of any other (missing fields, privacy,
  the type arguments of a generic struct); the types of every constant are known before any value
  is, for that. Its value is then computed from those types, as a tree of parts. An expression
  with a form no constant has (a call, a lambda, `..base`) is reported before it is checked, so
  the check never sees code outside a function. Q686 changes in how the C holds a composite
  constant: each use builds the value from its parts, as a literal of them, instead of copying a
  `static const` object written once. The result is the same value, which the C compiler folds,
  and the `String` parts of a constant need no object of their own outside the core's literals.

