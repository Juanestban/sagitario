# 0064. Nova in Sagitario

- Status: Accepted
- Date: 2026-10-26
- Refines: [ADR-0060](0060-driver-parity-for-nova.md) (where Nova's sources live, Q323)

## Context

Increment 3b of F6 ports Nova (`crates/sagitario_nova`: a TOML parser, the manifest, the graph of
packages and the commands) to Sagitario. Q323 made it a program of its own that runs `sag`. Nova
renders the errors of `Nova.toml` as the compiler renders its own, with the same spans,
excerpts and colors, so it needs the compiler's `span` and `diagnostics`; and a Nova package
is a program or a library, not both, so the compiler cannot be a library Nova depends on.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q343 | Where | Nova's root module is `compiler/src/nova.sag`, built with `sag build compiler/src/nova.sag`, and its modules are `compiler/src/nova/{codes, toml, manifest}.sag`. It shares the compiler's source tree, and imports `pkg::span`, `pkg::diagnostics` and `pkg::lexer` (for keywords); it is still a program of its own, which runs `sag` as a process (Q323). Choosing colors (`diagnostics::use_color`) moves to `diagnostics.sag`, so Nova does not bring in the driver. |
| Q344 | Which `sag` | `SAG` when set; else `sag`, or `sagc` until the rename, next to Nova's executable; else `sag` on `PATH`. |
| Q345 | What Nova passes | The options of ADR-0060: the root module, `--name`, `--library`, the dependencies with `--dependency`, `--dependency-uses` and `--uses`, `[link]` of every package with `--link-lib`, `--link-search` and `--link-source`, and `--output` (programs) or `--out-dir`, `--static` and `--dynamic` (libraries). It finds the targets of `--target` in the output of `sag targets`. |
| Q346 | Library builds | Nova reads the files `sag build --library` prints, and passes its stderr on; it sets `CLICOLOR_FORCE` for `sag` when Nova itself would color, so diagnostics keep their colors. |
| Q347 | Package identity | Without `canonicalize` in `std::fs`, a package is recognized by its directory made absolute and tidied (`.` and `..` resolved), in the form of `path_key`; symbolic links are not followed. |
| Q348 | Tests | Until the harness is ported (3c), the end-to-end tests of the Rust Nova (`crates/sagitario_nova/tests/nova.rs`) run any Nova given in `NOVA_BINARY`; the `bootstrap` CI job builds Nova with stage 1 and runs them with it. |

## Consequences

- Nova in Sagitario passes the 21 end-to-end tests of the Rust Nova with official clang (with
  sanitizers), gcc, MinGW clang and `cl`, the cross-compilation for Android and the F3
  acceptance included.
- `nova clean` removes the target directory file by file, since `std::fs` removes empty
  directories only.
