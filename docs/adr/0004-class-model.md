# 0004. Class model: single inheritance, interfaces, no open/final

- Status: Accepted
- Date: 2026-10-21

## Context

The goal is class handling very similar to TypeScript, Java and C#, while keeping the
developer-facing language simple.

## Decision

- Single inheritance with `extends`; parent access with `zod`; mandatory `override`.
- `abstract class` and `abstract fn`.
- Interfaces with default methods, implementable by classes and structs (`impl`), usable as
  types. Interfaces can extend interfaces.
- No multiple class inheritance. Structs do not inherit.
- **No `open` and no `final`**: every class is inheritable and every method overridable. The
  compiler, which sees the whole program, devirtualizes methods nobody overrides.
- Member visibility `pub` / `prv` is mandatory; `prv` means the declaring class only. No
  `protected` in the MVP.

## Alternatives considered

- **Final by default with `open` (Kotlin, Swift)**: faster by default, but `open`/`final`
  were judged an unnecessary concept for developers; performance is recovered by the
  compiler instead.
- **No inheritance, composition + traits (Rust, Go)**: simpler, but does not match the
  Java/C# style that is a core goal.
- **Multiple inheritance / mixins (C++, Scala)**: diamond problem and complex layouts.
- **`protected`**: not needed yet; may be reconsidered after the MVP.

## Consequences

- Familiar OOP for Java/C#/TS developers.
- Devirtualization is a compiler responsibility, not a developer one; it depends on
  whole-program analysis, which separately compiled libraries (F3) may limit.
