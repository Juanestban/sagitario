# 0107. A seed for each host

- Status: Proposed
- Date: 2026-11-14
- Supersedes: Q363, Q364 and Q371 of [ADR-0067](0067-bootstrap-from-the-seed.md) (one seed for
  every host, and the same C of the compiler on every system)
- Refines: [ADR-0085](0085-conditional-compilation-and-std-time.md) (`@cfg` in the standard
  library), [ADR-0104](0104-include-str-as-a-macro.md) (Q741: the seed's units)

## Context

ADR-0067 made one seed, `bootstrap/sag.c`, for every 64-bit host: in October the C that the
compiler generated for itself was the same on every system, since what differed between systems
was in the C runtime, behind `#ifdef`. CI checks it: each bootstrap job hashes the C of stage 2,
and a last job, `same C on every system`, requires one hash.

Since S1 ([ADR-0085](0085-conditional-compilation-and-std-time.md)), `std::sys` reaches each system
through `@cfg`, so the C the compiler generates for itself holds the API of the system it was
generated on: the C of Windows calls `CreateFileW`, and the C of Linux on AArch64 has an unsigned
`char`. ADR-0085 noted that the compiler could not use `@cfg` until the seed was updated. The seed
was not updated until [ADR-0104](0104-include-str-as-a-macro.md), on Windows, so it kept building
everywhere; once it was, every bootstrap but Windows' failed: macOS links symbols of Win32, and
Linux on AArch64 passes `int8_t` where its `char` is `unsigned char`. The job `same C` cannot pass
either, the C of stage 2 being different on each system by design.

The seed of October cannot come back: it cannot parse `include_str!` nor the other macros, and its
`std` has no `fs::read`, which the compiler uses now.

The user chose a seed for each host.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q765 | Seeds | A seed for each host CI and the releases build on: `bootstrap/seed/<host>/` with `sag.c` and its FFI units (`sag_ffi.c`, `sag_std_ffi.c`), for `windows-x86_64`, `linux-x86_64`, `linux-aarch64`, `macos-aarch64` and `macos-x86_64`. The copy of the runtime, `bootstrap/sagitario_runtime.c` and `.h`, is one for all, as the runtime is portable C. |
| Q766 | Generating them | Generating C needs no toolchain, so one machine generates every seed: `sag check --dump=c --seed-host <host>` (an option of the compiler, hidden as `--dump`) generates the compiler's C for that host, whose `os`, `arch`, `long` and `char` it takes as `TargetInfo::host` would find them there. `--update-seed` writes the five. |
| Q767 | Choosing | `bootstrap/build.sh` finds the host with `uname -s` and `uname -m`, `bootstrap/build.ps1` with `PROCESSOR_ARCHITECTURE`, and they build stage 0 from its seed. A host without one is an error that names the hosts there are. |
| Q768 | CI | The fixed point stays: stages 1 and 2 generate the same C. Each bootstrap job also checks that `--seed-host` of its own host generates the same C as stage 2, so that a seed generated on another machine is the one this host would generate. The job `same C` compares the jobs of each host: the same C with every C compiler of a host. |
| Q769 | Git | The seeds are marked `-diff linguist-generated`, as `bootstrap/sag.c` was. |

## Alternatives considered

- **One seed with the C of every host behind `#if`:** one file, but of 60 to 90 MB, close to
  the 100 MB GitHub takes.
- **The seed of October, and a compiler it can still compile:** the compiler's sources would
  take back `include_str(...)` without `!`, reverting part of ADR-0104, and could never use
  anything newer than October.

## Consequences

- `bootstrap/` holds five seeds of about 15 MB each, which Git stores as deltas when they change.
- Supporting a new host takes a seed for it.
- The seed is updated as rarely as before (Q369), now for every host at once.
