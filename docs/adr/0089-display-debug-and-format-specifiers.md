# 0089. `Display`, the debug form of values, and format specifiers (S2)

- Status: Accepted
- Date: 2026-11-11
- Refines: [ADR-0082](0082-format-strings.md) (Q523: format specifiers were left for a `Display`
  interface), docs/spec/02-types.md (user types cannot be interpolated "until a `Display`
  interface exists") and [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: the
  audit of S2 puts `fmt` second)

## Context

Format strings (`f"total = {a + b}"`, ADR-0082) interpolate numbers, `bool`, `char` and
`String`, and nothing else: a struct, an enum or a class cannot be turned into text except by a
method each program names itself, values cannot be printed for debugging, and numbers cannot be
padded, rounded or written in hexadecimal. The audit of S2 ([`docs/std-audit.md`](../std-audit.md))
puts this second, after the gaps of the operators.

Rust has two traits, `Display` (`{}`) and `Debug` (`{:?}`, usually derived), whose `fmt` writes
to a `Formatter`, and a mini-language of specifiers after a `:` inside the braces:
`{:>8.2}`, `{:#x}`, `{:08}`, `{:?}`, `{:#?}`. Python's f-strings use the same place for theirs.

The user chose, for this record: a `Display` whose method returns the text, `fn to_string(self)
-> String`, rather than Rust's `Formatter`; a debug form the compiler gives every type, since
Sagitario has no `derive`; and Rust's mini-language of specifiers.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q583 | `Display` | A prelude interface, `pub interface Display { fn to_string(self) -> String; }`. A type implementing it, in its body or an `impl Display for T` block (ADR-0081), can be interpolated: `f"{p}"` is `p.to_string()`. Integers, floats, `bool`, `char` and `String` implement it, with the formats they have today, so `5.to_string()` is `"5"` and a type parameter bounded by `Display` can be interpolated. |
| Q584 | Without `Display` | Interpolating a value whose type does not implement `Display` stays an error (`E0306`), now with help: implement `Display`, or write `{x:?}` for the debug form. Lists, maps, options and results do not implement `Display`, as in Rust. |
| Q585 | Specifiers | After the expression, a `:` and a specifier: `{expr:spec}`. The `:` is the first one outside parentheses, brackets, braces and strings that is not part of `::`, so `{a::b}` and `{f((x: i32) => x)}` keep their meaning. The specifier is Rust's: `[[fill]align][+][#][0][width][.precision][type]`, all optional, with a literal `width` and `precision`. An invalid specifier is an error (`E0007`), as other mistakes in a format string are. |
| Q586 | Alignment and width | `width` is the least number of characters (Unicode scalar values); shorter text is filled with `fill` (a space by default) on the side `align` says: `<` left, `^` centered (an odd fill goes right), `>` right. Numbers align right by default, other text left. `0` pads a number with zeros after its sign and prefix (`{-5:04}` is `-005`), and is ignored with an explicit `align`. |
| Q587 | Sign, precision and types | `+` writes the sign of every number (`+5`). `.precision` gives a float that many decimals, rounded to the nearest (`{2.675:.2}` is `2.67`, since 2.675 is a little below it in binary, as in Rust), and cuts a text after that many characters; on an integer it is an error. The type is none (`Display`), `?` (the debug form), `x` and `X` (hexadecimal), `o` (octal), `b` (binary) for integers, whose negative values show their two's complement bits as in Rust (`{-1i8:x}` is `ff`), and `e` and `E` for floats in scientific notation (`{1500.0:e}` is `1.5e3`). `#` adds the prefix `0x`, `0o` or `0b`, and makes `?` multi-line. |
| Q588 | The debug form | Every type has one, from the compiler, as Rust's derived `Debug`: numbers and `bool` as `Display`; `char` and `String` quoted with escapes (`'a'`, `"tab\t"`); `Some(1)`, `None`, `Ok(..)`, `Err(..)`; structs and classes with every field, also private (`Point { x: 1, y: 2 }`); enum variants by name, with their fields (`Circle(2.5)`, `Move { x: 1 }`, `Red`); lists and arrays as `[1, 2]`, slices too; maps as `{"a": 1}`; a function value as `fn`, a raw pointer as its address in hexadecimal, an interface value as the value inside. `{x:#?}` writes one field or element per line, indented by four spaces, as Rust's. A class instance reached again while its own form is being written (a cycle) is written `Name { .. }`. A type implementing `Display` keeps its structural debug form. |
| Q589 | Where formatting runs | Padding, signs, bases and the debug form are written in Sagitario, in the prelude and in code the compiler generates for each type; only the digits of floats (shortest round-trip, precision, scientific notation) stay in the runtime's core, which already writes them. |
| Q590 | The editor | The grammar highlights the specifier inside the braces; `sag lsp` completes `to_string` and the methods of `Display`; a snippet `impldisplay` writes an `impl Display for` block (CLAUDE.md). |
| Q591 | Increments | **1** `Display` and `to_string()` (Q583, Q584), with the tests and the editor. **2** The specifiers (Q585–Q587) for numbers, `String` and `Display` types. **3** The debug form (Q588) and `?`, `#?`. |

