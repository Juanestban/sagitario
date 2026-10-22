# 0018. Windows C toolchain: official LLVM (MSVC target), MinGW clang tolerated

- Status: Accepted
- Date: 2026-10-22
- Refines: [ADR-0015](0015-compiler-architecture-and-toolchain.md)

## Context

ADR-0015 made clang with UCRT and the MSVC linker the official Windows toolchain. Windows
machines often also have an MSYS2/MinGW clang (`x86_64-w64-windows-gnu`) earlier on `PATH`,
so plain `clang` may resolve to it. Checked on the main development machine:

- Both clangs compile and run strict C11.
- Official LLVM (`x86_64-pc-windows-msvc`) builds AddressSanitizer binaries; running them
  only needs the sanitizer runtime DLL directory on `PATH`.
- MSYS2 MINGW64 clang ships no ASan runtime, so ASan builds fail to link, and it links
  against the legacy `msvcrt.dll`, whose `printf` differs from C99/C11 (e.g. `%zu`, `%lld`).

`sag` invokes the C compiler as an external process; no LLVM libraries or bindings are used.

## Decision

- **Supported on Windows:** official LLVM clang targeting `x86_64-pc-windows-msvc`, with
  Visual Studio Build Tools (already required by the Rust MSVC toolchain). This is what CI
  tests.
- **Tolerated, best-effort:** MinGW clang (`*-windows-gnu`). It may be used, but it is not
  tested in CI and sanitizers are unavailable.
- Detection keeps the ADR-0015 order (`SAG_CC` / `--cc`, then `clang`, then `gcc`). On Windows
  the driver identifies the compiler with `<cc> -dumpmachine`; for a `windows-gnu` target it
  warns that the toolchain is best-effort and that sanitizers are disabled.
- When running sanitized programs (tests), the driver adds the clang sanitizer runtime
  directory to the child process `PATH`.
- Developers whose `PATH` resolves `clang` to MinGW set `SAG_CC` to the LLVM clang (or reorder
  `PATH`).

## Alternatives considered

- **MinGW clang as the official Windows toolchain**: avoids nothing on machines that already
  have the Rust MSVC toolchain, loses ASan (required by the F2 acceptance criteria), and
  brings legacy CRT quirks.
- **Supporting both equally in CI**: doubles the Windows matrix for little benefit in the MVP.

## Consequences

- The detection and warning are implemented in F1, when the driver first invokes a C compiler.
- MinGW users get working builds without memory checks.
