# 0067. The bootstrap from the seed, and retiring Rust

- Status: Accepted
- Date: 2026-10-27
- Refines: [ADR-0060](0060-driver-parity-for-nova.md) (the seed and its bootstrap, Q324–Q326; the
  standard library, Q330; CI, Q331; retiring Rust, Q332) and [ADR-0044](0044-c-toolchains.md)
  (the options of `cl`, Q163)
- Supersedes: Q214 of [ADR-0049](0049-self-hosting-plan-and-language-prerequisites.md) (the Rust
  compiler)

## Context

Increment 3d of F6 ends the pivot without Rust (ADR-0060): the compiler in Sagitario builds from a
seed of generated C with a C compiler alone, CI runs without `cargo`, `sagc` takes the name `sag`
and `crates/` goes. ADR-0060 fixed the outline; this record fixes the details. Measured before it:
the C that `sagc` generates for itself is one file of 8.9 MB, which already holds the standard
library and the runtime (`include_str`) and needs no C file for FFI; it is the same for every
64-bit target, and differs for 32-bit ones, whose `usize` bounds it writes. The compiler, Nova and
the harness have no warnings.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q362 | Steps | Three, each verified on its own. **3d-1**: the seed, the scripts and CI from the seed, with Rust still in place. **3d-2**: `sagc` renamed `sag`, the standard library in `compiler/std/`, and `codes.sag` as the source of the codes. **3d-3**: `crates/` and Cargo removed, the documentation rewritten, and `CLAUDE.md` proposed to the author. |
| Q363 | The seed | `bootstrap/sag.c`, the C of the compiler, with a copy of the runtime it was generated with (`bootstrap/sagitario_runtime.c` and `.h`): a later change to `runtime/` does not break it, and both are updated together. |
| Q364 | Hosts | The seed builds on 64-bit hosts: Windows, Linux and macOS on x86-64 and AArch64. A 32-bit host cannot build it; 32-bit Android stays a target only. |
| Q365 | Git | `bootstrap/sag.c` is marked `-diff linguist-generated`: its changes show no diff, GitHub does not count it as source, and Git stores each version as a delta. |
| Q366 | The scripts | `bootstrap/build.sh` (Linux and macOS; also Git Bash on Windows) and `bootstrap/build.ps1` (Windows, with clang, MinGW or `cl`) take the C compiler from `SAG_CC`, else `cc` and `clang`. The seed compiled with the options of a release build (`-std=c11 -pedantic-errors -O2 -DSAG_RELEASE`, with the stack and libraries `sag` links) is stage 0; stage 0 builds stage 1, stage 1 builds stage 2, and stages 1 and 2 must generate the same C for the compiler. Stage 2 then builds Nova. Everything goes to `build/` (ignored by Git): `build/bin/sag` and `build/bin/nova`. Both scripts take the same options. |
| Q367 | Build mode | Stages 1 and 2 build with `--release`; `--debug` builds them in debug, where an integer overflow in the compiler panics, which CI uses. |
| Q368 | Installing | `--install` copies `sag` and `nova` to `~/.sagitario/bin` (`%USERPROFILE%\.sagitario\bin` on Windows) and adds it to `PATH` once: the `PATH` of the user on Windows, a line in `~/.profile` elsewhere, and in `~/.zshrc` too on macOS. On Windows a running executable is renamed before it is replaced. F7's installers use the same directory, downloading instead of building. |
| Q369 | Updating the seed | `--update-seed` writes the C of stage 2 and the current runtime to `bootstrap/`. The seed is updated when the compiler needs something the seed cannot compile (Q324), or when the driver it carries must change (Q375); CI does not require it to be current. |
| Q370 | Warnings as errors (Q331) | The scripts fail if `sag check` reports anything for the compiler, Nova or the harness. No option of the compiler is added for it. |
| Q371 | CI | Each `bootstrap` job (the systems and C compilers of today) runs the script with `--debug` and the whole harness with stage 2 and its Nova, with sanitizers and the NDK where they exist, and publishes the SHA-256 of the C of stage 2; a last job checks that all are equal (Q325). In 3d-3 the `cargo` jobs (`test`, `gcc`, `cl`, `msrv`) go; the extension's job stays. |
| Q372 | The name | The executable, its version (`sag 0.8.0`), its usage text, the `serverInfo` of the language server, `compiler/Nova.toml`, the default of the VS Code extension (`sag lsp`) and the harness say `sag`. Nova no longer looks for `sagc`. `sagc` and its Nova said `0.4.0` until 3d-1, behind the `0.8.0` of the workspace; they now say `0.8.0` too. |
| Q373 | Library and codes | `prelude.sag` and `std/*.sag` move to `compiler/std/` (Q330). `compiler/src/diagnostics/codes.sag` becomes the source of the registry; `compiler/src/unicode.sag` stays committed data, with a note saying where it came from (Q329). |
| Q374 | Retiring Rust (Q332) | This record supersedes Q214 of ADR-0049: the Rust compiler retires in 3d-3, with `crates/`, `Cargo.toml`, `Cargo.lock`, `rustfmt.toml` and `clippy.toml`. `docs/architecture.md`, `docs/testing.md`, `CONTRIBUTING.md`, the README and the READMEs of `tests/` are rewritten; the records about Rust stay as history. The text of `CLAUDE.md` is proposed to the author first (Q333). |
| Q375 | `cl` in release | `cl` optimizes with `/O1` instead of `/O2` (Q163 of ADR-0044). The `/O2` of Visual Studio 18 miscompiles the compiler: a `switch` on the `uint32_t` tag of a union checks the range on 32 bits, then indexes its jump table with the whole 64-bit register, whose upper half holds the next field; stage 0 crashes. ASan and UBSan with clang find nothing, and clang and gcc at `-O2` and `cl` at `/O1` and `/Od` build a working compiler. |
| Q376 | Embedded files | The files the compiler embeds with `include_str` (the runtime, the prelude and the standard library) and `bootstrap/` keep LF line endings in every checkout: with `core.autocrlf`, Windows embedded them with CRLF, and the C differed between systems. |
| Q377 | Paths in `#line` | The paths of `#line` directives use `/` on every system, in both compilers: the loader joins paths with `\` on Windows, which reached the C. A panic on Windows reports its file with `/`. |

## Alternatives considered

- **One step for 3d:** the seed, the rename and the removal of Rust in one change, which would be
  hard to verify and to review.
- **The seed with the runtime of `runtime/`:** fewer files, but a change to the runtime would break
  the seed until it is regenerated.
- **An option `--deny-warnings` in the compiler:** an option for one use, which the scripts do
  without.
- **Keeping `/O2` with `cl`:** `sag build --release --cc cl` would build a compiler that crashes.
  Generating the C differently would avoid this pattern only, and change every dump of the C.
- **Normalizing line endings in `include_str`:** it would change what the function returns, which
  is the file as it is.

## Consequences

- The repository holds about 9 MB of generated C; each new seed adds a delta.
- Building Sagitario needs a C compiler only: about two minutes with the scripts.
- Until 3d-3, Cargo's tests run beside the bootstrap jobs.
- Programs built with `cl` in release are optimized less than with `/O2`.
