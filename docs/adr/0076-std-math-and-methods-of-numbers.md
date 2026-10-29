# 0076. `std::math` and the methods of numbers

- Status: Accepted
- Date: 2026-10-29
- Refines: [ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md) (Q221),
  [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) (G1 of roadmap v2)

## Context

G1, the first phase of [roadmap v2](../roadmap-v2.md), gathers what a game engine needs from the
language before windows and graphics. Its list starts with `std::math`: `sqrt`, `sin`, `cos`,
`atan2`, `pow`, `floor`, `abs`, `min`, `max`, `clamp`, `lerp` for `f32` and `f64`. Today numbers
only have `equals`, `compare` and `hash` (integers) and `total_compare` (floats), and there is
no way to name π or an infinity: a constant that is not finite is `E0334` (ADR-0049, Q221).

Sagitario has no overloading, so one free function cannot take both an `f32` and an `f64`. Rust
puts these functions on the types as methods (`x.sqrt()`), which C# and Java put in a class
(`Math.Sqrt(x)`), and C in `<math.h>` with a suffix for `float` (`sqrtf`).

## Decision

| # | Topic | Decision |
|---|---|---|
| Q453 | Increments | `std::math` comes in three increments: **1** the basic methods of floats and integers, the constants, and infinities and NaN in constants; **2** powers, roots and logarithms (`sqrt`, `cbrt`, `powi`, `powf`, `exp`, `exp2`, `ln`, `log2`, `log10`, `log`, `hypot`, `mul_add`); **3** trigonometry (`sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`, `sinh`, `cosh`, `tanh`, `to_degrees`, `to_radians`). The other items of G1 get their own records. |
| Q454 | The form | Methods of `f32` and `f64`, as in Rust, so both types share each name; and in `std::math`, functions of `f64` with the same names, which call the methods (`math::floor(x)` is `x.floor()`). |
| Q455 | Methods of floats | `abs()`, `signum()`, `copysign(sign)`, `min(other)`, `max(other)`, `clamp(min, max)`, `lerp(b, t)`, `floor()`, `ceil()`, `round()`, `trunc()`, `fract()`, `is_nan()`, `is_infinite()`, `is_finite()`. The arguments have the type of the receiver; the `is_` methods give a `bool`. |
| Q456 | Their semantics | Rust's: `min` and `max` ignore a NaN and give the other value (C's `fmin`, `fmax`); `clamp` panics unless `min <= max` (so also when a bound is NaN), and gives NaN for a NaN; `round` rounds halves away from zero; `fract()` is `x - x.trunc()`; `signum()` is `1.0` or `-1.0` with the sign of `x` (also for zeros and infinities), and NaN for NaN. `a.lerp(b, t)` is `(1 - t) * a + t * b`, exact at `t == 0` and `t == 1`, and does not clamp `t`. |
| Q457 | Methods of integers | `min(other)`, `max(other)`, `clamp(min, max)` and `pow(exp: u32)` on every integer type; `abs()` and `signum()` on signed ones only (on unsigned ones, `E0316` with a note). `pow` and `abs` overflow as arithmetic does: a panic in debug builds (`attempt to multiply with overflow`, `attempt to negate with overflow`), wrapping in release builds (the `abs()` of the smallest `i32` is itself). `clamp` panics unless `min <= max`. |
| Q458 | Constants | `std::math` has constants of `f64` only: `PI`, `TAU`, `E`, `SQRT_2`, `LN_2`, `LN_10`, `EPSILON`, `MIN`, `MAX`, `INFINITY`, `NEG_INFINITY` and `NAN`. For `f32`, `math::PI as f32` rounds them. |
| Q459 | Infinities and NaN in constants | Refining Q221: a float constant may be infinite or NaN, as IEEE-754 arithmetic gives it (`1.0 / 0.0`, `0.0 / 0.0`, `MAX * 2.0`), and comparisons with NaN are false except `!=`. A float literal too large for its type (`1e39` as `f32`) is still `E0334`, and so is an integer division by zero. The C code writes them with `INFINITY` and `NAN` of `<math.h>`. |
| Q460 | The runtime | Each method is a function of the runtime (`sag_f64_floor`, `sag_pow_i32_at`...) over `<math.h>`, which programs already include and link (`-lm` outside Windows); `std::math` declares nothing of its own. A panic in a function of `std::math` (`math::clamp`) reports its place in the standard library, not the caller's. |
| Q461 | Tests | A `run-pass` case for the constants, every method on both float types and on signed and unsigned integers, and the functions of `std::math`; release wrapping of `abs` and `pow`; `run-panic` cases for `pow` and `abs` overflowing and for `clamp` with invalid bounds; `compile-fail` cases for unknown methods, `abs` on an unsigned integer and wrong arguments; infinite and NaN constants in the C corpus. |
| Q462 | Documents | This record, `docs/spec/02-types.md` (the methods of numbers), `docs/spec/03-memory.md` (constants), `docs/spec/11-standard-library.md` (`std::math`) and `docs/roadmap-v2.md`. |

## Alternatives considered

- **Free functions with a suffix for `f32` (`math::sqrt`, `math::sqrt_f32`):** twice the names,
  and the one for `f32` is easy to forget; methods also chain (`x.abs().sqrt()`).
- **Generic functions over a `Float` interface:** needs an interface the primitive types implement
  with associated constants, which the language does not have.
- **Constants for `f32` (`PI_F32`):** a conversion with `as f32` gives the same values for the
  common constants, and keeps one name per constant.
- **Functions for infinities and NaN (`math::nan()`):** keeps Q221, but a constant that cannot be
  written as a constant is surprising, and IEEE-754 defines what `1.0 / 0.0` is.

## Consequences

- Numbers gain methods computed by the runtime, without new syntax; the type checker keeps their
  table in `types/numbers.sag`.
- Increments 2 and 3 add their methods and functions with the same mechanism.
