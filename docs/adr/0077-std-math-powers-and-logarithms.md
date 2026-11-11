# 0077. Powers, roots and logarithms in `std::math`

- Status: Accepted
- Date: 2026-10-29
- Refines: [ADR-0076](0076-std-math-and-methods-of-numbers.md) (increment 2 of `std::math`, Q453)

## Context

Increment 1 ([ADR-0076](0076-std-math-and-methods-of-numbers.md)) gave floats and integers their
basic methods, and `std::math` its constants. Q453 planned for increment 2 `sqrt`, `cbrt`, `powi`,
`powf`, `exp`, `exp2`, `ln`, `log2`, `log10`, `log`, `hypot` and `mul_add`, with the form of Q454:
methods of `f32` and `f64`, and functions of `f64` with the same names in `std::math`. What
remains to settle is what each one gives at the edges, and how exact the results are, since the
C libraries of each system round them differently.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q463 | The methods | On `f32` and `f64`: `sqrt()`, `cbrt()`, `powi(n: i32)`, `powf(y)`, `exp()`, `exp2()`, `ln()`, `log2()`, `log10()`, `log(base)`, `hypot(y)` and `mul_add(a, b)`, whose float arguments have the type of the receiver. `std::math` has each as a function of `f64` (`math::powi(x: f64, n: i32)`, `math::log(x, base)`). |
| Q464 | Edges | Those of IEEE-754 and C's `<math.h>`, as in Rust: `sqrt` of a negative number and `ln`, `log2`, `log10` of one are NaN, `sqrt(-0.0)` is `-0.0`, `ln(0.0)` is negative infinity, `cbrt` takes negative numbers, `powi` and `powf` with exponent zero give `1.0` (also for NaN), and a negative base with a fractional exponent in `powf` is NaN. Nothing panics. |
| Q465 | Precision | `sqrt` and `mul_add` are exact to the last bit (IEEE-754 requires `sqrt` correctly rounded; `mul_add` computes `x * a + b` rounding once, C's `fma`). The others are those of the system's C library, which may differ between systems in the last digit, as in Rust. `log(base)` is `ln(x) / ln(base)`, which may differ from `log2` and `log10` in the last digit. `powi` computes in `f64`, where every `i32` exponent is exact, also for `f32`. |
| Q466 | Integers | No new methods: `pow(exp: u32)` of increment 1 is the power of integers; their roots and logarithms (`isqrt`, `ilog2`) are not planned. |
| Q467 | Tests | A `run-pass` case for every method on `f64` and `f32` and every function, with the edges of Q464; results Q465 does not make exact are compared within a tolerance, so that the output is the same on every system. `compile-fail` cases for wrong arguments and for `sqrt` on an integer. |
| Q468 | Documents | This record, `docs/spec/02-types.md`, `docs/spec/11-standard-library.md` and `docs/roadmap-v2.md`. |

## Alternatives considered

- **Writing the functions in Sagitario, with the same results everywhere:** correctly rounded
  `exp` and `ln` are long, subtle code (as CORE-MATH or crlibm); the C library is what C, Rust and
  C# programs use, and a game needs speed more than the last bit.
- **`pow(y)` taking either an integer or a float:** without overloading, one name cannot take
  both; Rust's `powi` and `powf` say which one is meant.
- **`powi` by repeated multiplication, as LLVM does for Rust:** faster for small exponents, but
  it rounds at each step, while `pow` of C computes the power as one operation.

## Consequences

- Floats have the powers and logarithms an engine needs for lengths, falloffs and curves.
- Increment 3 (trigonometry) remains.
