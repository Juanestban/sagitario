# 0019. Prelude and printing

- Status: Accepted; refined by [ADR-0037](0037-standard-library-env-io-process.md) (`std::io`)
- Date: 2026-10-22

## Context

The documentation printed in two inconsistent ways (`io::println(...)?` returning `Result`, and
`std::io::println(...);` ignoring it), and examples used `Some`, `None`, `Ok`, `Err` without
imports, implying an undefined prelude.

## Decision

- A fixed prelude: `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `String`, `print`, `println`,
  `eprintln` (F1); `List`, `Map`, `Error` (F2). Not extensible by user code; local names
  shadow it.
- Prelude printing functions panic when writing fails, like Rust's `println!`.
- `std::io` keeps `Result`-returning functions for code that handles I/O errors.

## Alternatives considered

- **Only `std::io::println` returning `Result`**: the simplest program would already need
  `Result`, `?` and modules.

## Consequences

- Hello world is `pub fn main() { println("hello"); }`.
- The first vertical slice needs a minimal runtime `String`.
- Basic interpolation of integers in template strings moves forward to F1 increment 2 so that
  variables and arithmetic can be observed by `run-pass` tests.
