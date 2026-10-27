# 0065. Modules with submodules in `mod.sag`

- Status: Accepted
- Date: 2026-10-26
- Supersedes: the files of modules in [ADR-0024](0024-modules-imports-and-visibility.md) (a
  module with submodules was `name.sag` next to the directory `name/`)

## Context

A module with submodules was the file `name.sag`, next to the directory `name/` holding them. In
`compiler/src/` eight modules looked like this (`sir.sag` and `sir/`, `types.sag` and `types/`...),
and finding a module meant looking in two places: the file, or the directory. Rust lets a module
be `name/mod.rs`, inside its directory, and its developers find each module in one place.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q349 | The rule | A module without submodules is `name.sag`; a module with submodules is `name/mod.sag`, in the directory that holds them. A directory without `mod.sag` is still an empty module that only holds submodules. |
| Q350 | The old form | `name.sag` next to a directory `name/` is an error, `E0209`, pointing at `name.sag`: "move `name.sag` to `name/mod.sag`", or, when `name/mod.sag` exists too, "merge `name.sag` into `name/mod.sag`". A module has one place for its file. |
| Q351 | The name `mod` | `mod` cannot name a module, since `mod.sag` is always the file of its directory's module: `pkg::mod` names nothing (`E0200`, with a note saying why). `mod` is not a keyword and names anything else. |
| Q352 | The root | The entry file (`main.sag`, `lib.sag` or the file given to `sag`) is the root module whatever its name; a `mod.sag` next to it is no module. |
| Q353 | Implementation | Both compilers, their loaders and the suggestion of `E0200` ("the file `x.sag`, or `x/mod.sag` with its submodules"), with `tests/run-pass/module_directories/`, `tests/compile-fail/module_layout/` (both forms of `E0209`) and `tests/compile-fail/mod_module.sag`; the specification of modules. |
| Q354 | `compiler/src/` | The eight modules move to `X/mod.sag` (`ast`, `diagnostics`, `driver`, `lsp`, `resolve`, `sir`, `types`); their `include_str` paths gain a `../`. Nova's root module becomes `compiler/src/nova_main.sag` (built with `--output nova`), its modules staying in `compiler/src/nova/`, a directory without `mod.sag`. |
| Q355 | The standard library | The file of `std` itself becomes `std/mod.sag`, in both compilers and in how the language server recognizes the sources of `std` (ADR-0062). |

## Alternatives considered

- **Both forms allowed, as Rust 2018 does:** the confusion that prompted the change would stay.
- **Only `name/mod.sag`, even for modules without submodules:** a directory per module, for
  modules that are one file.

## Consequences

- `tests/resolve/program/net/` and `tests/run-pass/reference_modules/bank/` move their module
  files to `mod.sag`.
- Every file of a module with submodules is inside its directory, so the explorer of an editor
  shows a module and its submodules together.
