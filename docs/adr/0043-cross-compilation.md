# 0043. Cross-compilation to Android and iOS

- Status: Accepted
- Date: 2026-10-24
- Refines: [ADR-0026](0026-ffi-details.md) (`c_long` and `c_char` follow the target);
  builds on [ADR-0042](0042-library-outputs-and-linking-c.md)

## Context

F3 increment 7 compiles for mobile platforms. Until now the compiler built for the machine
running it, and the type checker took C's type sizes from that machine (`cfg!`). Android
provides clang in its NDK; iOS needs Apple's SDK, only available on macOS. Increment 7 settles
Q150–Q161.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q150 | Asking for it | `--target <name>` on `nova build`/`check` and `sag build`/`check`; `sag targets` lists the targets. |
| Q151 | Targets | `aarch64-linux-android`, `armv7-linux-androideabi`, `x86_64-linux-android`, `aarch64-apple-ios`, `aarch64-apple-ios-sim` and `x86_64-apple-ios`, Rust's names, mapped to clang triples. Other targets wait for sysroot support. |
| Q152 | Android | The NDK from `ANDROID_NDK_HOME`, `ANDROID_NDK_ROOT`, or the newest in the `ndk` directory of `ANDROID_HOME` or `ANDROID_SDK_ROOT`; its clang with `--target=<triple><api>` and its `llvm-ar`. API level 24. |
| Q153 | iOS | macOS only: `xcrun` gives clang and the SDK (`-isysroot`); iOS 13.0. |
| Q154 | Outputs | Android builds programs and libraries. iOS builds only libraries, by default only the static one, which Xcode links. |
| Q155 | Target-dependent types | `TargetInfo` (pointer width, `long` size, `char` sign) travels in the package to the type checker: `c_long` has 32 bits on Windows and 32-bit targets, `c_char` is unsigned on Linux and Android for ARM, and `isize`/`usize` literals are checked against the target's width. |
| Q156 | Output directory | `target/<target>/<profile>/`. |
| Q157 | Running | Only for this machine; running on devices (`adb`) is deferred. |
| Q158 | Options | `--sanitize` is rejected for targets; `--release`, `--emit-c` and `[link]` work, with C sources compiled by the target's compiler. |
| Q159 | Runtime | It compiles with the NDK for 32 and 64 bits. On Apple systems it reads the environment through `_NSGetEnviron()`, since shared libraries there cannot refer to `environ`. |
| Q160 | Tests and CI | Nova integration tests build for the three Android targets (checking the ELF class and machine) and for iOS on macOS. They are skipped without an NDK, except in CI's Ubuntu job, where `SAG_REQUIRE_NDK=1` makes that a failure. |
| Q161 | Documentation | This ADR, `docs/nova.md` and the FFI chapter of the specification. |

## Alternatives considered

- **Any clang triple:** without a sysroot most would fail at the first `#include`, with
  confusing errors.
- **Running Android programs through `adb`:** useful, but it needs a device or an emulator
  and its own error handling; it can come with a later increment.
- **Detecting the target's types from the C compiler:** possible with `-dM -E`, but slower and
  only right for the targets listed anyway.

## Consequences

- `CCompiler` carries arguments that every call to it needs. The C toolchain code builds its
  commands through `CCompiler::command`.
- Executables for a target have no extension, even when built on Windows.
- `IntTy::max` takes the target's pointer width; `builtin_sig`, `c_type` and `c_string_ptr`
  take the `TargetInfo`, so that `*const c_char` means the same type everywhere in a program.
- The `environ` fix also concerns macOS libraries built for the host since increment 6.
- iOS was not built locally (no macOS available); CI's macOS job is its first check.
