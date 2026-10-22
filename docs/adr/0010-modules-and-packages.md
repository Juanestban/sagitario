# 0010. File-as-module, logical imports, packages

- Status: Accepted
- Date: 2026-10-21

## Context

Module design affects name resolution, project layout and future incremental compilation.

## Decision

- Each file is a module implicitly; directories define the hierarchy.
- `use` with logical paths rooted at `pkg::`, `std::` or a dependency name.
- Items are module-private by default; `pub(pkg)` and `pub` widen visibility.
- Packages are described by `sagitario.toml`, with `src/main.sag` / `src/lib.sag`.
- Cycles allowed within a package, forbidden between packages.
- File extension `.sag`.

## Alternatives considered

- **Explicit `mod` declarations (Rust)**: redundant with the file system.
- **Relative-path imports (TypeScript)**: fragile under refactoring.
- **Capitalization-based visibility (Go)**: conflicts with the naming conventions.

## Consequences

- Layout is predictable from paths alone.
- The compiler must analyze a whole package together to support intra-package cycles.
