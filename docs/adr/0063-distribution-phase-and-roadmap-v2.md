# 0063. Distribution as phase F7, and the roadmap v2 of a game engine

- Status: Accepted
- Date: 2026-10-26
- Supersedes: the number of the phase of `async`/`await` in Q293 of
  [ADR-0057](0057-editor-tooling-and-independence-from-rust.md) (F7 becomes F8)

## Context

Once F6 removes Rust, Sagitario is still installed by building it from the repository. To program
with it as one does with Rust, it needs releases, installers and a way to update itself. And the
goal the language serves needs a plan of its own: a game engine, with windows on every system,
OpenGL and Vulkan, audio, physics, networking and tools.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q340 | Distribution | A new phase **F7, distribution and updates**, after F6: releases of the whole toolchain under one version, published by CI as GitHub Releases of the repository with checksums; install scripts for Windows, Linux and macOS into `~/.sagitario/bin`; `nova update self`, which updates `sag`, Nova and the standard library from the latest release; and the VS Code extension published with each release. Its open questions (HTTPS through `curl` or `std::net`, SHA-256 and JSON in `std` or in Nova, bundling a C compiler, signing) are settled when it starts. |
| Q341 | `async`/`await` | Moves from F7 to F8, after distribution. |
| Q342 | Roadmap v2 | `docs/roadmap-v2.md` plans a game engine in phases `G1`–`G9`: language and library prerequisites, windows and input, graphics (OpenGL, Vulkan), engine core and assets, audio, physics, networking, tools, and shipping games. Each phase is planned with its own decision table and records when it starts; until then the roadmap changes nothing in the language. |

## Consequences

- `async` and `await` stay reserved keywords until F8.
- Several prerequisites of the engine (operator overloading, slices, binding generation,
  mutable globals) are deferred decisions of the first roadmap; G1 takes them up.
