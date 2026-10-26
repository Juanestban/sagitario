# 0060. The pivot without Rust, and the driver for Nova

- Status: Accepted
- Date: 2026-10-26
- Refines: [ADR-0057](0057-editor-tooling-and-independence-from-rust.md)

## Context

The third part of F6 develops Sagitario without Rust (ADR-0057, Q303): the compiler, Nova, the
test harness and CI all in Sagitario or C. Today Rust holds the compiler `sag`, Nova (a TOML
parser, manifests and packages, about 1 900 lines), what only Nova reached of the driver
(libraries and their C headers, dependencies, `[link]`), the harness of the suites and of the
self-hosted compiler, 157 unit tests and a CI built on `cargo`. Q322–Q333 plan the part; the
first increment, 3a, gives `sagc` what Nova needs.

## Decision

### The plan

| # | Topic | Decision |
|---|---|---|
| Q322 | Increments | **3a**: `sagc` reaches `sag` in what only Nova used: libraries (static and dynamic, with their C header), dependencies and `[link]`. **3b**: Nova in Sagitario. **3c**: the test harness in Sagitario. **3d**: the seed, CI without `cargo`, `sagc` renamed `sag` and `crates/` removed. |
| Q323 | How Nova calls the compiler | Nova is a program of its own (`nova/`), which runs `sag` as a process, as Cargo runs rustc. `sag` takes options that Nova passes and the usage text does not show (below). |
| Q324 | Bootstrapping without Rust | A **seed** is committed: the C `sag` generates for itself, `bootstrap/sag.c`, with the runtime, so that a C compiler alone builds stage 0. It is regenerated when the compiler needs something the seed cannot compile, not on every commit, as Rust's stage0; Git stores its versions as deltas. |
| Q325 | One seed | The same `bootstrap/sag.c` on Windows, Linux and macOS: the generated C does not depend on the machine that generates it, which CI checks on the three. |
| Q326 | Bootstrap | A script per platform (`bootstrap/build.sh`, `bootstrap/build.ps1`): the seed compiled by `cc` is stage 0, stage 0 compiles stage 1, stage 1 stage 2, and stages 1 and 2 must generate the same C. |
| Q327 | Tests | The harness becomes a Sagitario program, `tests/harness/`, with the same annotations, the sessions of the language server, the keywords of the editor and the tests of Nova. The differential tests against Rust go with Rust; expected dumps (`.expected`) of the corpus of each pass take their place, rewritten with `SAG_BLESS=1`. |
| Q328 | Unit tests of Rust | Retired with their code: they test the internals of the Rust compiler. The suites and the expected dumps cover what they check; `@test` stays deferred. |
| Q329 | Modules made from Rust | `compiler/src/diagnostics/codes.sag` becomes the source of the registry of codes; `compiler/src/unicode.sag` stays committed data, with a note on how to regenerate it. |
| Q330 | The standard library | `prelude.sag` and `std/*.sag` move from `crates/sagitario_driver/src/` to `compiler/std/`, next to the compiler that embeds them. |
| Q331 | CI without `cargo` | The same systems and C compilers: the bootstrap from the seed, the fixed point, the whole harness with sanitizers where they exist, and the packaging of the extension. `cargo fmt` and clippy go; the compiler checks itself with warnings as errors. A formatter (`sag fmt`) comes later. |
| Q332 | Retiring Rust | A record superseding Q214 of ADR-0049 retires the Rust compiler and removes `crates/`, `Cargo.toml`, `rustfmt.toml` and `clippy.toml`. Records about Rust stay as history; the architecture, `CONTRIBUTING.md` and the README are rewritten. |
| Q333 | `CLAUDE.md` | Its rules about Rust are rewritten for the compiler in Sagitario in 3d, with the text proposed to the author first. |

### Increment 3a: the options Nova passes

`sag build`, `run` and `check` take, without showing them in the usage text:

| Option | Meaning |
|---|---|
| `--library`, `--name <name>` | The input is the root of the library `name`: it needs no `main`, and `build` makes its libraries and its header. |
| `--output <file>` | Where `build` and `run` write the executable; `--emit-c` writes the C next to it. |
| `--out-dir <dir>`, `--static`, `--dynamic` | Where `build --library` writes which libraries; without `--static` or `--dynamic`, both. It prints each file written, one per line. |
| `--dependency <name>=<entry>` | The next package the input depends on, directly or not, numbered from 0. |
| `--dependency-uses <index>=<indices>` | The dependencies of dependency `index` (`1=0,2`). |
| `--uses <indices>` | The input's own dependencies. An index that no `--dependency` gave is a usage error. |
| `--link-lib <name>`, `--link-search <dir>`, `--link-source <file>` | What `[link]` adds: libraries, where to find them and C sources. |

The loader of `sagc` loads a package per dependency, each with its root and files, as the Rust
driver does; the driver builds static and dynamic libraries with the linker's export lists and
the C header (`compiler/src/driver/header.sag`), compiles the package's C sources, and reports a
failure of them or of linking with `[link]` as an error of the package, not of the compiler.

## Alternatives considered

- **Nova as a part of `sag` (`sag nova build`):** one program, but the compiler would carry TOML
  and the package graph, which only Nova needs.
- **Downloading the previous release to bootstrap:** no seed in the repository, but it needs the
  network, published releases and a binary per platform.
- **A file describing the package graph instead of options:** one more format; the graph of a
  build fits options.

## Consequences

- Built with the options, the F3 acceptance gives what Nova in Rust gives: the same files and
  the same header for `textcount`, the same output of `wc`, and the library linked from C
  statically and dynamically.
- Until Nova is ported (3b), the options are only exercised by hand and by the test of 3b.
