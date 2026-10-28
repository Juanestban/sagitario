# Sagitario

Compiled systems language (Rust-like safety, TS/Java/C#-style classes, JS-style functional
features), compiled to C11. The compiler is written in Sagitario (`compiler/`) and builds from
a seed of the C it generates for itself (`bootstrap/`).

@CONTRIBUTING.md

## Sources of truth
- Language semantics/syntax: `docs/spec/` (normative). Rationale: `docs/adr/`.
- Compiler structure: `docs/architecture.md`. Test harness: `docs/testing.md`.
- Scope and milestones: `docs/roadmap.md`. F0–F6 are done; do not implement F7+ features
  (installers, `nova update self`, `async`/`await`, ...) unless asked.

## Rules
- Never invent syntax or semantics. If the spec is silent or ambiguous (see its open
  questions), stop and ask. Changing a decision requires a new ADR that supersedes the old
  one, plus a spec update.
- The compiler, Nova and the runtime have zero external dependencies. Ask before adding any
  dependency anywhere.
- The compiler must never panic on user input: every failure is a diagnostic with a code
  (`E0001`…), a span and a message. Any internal compiler error gets a regression test in the
  harness.
- Generated C is strict C11: no compiler extensions, no undefined behavior (signed overflow
  goes through runtime helpers).
- The compiler, Nova and the harness build without warnings (the bootstrap scripts enforce
  it). Update the seed (`--update-seed`) only when the compiler needs something the seed cannot
  compile, or when the driver it carries must change.
- Do not loosen checks (warnings, sanitizers, expected dumps, skipped tests) to make code pass.