### Example

```sag
struct Point {
  pub x: i32;
  pub y: i32;
}

impl Display for Point {
  fn to_string(self) -> String {
    f"({self.x}, {self.y})"
  }
}

fn main() {
  const p = Point { x: 1, y: -2 };
  println(f"{p} [{p:>10}] {p:?}");      // (1, -2) [   (1, -2)] Point { x: 1, y: -2 }
  println(f"{3.14159:.2} {42:08} {255:#x} {5:b} {-7:+}");  // 3.14 00000042 0xff 101 -7
  println(f"[{"Ada":^7}] [{"Ada":*<6}] {1500.0:e}");       // [  Ada  ] [Ada***] 1.5e3
}
```

## Alternatives considered

- **A `Formatter` as Rust's** (`fn fmt(self, f: Formatter) -> Result<(), FmtError>`): it writes
  without intermediate strings and lets a type see the width and precision asked of it, but it
  is more code for every type and more API; the user chose `to_string`. The width and alignment
  apply to the text `to_string` returns, which covers most uses.
- **A debug form only for types that opt in** (an interface, or a `derive`-like attribute): Rust
  needs `#[derive(Debug)]` because it cannot see private fields from outside a crate; Sagitario's
  compiler can, and every type being printable for debugging is what programs need.
- **Only `:?` and `.precision` now:** smaller, but widths and bases are what tables, logs and
  binary formats need, and Rust's mini-language is one coherent design.
- **Python's specifier types** (`f`, `d`, `%`): Rust's are the reference of the language, and its
  default formats already need no `d` or `f`.

## Consequences

- Any type can be printed for debugging, and those implementing `Display` can be interpolated
  like numbers and strings.
- `Display` joins the prelude's interfaces; a program declaring its own `Display` hides the
  prelude's, as any of its names does.
- The compiler generates a debug function per type that is printed with `?`, only where it is
  used.

## Notes

- Increment 1 (`Display`) is done. The prelude declares it; `TypedModule::displayable` decides
  what a format string interpolates, and numbers, `bool`, `char` and `String` implement it among
  the interfaces of primitives (`primitive_impls`), so they satisfy a `Display` bound. SIR
  interpolates any other value through its `to_string()`, found as a call of an interface method
  on its type (`Lowering::display`), or through the itable of a `Display` value; `to_string()`
  of a primitive is the interpolation of it alone. `sag lsp` completes `to_string` on primitives,
  and the snippet `impldisplay` writes a block. Tests: `tests/run-pass/display.sag` (structs,
  enums, classes, nested values, a bounded type parameter, a `Display` value and primitives).
- Increment 2 (the specifiers) is done. The lexer ends the expression of a `{...}` of a format
  string at its first `:` outside brackets that is not part of `::`, and gives the rest as a
  `Spec` part; the parser reads it into an `ast::FormatSpec` of a `TemplatePart::Formatted`, the
  type checker matches it to the value (`exprs::check_spec`), and SIR calls private functions of
  the root of `std`, written in Sagitario, which every program loads (`format_signed`,
  `format_unsigned`, `format_float`, `format_text`). Only the digits of floats come from the
  runtime's core (`sag_float_format`), on C's `%.*f` and `%.*e`. `?` is reported as not
  implemented (`E0900`) until increment 3. The grammar of the editor colors the specifier.
  Tests: `tests/run-pass/format_specifiers.sag`, `tests/compile-fail/format_specifiers.sag` and
  `format_specifier_syntax.sag`.
- Increment 3 (the debug form) is done. SIR lists the types `{x:?}` writes
  (`Program::debug_types`) and registers the class instances their forms reach; the layouts now
  carry the names of fields and variants. The C backend writes a function `sag_debug_N` for each
  of those types and the types inside them (`debug_support`), and `sag_debug_string_N` for the
  ones the program writes; the runtime's core gives the escapes of strings and characters, the
  separators of `#?` and the stack of the objects being written, which stops cycles. A field of
  type `Atomic` is read with `atomic_load`. Two details differ from Q588: an interface value is
  written `dyn Name`, since its itable has no debug function yet, and a weak field `(weak)`, as
  Rust writes its `Weak`. Tests: `tests/run-pass/debug_format.sag` (structs, enums, strings and
  their escapes, options, results, lists, arrays, maps, classes with a parent, a cycle, a width,
  a generic struct and `#?`), with the leak check.
