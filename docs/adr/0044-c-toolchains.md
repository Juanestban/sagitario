# 0044. C toolchains: MSVC `cl` best-effort, no `zig cc`, and the F3 acceptance program

- Status: Accepted
- Date: 2026-10-24
- Refines: [ADR-0015](0015-compiler-architecture-and-toolchain.md) (the re-evaluation it
  deferred to F3), [ADR-0018](0018-windows-c-toolchain.md) (`cl` joins MinGW as a tolerated
  Windows compiler) and [ADR-0042](0042-library-outputs-and-linking-c.md) (`c_char` in headers)

## Context

ADR-0015 made clang the official C compiler and left MSVC `cl.exe` and `zig cc` for F3. F3
increment 8 settles them and closes F3 with its acceptance program. Q162–Q170.

Before deciding, the reference zoo of F2 was compiled with `cl` 19.50 (Visual Studio 2026)
using `/std:c11 /W4`. It built without changes and printed the expected output. The 61
warnings were harmless: unused parameters, unreachable code, tentative `const` definitions,
and flexible array members, which MSVC calls an extension although C99 defines them. One
other program failed: MSVC rejects dividing a floating-point constant by a constant zero
(`0.0 / 0.0`), which C11 with IEEE 754 defines as NaN or an infinity.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q162 | `cl` | Supported best-effort, like MinGW. It builds programs and libraries without sanitizers, since MSVC has no UndefinedBehaviorSanitizer. The official Windows compiler stays LLVM clang. |
| Q163 | Using `cl` | `SAG_CC=cl` or `--cc cl`, from a Developer Command Prompt: `cl` finds headers and libraries through `INCLUDE` and `LIB`, and without `INCLUDE` the driver reports how to set them up. `cl` has no `-dumpmachine`, so it is recognized by its file name. Options are `/std:c11 /W3 /utf-8`, plus `/Od /Z7` (debug) or `/O2` (release). Static libraries come from `lib.exe` next to `cl`. Dynamic libraries come from `/LD`, with the same `.def` file and names as clang for MSVC. `--sanitize` warns and builds without sanitizers. It needs Visual Studio 2019 16.8 or newer. |
| Q164 | CI for `cl` | A `test (windows-latest, cl)` job runs the whole suite with `SAG_CC=cl` and `SAG_TEST_SANITIZE=0`. Its environment comes from `vswhere` and `vcvars64.bat`, without third-party actions. |
| Q165 | `zig cc` | Not adopted: not official, not bundled, and not given specific support. Zig is still pre-1.0 and would be a large binary dependency. It also does not ship what the mobile targets need (Android's bionic, Apple's iOS SDK), so it cannot replace the NDK or Xcode. Re-evaluate with Zig 1.0, or when cross-compiling between desktop systems, where it helps most. `SAG_CC` names a single program, so `zig cc` would need a wrapper script; that use is untested. |
| Q166 | Acceptance program | `wc`, a small `wc` built as a package. It prints the lines, words and bytes of each file and their total, or of stdin when given no file. A missing file is reported and gives exit code 1. The counting is in the local library package `textcount`. |
| Q167 | The library from C | `textcount` also exports C functions (`textcount_lines`, `textcount_words`) that take a C string. A C program includes the generated `textcount.h`, links the static and the dynamic library, and compiles with warnings as errors. |
| Q168 | Location and test | `tests/acceptance/f3/` holds both packages and the C program. The Nova test `f3_acceptance` copies them to a temporary directory and runs `wc` with sanitizers, on files and on stdin. It then builds the library, runs the C program with each library, and builds the tool and the library for the three Android targets (when the NDK is found). For iOS, which runs no standalone programs (ADR-0043), it checks the library and builds it on macOS. |
| Q169 | Closing F3 | This ADR, plus the roadmap (F3 done), `architecture.md`, `testing.md`, `CONTRIBUTING.md` and `docs/nova.md`. |
| Q170 | `c_char` in headers | A library's header spells `c_char` as `char`, by looking at the type written in the signature or the field. Other C types keep their fixed-width spelling. This changes only how the header writes the type, not the ABI. `int8_t` (`signed char`) is not compatible with `char` in C, so callers passing a `const char *` got warnings, and C++ callers got errors. |

## Alternatives considered

- **Leaving `cl` unsupported:** the generated C already compiled with it. Supporting it is
  mostly a second set of command-line options, kept in `Toolchain`.
- **`cl` with AddressSanitizer:** MSVC has `/fsanitize=address`, but the test harness's
  guarantees need UBSan too. A sanitized build that only half-checks would be misleading.
- **Avoiding the constant division in the type checker or the generated C:** folding it would
  need NaN and infinity constants in C. A `static inline` helper is called only for
  constant/constant divisions, and clang and gcc inline it.
- **Bundling `zig cc` for cross-compilation:** see Q165. The NDK and Xcode are needed anyway
  for the mobile targets.
- **Spelling every C type by its C name in headers (`int`, `long`):** `c_char` is the only
  one whose fixed-width spelling is a different C type. The others are typedefs of the same
  types on every supported target.

## Consequences

- `CCompiler` has a `cl` flag. The compile, link and archive commands branch on it in
  `Toolchain`, `build_library`, `export_list` and `find_archiver`.
- Float divisions with two constant operands go through `sag_div_f32`/`sag_div_f64` in the
  runtime header.
- Integration tests that compile C programs go through `compile_c_program`, which knows
  `cl`'s options.
- F3 is complete. F4 (concurrency) will need `<stdatomic.h>`, which MSVC provides only behind
  `/experimental:c11atomics`. `cl` may stay without concurrency until that changes.
