# 0042. Library outputs and linking C

- Status: Accepted
- Date: 2026-10-24
- Builds on: [ADR-0026](0026-ffi-details.md) (exported functions), [ADR-0040](0040-nova-package-manager.md),
  [ADR-0041](0041-dependencies-by-path.md)

## Context

F3 increment 6 turns library packages into files that C programs link, and lets packages link
C libraries and sources. Until now a library was only checked, `extern "C" fn` exported symbols
only within the program, and the C standard library was the only one linked. The generated C
must stay strict C11, so symbol visibility cannot use compiler attributes. Increment 6 settles
Q140–Q149.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q140 | Outputs | `nova build` in a library writes a static and a dynamic library into `target/<profile>/`; `[library] outputs` chooses. Names per platform: `libgeo.a`/`libgeo.so` (Linux), `libgeo.a`/`libgeo.dylib` (macOS), `geo.lib`/`geo.dll` with `geo.dll.lib` (MSVC, Rust's naming, so the import library does not clash with the static one), `libgeo.a`/`geo.dll` with `libgeo.dll.a` (MinGW). |
| Q141 | Exports | The `extern "C" fn` functions of the whole graph. A library is lowered without `main`, so endless generic instantiation is reported for libraries too. A library without exports builds with a warning. |
| Q142 | Visibility | A dynamic library exports only those functions, through linker inputs rather than C extensions: a module-definition file on Windows, a version script on Linux, an exported symbols list on macOS. |
| Q143 | Runtime and panics | Each library contains the runtime. A panic ends the host process with 101; nothing unwinds into C. Two Sagitario static libraries cannot be linked together, since their runtimes clash; dynamic ones can. |
| Q144 | Header | `target/<profile>/geo.h`: an include guard, `extern "C"` for C++, the standard integer headers, the `@repr(C)` structs used and the prototypes, with `///` comments copied. |
| Q145 | Archiver | `SAG_AR`, then `llvm-ar` next to the C compiler, then `llvm-ar` or `ar` on `PATH`. |
| Q146 | `[link]` | `libraries` (`-l`), `search_paths` (`-L`, relative to the package) and `sources` (C files compiled as C11 and linked), for programs and libraries, accumulated through dependencies. |
| Q147 | Options | `--release` and `--emit-c` work for libraries; `--sanitize` is rejected, since hosts would need the sanitizer runtime. |
| Q148 | Tests | Nova integration tests build a library and a C program using it, linked statically and dynamically, and run it; they check the header, the chosen outputs, `[link]` sources and libraries, and the errors. |
| Q149 | Documentation | This ADR, `docs/nova.md`, the FFI chapter of the specification and the roadmap. |

The header spells types with fixed widths (`int32_t` for both `i32` and `c_int`): the type
checker resolves `c_int` to `i32` before the header is written, and both have the same ABI on
every supported target.

## Alternatives considered

- **`__attribute__((visibility))` and `__declspec(dllexport)`:** compiler extensions, which the
  generated C avoids.
- **Only a static library by default:** C hosts and other languages (Swift, Kotlin via JNI)
  more often load dynamic libraries; building both costs one link.
- **Exporting only the root package's functions:** the dependencies' exported functions are C
  symbols of the same binary anyway.
- **A separate runtime library shared by all Sagitario libraries:** solves the clash of static
  libraries but adds a file to distribute; it can come later.

## Consequences

- Builds compile objects first and link or archive them second. This separates internal
  compiler errors (the generated C) from errors of the package (its C sources and libraries).
- `CompileOptions` carries the package's name and its `Link`, and `build_library` returns the
  files it wrote.
- The macOS path (exported symbols list, `@rpath` install name) is first exercised in CI; the
  Linux path was checked by building with the Android NDK's clang.
- MinGW gcc reports the target `x86_64-w64-mingw32`, without "windows": the driver now detects
  Windows, MinGW and Apple targets in one place.
