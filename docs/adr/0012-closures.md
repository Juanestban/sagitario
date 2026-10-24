# 0012. Closures capture by value, read-only

- Status: Accepted; refined by [ADR-0033](0033-closures-details.md)
- Date: 2026-10-21

## Context

Without a borrow checker, closure capture semantics are a classic source of reference cycles
and surprising mutation.

## Decision

- Capture by value (structs copied, class references retained).
- Mutating a captured variable is a compile error in the MVP.
- Function type `fn(A) -> B`, shared by named functions and closures.
- Representation: function pointer + ARC environment; no allocation without captures.
- Warning when a closure stored in a class field captures `self`; `weak` captures post-MVP.

## Alternatives considered

- **Mutable captures with automatic heap boxing (Swift, JS, C#)**: ergonomic, but hides
  allocations and complicates SIR. Relaxing the rule later is non-breaking; tightening it
  would be breaking.

## Consequences

- No hidden allocations or shared mutable captures.
- Accumulation patterns use `fold`/`reduce` or class instances.
