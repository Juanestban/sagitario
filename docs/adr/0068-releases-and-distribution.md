# 0068. Releases and distribution

- Status: Accepted
- Date: 2026-10-27
- Refines: [ADR-0063](0063-distribution-phase-and-roadmap-v2.md) (phase F7, Q340: its open
  questions)

## Context

F6 left Sagitario building from a seed of generated C with a C compiler alone
([ADR-0067](0067-bootstrap-from-the-seed.md)), and `--install` already puts a build from source
in `~/.sagitario/bin`. F7 distributes it: releases, installers, `nova update self` and the editor
(ADR-0063). ADR-0063 left four questions to settle when the phase starts. Measured before this
record: `sag` holds the standard library and the runtime, so a release is two executables; built
with the official LLVM clang on Windows, they import only `KERNEL32.dll`; and the version
`0.8.0` was written in three places (`sag`, its language server, Nova) and in
`compiler/Nova.toml`.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q379 | Steps | The four increments of the roadmap, each verified on its own: **1** releases, **2** installers, **3** `nova update self`, **4** the VS Code extension in the Marketplace. This record settles the open questions of the phase and increment 1; the others add tables only for what is new. |
| Q380 | One version | `compiler/src/version.sag` holds `VERSION`, which `sag --version`, the `serverInfo` of `sag lsp` and `nova --version` report. `compiler/Nova.toml` says the same, which the harness checks, and so does the tag of a release, which the release workflow checks. |
| Q381 | Platforms and archives | Five platforms, named with the triples `--target` uses: `x86_64-pc-windows-msvc` (a `.zip`), `x86_64-unknown-linux-gnu`, `aarch64-unknown-linux-gnu`, `aarch64-apple-darwin` and `x86_64-apple-darwin` (a `.tar.gz`). The archive `sagitario-<version>-<triple>` holds the directory of the same name, with `bin/sag`, `bin/nova`, `LICENSE-MIT`, `LICENSE-APACHE` and `README.md`. |
| Q382 | How a release builds | In release mode from the seed, with clang, on the native runner of each platform: Windows with the official LLVM clang, whose executables link the C runtime statically; Linux on Ubuntu 22.04 (x86-64 and AArch64), the oldest runners, so that the executables need glibc 2.35 rather than a newer one; macOS on the runners for AArch64 and x86-64, with `MACOSX_DEPLOYMENT_TARGET=11.0`. |
| Q383 | Packaging | `--dist` in `bootstrap/build.sh` and `bootstrap/build.ps1` writes the archive of the host to `build/dist/`, so a release can be made and checked locally. It builds in release mode, and refuses `--debug`. |
| Q384 | The workflow | `.github/workflows/release.yml` runs on a tag `v*`. Each platform checks that the tag is `v` and the version of `compiler/Nova.toml`, builds with `--dist`, runs the harness with what it built (some platforms are not in CI) and keeps its archive. A last job writes `SHA256SUMS` (the format of `sha256sum`) and publishes everything with `gh release create --generate-notes --verify-tag`, as a release, not a prerelease, so that it is the latest. It uses no action that CI does not already use, and the token of the workflow with `contents: write`. |
| Q385 | The first release | `v0.8.0`, the current version, tagged by the author once increment 1 is on `main`. |
| Q386 | HTTPS | Through the `curl` of the system, which Windows 10 (since 1803), macOS and Linux distributions ship: the installers run it, and Nova runs it with `Command`. F7 adds no `std::net`. |
| Q387 | SHA-256 and JSON | Inside the compiler, not in the standard library: a new `compiler/src/sha256.sag`, and `compiler/src/lsp/json.sag` moved to `compiler/src/json.sag`, shared by the language server and Nova. `std::hash` and `std::json` wait for records of their own. The installers check with `sha256sum`, `shasum` or `Get-FileHash`. |
| Q388 | The C compiler | Required, not bundled. The installers look for one (clang, gcc or `cc`; on Windows LLVM or `cl`) and say how to install one on each system. |
| Q389 | Signing | The executables are not signed in F7. Installers that download with `curl` instead of a browser should avoid the warnings of SmartScreen and Gatekeeper; increment 2 checks that on each system and documents what remains. Signing needs certificates, and an Apple developer account for notarization. |

## Alternatives considered

- **Building the releases on the newest runners:** the Linux executables would need the glibc of
  Ubuntu 24.04, which older distributions lack.
- **Linking statically on Linux (musl):** one executable for every distribution, but `sag` runs a
  C compiler of the system anyway, and its sanitizer and thread tests use the glibc toolchain.
- **Packaging in the workflow alone:** a release could not be reproduced locally.
- **`std::net`, `std::hash` and `std::json` now:** they are library design, larger than F7 needs.
- **Bundling clang:** large, and its licence and the Windows SDK need care.

## Consequences

- One edit to `version.sag` and `compiler/Nova.toml` makes a new version; a tag publishes it.
- The release workflow is the only one that builds on Linux AArch64 and macOS x86-64.
- Until increment 2, a release is installed by extracting its archive and adding `bin/` to `PATH`.
