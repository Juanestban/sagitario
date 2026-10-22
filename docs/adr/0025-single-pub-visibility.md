# 0025. A single `pub` visibility, without `pub(pkg)`

- Status: Accepted
- Date: 2026-10-22
- Amends: [ADR-0010](0010-modules-and-packages.md) and
  [ADR-0024](0024-modules-imports-and-visibility.md) (visibility levels)

## Context

ADR-0010 and ADR-0024 gave items three visibility levels: private (the module and its
submodules), `pub(pkg)` (the whole package) and `pub` (other packages too). Until packages can
depend on each other `pub(pkg)` and `pub` are equivalent, and a third level adds a choice to
every declaration for a distinction few programs need.

## Decision

- Items have two visibility levels: **private** (no modifier: the declaring module and its
  submodules, as in Rust) and **`pub`** (visible to every module that imports it, in this
  package and, with the package manager, in other packages).
- `pub(pkg)` and any other restricted form (`pub(...)`) do not exist; the parser rejects them
  with a hint to write `pub`.
- Struct members keep `pub`/`prv`; classes keep their own rules (docs/spec/04-classes.md).

## Alternatives considered

- **Keep `pub(pkg)` (Rust's `pub(crate)`)**: lets a library share items between its modules
  without exporting them. It can be reintroduced if libraries need it, as an addition that
  breaks no code.

## Consequences

- Everything a library makes `pub` will be part of its public API once packages exist.
- The "private type in a public interface" warning compares only private and `pub`.
