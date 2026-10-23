# 0028. `self` without `var`: methods that modify `self` are inferred

- Status: Accepted
- Date: 2026-10-23
- Amends: [ADR-0007](0007-mutability-const-var.md) and
  [ADR-0021](0021-structs-and-fixed-arrays.md) (`var self`)

## Context

ADR-0007 and ADR-0021 marked the struct methods that modify `self` with `var self`. With
classes, `var self` makes no sense: an instance is a reference, and its `var` fields can change
through any reference. `var` then means two things: a binding that can be reassigned, and a
method that modifies its receiver. The project prefers `var` only for bindings, with `self` as
a plain first parameter as in Python, modifiable from the start.

## Decision

- Methods declare `self` without `var`. `var self` is a syntax error (E0100) whose help says to
  write `self`.
- Every method may modify `self`: assign its fields or elements, call methods that modify it,
  or replace it with `self = value` (structs and enums only).
- For structs and enums, the compiler **infers** which methods modify `self`. A method does if
  its body modifies `self` or a part of it, or calls such a method on `self` or a part of it,
  transitively.
- Those methods receive the caller's value, and can only be called on mutable places, exactly
  as `var self` methods could: `var` bindings, `self`, `var` fields of class instances, and
  their fields and elements. Calling one on a `const` binding, a parameter, a loop variable, a
  pattern binding or a temporary is E0313. Methods that do not modify `self` receive a copy.
- A `var` binding modified only through such calls counts as reassigned for the
  unneeded-`var` warning, as before.
- Classes are unaffected by the inference. Their `self` is a reference: a `var` field can be
  modified in any method, a `const` field in none (outside `new`), and `self` itself cannot be
  reassigned.

## Alternatives considered

- **Keep `var self` for structs only**: the signature states the mutation explicitly. But
  `var` keeps two meanings, and struct and class methods are written differently.
- **Remove `var self` with no protection**: any method could modify a `const` struct, so
  `const` would no longer guarantee that a value does not change.

## Consequences

- Whether a struct method modifies `self` is part of its behavior but not of its written
  signature. The error at a call on a `const` value names the method and says it modifies
  `self`.
- The type checker decides mutation after checking every body (a fixed point over the calls
  on `self`), then reports the calls that need a mutable receiver.
- E0313 now reads "cannot call the method `m`, which modifies `self`, on `x`".
