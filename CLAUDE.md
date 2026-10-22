# Sagitario

Compiled systems language (Rust-like safety, TS/Java/C#-style classes, JS-style functional
features), compiled to C11. Compiler written in Rust; goal: self-hosting.

@CONTRIBUTING.md

## Sources of truth
- Language semantics/syntax: `docs/spec/` (normative). Rationale: `docs/adr/`.
- Compiler structure: `docs/architecture.md`. Test harness: `docs/testing.md`.
- Scope and milestones: `docs/roadmap.md`. The MVP is F0+F1+F2 (desktop only); do not
  implement F3+ features (concurrency, mobile, self-hosting) unless asked.

## Rules
- Never invent syntax or semantics. If the spec is silent or ambiguous (see its open
  questions), stop and ask. Changing a decision requires a new ADR that supersedes the old
  one, plus a spec update.
- Core crates (`sagitario_span` … `sagitario_codegen_c`) have zero external dependencies.
  Ask before adding any dependency anywhere.
- The compiler must never panic on user input: every failure is a diagnostic with a code
  (`E0001`…), a span and a message. Any internal compiler error gets a regression test.
- Write Rust that is portable to Sagitario for self-hosting: plain structs, enums, functions
  and simple traits. No proc macros, no clever trait-level generics.
- Generated C is strict C11: no compiler extensions, no undefined behavior (signed overflow
  goes through runtime helpers).
- Do not loosen lints (`unsafe_code = forbid`, clippy denies) to make code pass.
