# 0102. Binding generation before the core in Sagitario

- Status: Accepted
- Date: 2026-11-14
- Supersedes: Q545 of [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (the order
  of S3 and S4)

## Context

S2 is done ([ADR-0101](0101-the-rest-of-s2.md)). Q545 put the two phases left in this order:
S3, the runtime's core rewritten in Sagitario, then S4, the generation of bindings from C headers.

The two phases do not depend on each other. S4 writes declarations the FFI already has
(`extern "C"` blocks, `@repr(C)` structs and unions, function pointers,
[spec 10](../spec/10-ffi.md)). S3 changes how the runtime is written, not what a program sees
of it. The acceptance of S4 can be met with the core still in C.

They differ in size and in what they unblock:

- S3 is the largest phase left: about 2,300 lines of C and the fast paths of the header, a
  low-level layer in the compiler, code without reference counting, and an open question about
  the bootstrap.
- S4 is smaller, and it is what the game engine, a project apart (Q538), needs first: bindings
  for SDL, OpenGL, Vulkan and audio libraries, written today by hand.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q719 | Order | S4 comes before S3. The phases keep their names: S3 is the core in Sagitario and S4 the generation of bindings, as Q543 and Q544 and the records that cite them say. Filling gaps of S2 can still interleave with both. |

## Alternatives considered

- **Keep S3 before S4 (Q545):** the core in Sagitario first. The engine would wait for the
  longest phase, or start with bindings written by hand that S4 would then replace.
- **Swap the names, so that binding generation becomes S3:** the order would read in the names,
  but ADR-0084, 0085, 0086, 0087, 0099 and 0101, the architecture, the audit and the spec cite
  "S3" for the core ("stays C until S3", "⛔ until S3"). Accepted records are not rewritten, so
  the name would mean two things.

## Consequences

- The order section of [`docs/roadmap-v2.md`](../roadmap-v2.md) says S4 comes before S3.
- S4's record follows: [ADR-0103](0103-build-scripts-and-binding-generation.md).
- What is deferred to S3 (backtraces, the deferred details of constants, the prelude's output
  without C's stdio) waits for it, after S4.
