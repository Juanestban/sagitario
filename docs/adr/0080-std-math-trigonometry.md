# 0080. Trigonometry in `std::math`

- Status: Accepted
- Date: 2026-11-10
- Refines: [ADR-0076](0076-std-math-and-methods-of-numbers.md) (increment 3 of `std::math`, Q453)

## Context

Increments 1 and 2 ([ADR-0076](0076-std-math-and-methods-of-numbers.md),
[ADR-0077](0077-std-math-powers-and-logarithms.md)) gave floats their basic methods, powers, roots
and logarithms. Q453 planned for increment 3 `sin`, `cos`, `tan`, `asin`, `acos`, `atan`, `atan2`,
`sinh`, `cosh`, `tanh`, `to_degrees` and `to_radians`, with the form of Q454: methods of `f32` and
`f64`, and functions of `f64` with the same names in `std::math`. As in increment 2, what remains
is what each one gives at the edges, and how exact the results are.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q501 | The methods | On `f32` and `f64`: `sin()`, `cos()`, `tan()`, `asin()`, `acos()`, `atan()`, `atan2(x)`, `sinh()`, `cosh()`, `tanh()`, `to_degrees()` and `to_radians()`, in radians. As in Rust, the receiver of `atan2` is `y`: `y.atan2(x)` is the angle of the point `(x, y)`, in `[-π, π]`. `std::math` has each as a function of `f64`, with `math::atan2(y, x)` in C's order. |
| Q502 | Edges | Those of IEEE-754 and C's `<math.h>`, as in Rust: `sin`, `cos` and `tan` of an infinity are NaN; `asin` and `acos` outside `[-1, 1]` are NaN; `atan` of an infinity is `±π/2`; `atan2` follows the signs of its zeros (`0.0.atan2(-0.0)` is `π`, `(-0.0).atan2(-0.0)` is `-π`); `tanh` tends to `±1`, `sinh` and `cosh` overflow to infinities. Nothing panics. |
| Q503 | Precision | Those of the system's C library, which may differ between systems in the last digit, as in Rust. `PI.sin()` is not exactly zero, since `PI` is not exactly π. `to_degrees` and `to_radians` multiply by a constant, as Rust does: `180 / π` and `π / 180` in `f64`, and for `f32` the `f32` nearest to `180 / π` and `math::PI as f32 / 180`. |
| Q504 | Not planned | `sin_cos`, the inverse hyperbolic functions (`asinh`, `acosh`, `atanh`) and `exp_m1`/`ln_1p` were not in Q453; they can join later without changing these. |
| Q505 | Tests | A `run-pass` case for every method on `f64` and `f32` and every function, with the edges of Q502; inexact results are compared within a tolerance, so that the output is the same on every system. `compile-fail` cases for wrong arguments and for a method of floats on an integer. |
| Q506 | Documents | This record, `docs/spec/02-types.md`, `docs/spec/11-standard-library.md` and `docs/roadmap-v2.md`. |

## Alternatives considered

- **`atan2` with the receiver as `x`:** reads as "the angle of x and y", but every C, Rust and C#
  program writes `atan2(y, x)`; keeping Rust's form keeps code ported from them correct.
- **Degrees as a separate type (`Angle`):** safer, but a choice for the math package of the
  engine (G4), not for the methods of numbers.
- **Writing the functions in Sagitario:** as for increment 2 (ADR-0077), the C library is what
  C, Rust and C# programs use, and its range reduction for large arguments is long, subtle code.

## Consequences

- Floats have what an engine needs for rotations, cameras and directions; with increments 1 and
  2, `std::math` has the list of G1.
