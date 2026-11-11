# 0088. Bitwise operators, shifts and the integer methods of spec 2 (S2)

- Status: Accepted
- Date: 2026-11-11
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: the gaps the audit of
  S2 finds) and [ADR-0076](0076-std-math-and-methods-of-numbers.md) (the methods of numbers)

## Context

The audit of S2 ([`docs/std-audit.md`](../std-audit.md)) puts two gaps of the language first.
Sagitario has no bitwise operators: the standard library hashes, decodes UTF-8 and reads the
bits of `stat` with `*`, `/` and `%`, which is slower and harder to read, and programs cannot
work with flags, masks or binary formats. And spec 2 promises `wrapping_*`, `checked_*` and
`saturating_*` methods that were never implemented, so code that must not panic on overflow in
debug builds cannot say so.

Rust's operators are `&`, `|`, `^`, `!` (the same `!` as on `bool`), `<<` and `>>`, with
compound assignments; a shift by the width of the type or more panics in debug builds and is
reduced modulo the width in release builds. C defines `&`, `|`, `^` and `~` on any integer, but
shifting by the width or more, shifting a negative number left and overflowing a signed left
shift are undefined behavior, which the generated C must never have (CLAUDE.md).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q574 | Operators | As Rust's: `a & b`, `a \| b` and `a ^ b` (and, or, exclusive or), `a << n` and `a >> n` (shifts), `!a` (every bit flipped), and the assignments `&=`, `\|=`, `^=`, `<<=` and `>>=`. `!` stays the logical not on `bool`, as in Rust (the user chose it over `~`). |
| Q575 | Precedence | Rust's, from tighter to looser: unary operators, `as`, `* / %`, `+ -`, `<< >>`, `&`, `^`, `\|`, comparisons, `&&`, `\|\|`, ranges. `a & b == c` is `a & (b == c)` in C but `(a & b) == c` here, as in Rust. |
| Q576 | Types | `&`, `\|` and `^` take two operands of the same integer type, or two `bool`s (both evaluated, unlike `&&` and `\|\|`); the result has their type. `!` takes an integer or a `bool`. A shift keeps the type of its left operand, an integer; its amount is any integer type, and an unsuffixed literal amount is a `u32`. Other operands are errors (`E0310`). The operators are not overloadable yet: interfaces like Rust's `BitAnd` are left for a later record. |
| Q577 | Shifts | A shift amount that is negative, or equal to the width of the type or more, panics in debug builds (`attempt to shift left with overflow`, Rust's messages) and is reduced modulo the width in release builds, as in Rust. `<<` drops the bits shifted out, also of signed numbers (two's complement); `>>` is arithmetic on signed numbers (copies of the sign bit come in) and logical on unsigned ones. The C computes through unsigned values, so it never has undefined behavior. |
| Q578 | Tokens | The lexer gives `^`, `^=`, `&=`, `\|=`, `<<` and `<<=` as tokens. `>>` and `>>=` are two tokens joined by the parser where an operator is expected, when nothing separates them, so the `>>` closing `List<List<T>>` stays two `>`. `a > > b` is not a shift. |
| Q579 | Block statements | A statement starting with a block-like expression still ends at its `}` (spec 6): `unsafe { x } & mask;` is an error (`E0100`) asking for parentheses, like any other operator. |
| Q580 | Integer methods | Every integer type gains the methods spec 2 promises and their companions in Rust: `wrapping_add`, `wrapping_sub`, `wrapping_mul`, `wrapping_neg` (signed), `wrapping_shl`, `wrapping_shr`; `checked_add`, `checked_sub`, `checked_mul`, `checked_div`, `checked_rem`, `checked_neg` (signed), `checked_shl`, `checked_shr`, giving `Option<T>`; `saturating_add`, `saturating_sub`, `saturating_mul`; and `count_ones`, `count_zeros`, `leading_zeros`, `trailing_zeros` (giving `u32`), `rotate_left(n: u32)`, `rotate_right(n: u32)`, `swap_bytes()`, and `is_power_of_two()` on unsigned types. `T::MIN` and `T::MAX` name the limits of each integer type (`i32::MAX`), as constants. |
| Q581 | The editor | The grammar of the VS Code extension highlights the new operators; the completion of `sag lsp` offers the new methods of numbers and `MIN` and `MAX` after an integer type and `::` (CLAUDE.md). |
| Q582 | Increments | **1** The operators (Q574–Q579): the lexer, the parser, the type checker, SIR and the C (runtime helpers for shifts, as for the arithmetic), the editor, a `run-pass` test of every operator and edge, a `run-panic` test of a shift overflow, and `compile-fail` tests of the operands. **2** The integer methods and `MIN`/`MAX` (Q580), with their tests. **3** The standard library rewritten with them where `*`, `/` and `%` stood for bits: the reading of UTF-8, the hashing of the prelude, the mode of `stat`. |

### Example

```sag
const READ: u32 = 1 << 2;
const WRITE: u32 = 1 << 1;

fn main() {
  const mode = READ | WRITE;
  const low = 0xABCDu16 & 0xFF;
  println(f"{mode & READ != 0} {low} {!0u8} {-16i32 >> 2} {mode ^ WRITE}");
  // true 205 255 -4 4
  println(f"{i32::MAX.wrapping_add(1)} {200u8.checked_add(100).is_none()} {250u8.saturating_add(10)}");
  // -2147483648 true 255
}
```

## Alternatives considered

- **`~` for the bitwise not**, as C, C# and JavaScript: it keeps `!` for `bool` alone, but the user
  chose Rust's spelling, which Sagitario follows elsewhere, and `!` on an integer cannot be
  mistaken for anything else.
- **C's precedence**, where `&` binds looser than `==`: a known trap (`flags & MASK == 0`); Rust
  changed it, and so does Sagitario.
- **Shifts that always panic, or always wrap:** Rust's rule matches how Sagitario's arithmetic
  already behaves in each build mode (spec 2).
- **Lexing `>>` as one token and splitting it in generic types**, as Rust's parser does: joining
  two `>` where an operator is expected is the same rule, written once in the parser.

## Consequences

- The standard library can use bits directly, and programs can read and write binary formats,
  flags and hashes.
- Code that must not panic on overflow in debug builds says so with `wrapping_*`, `checked_*` or
  `saturating_*`.
- `&` and `|` mean more than before: `&` stays a reference before a parameter's argument, `|`
  stays the separator of patterns, and both are operators between two values.

## Notes

- Increment 1 (the operators) is done. The lexer gives `^`, `^=`, `&=`, `|=`, `<<` and `<<=`,
  and suggests `!` after a `~`; the parser joins `>` `>` and `>` `>=` that touch
  (`Parser::infix_here`, `assignment_here`), with Rust's precedence (`PREC_SHIFT`...). The type
  checker gives the rules of Q576 (`exprs::bitwise`, `shift_amount`), the constants fold the
  operators one bit at a time, since the seed that builds the compiler has none
  (`consts::combine_bits`, `const_shift`), and SIR passes the amount of a shift as a `u64`.
  The runtime's header gains `sag_shl_*_at` and `sag_shr_*_at`, computed through `uint64_t`.
  Tests: `tests/run-pass/bitwise_operators.sag`, `tests/run-panic/shift_overflow.sag` and
  `tests/compile-fail/bitwise_operators.sag`; the corpus of constants now gives `!` a float to
  reject, since `!1` is an integer.
