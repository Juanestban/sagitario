# 0035. Initializers declare `self`

- Status: Accepted
- Date: 2026-10-23
- Refines: [ADR-0027](0027-classes-and-arc.md) (decision 2, `self` implicit inside `new`)

## Context

[ADR-0028](0028-inferred-self-mutation.md) made `self` an explicit first parameter, as in
Python: a function receives the instance only if it declares `self`. The initializer `new` of
a class was the only exception: `self` existed inside it without being declared, and declaring
it was an error.

The exception also made `fn new(...)` mean two different things. In a struct or enum it is a
static function returning the value. In a class it was the initializer, receiving an instance
that its signature did not show.

## Decision

- The initializer of a class declares `self` first, like a method: `pub fn new(self, name: String)`.
  `self` is the instance being initialized.
- A class's `new` without `self` is an error (E0324) whose help shows `pub fn new(self, ...)`.
  Its body is still checked as an initializer, so the mistake is reported once.
- Nothing else changes:
  - `new ClassName(args)` passes the arguments after `self`;
  - inside `new`, `const` fields can be assigned, `zod.new(...)` comes first, and every field
    must be assigned before `self` is used or `new` returns;
  - `new` cannot be called as `ClassName::new(...)` or `obj.new(...)`, is not a method, and is
    not inherited.
- In structs and enums, `new` stays an ordinary function. Without `self` it is static.

## Alternatives considered

- **Keeping `self` implicit (like `this` in TypeScript, Java and C#):** shorter, but the only
  place where `self` appears without being declared, contrary to ADR-0028.
- **Accepting both forms:** two spellings of one thing, and no help in reading code.

## Consequences

- Every class initializer gains `self`. The spec, examples, tests and prelude are updated.
- Signatures show, by themselves, which functions receive an instance.
