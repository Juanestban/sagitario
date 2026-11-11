# 0085. Conditional compilation with `@cfg`, and `std::time` in Sagitario (S1)

- Status: Proposed
- Date: 2026-11-11
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q541: the pattern and
  the order of S1)

## Context

S1 rewrites the standard library half of the runtime in Sagitario, over the C library and the
system API ([ADR-0084](0084-a-complete-language-and-the-engine-apart.md), Q541). Its first module
is `std::time`, the smallest: `compiler/std/time.sag` already defines `Duration`, `Instant`,
`SystemTime` and `sleep` in Sagitario, but reads the clocks and sleeps through three functions of
the runtime written in C (`sag_time_monotonic`, `sag_time_system`, `sag_time_sleep`).

Writing those three in Sagitario shows what the language lacks:

- **Choosing code by system.** The monotonic clock is `QueryPerformanceCounter` on Windows and
  `clock_gettime` elsewhere. The C chooses with `#ifdef _WIN32`; Sagitario has no way to, and
  declaring both functions everywhere fails to link.
- **Values and layouts that change by system.** `CLOCK_MONOTONIC` is 1 on Linux and Android and
  6 on macOS and iOS. `time_t`, the seconds of `struct timespec`, has 32 bits on 32-bit Android.
  C's `errno` is a macro, reached through a function whose name changes with the C library
  (`__errno_location` in glibc, `__error` on Apple systems, `__errno` in Android's bionic).
- **The core uses them.** The runtime's core, which stays C until S3, uses `sag_time_monotonic`
  and `sag_time_sleep` for the timers of `async` tasks and to wait in a panic of a thread.

Rust chooses code with the attribute `#[cfg(...)]`: an item whose condition does not hold for the
target is removed before names are resolved. Its std keeps the declarations of each system in a
private module, `std/src/sys`, and its public API on top.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q547 | `@cfg` | The attribute `@cfg(condition)`, as Rust's `#[cfg(...)]`, keeps an item only when its condition holds for the target being compiled. Programs and packages may use it, as the standard library does. An item may have one `@cfg`; `all(...)` combines conditions. |
| Q548 | Conditions | `windows`; `unix` (Linux, macOS, Android and iOS); `target_os = "windows"`, `"linux"`, `"macos"`, `"android"` or `"ios"`; `target_arch = "x86_64"`, `"aarch64"` or `"arm"`; `target_pointer_width = "32"` or `"64"`; and `all(c, ...)`, `any(c, ...)` and `not(c)`. A name or a value outside these is an error (`E0105`) that lists the valid ones, since the set of attributes is closed (docs/spec/01-lexical.md). The values follow the target of `--target`, or the machine running the compiler without it. |
| Q549 | Where | On module-level items: functions, structs, enums, unions, classes, interfaces, constants, statics, `use` declarations, `impl` blocks and `extern "C"` blocks. It may also be on each function inside an `extern "C"` block. Fields, methods, statements and an expression form, as Rust's `cfg!`, are left for later. |
| Q550 | Meaning | An item whose condition does not hold is removed after parsing and before the resolver. Its code must parse, but its names are not resolved and its types are not checked, as in Rust. Two items of the same name whose conditions exclude each other are allowed; if both hold, the usual error of a repeated name applies. `sag check --target` checks the program as that target sees it. |
| Q551 | `std::sys` | The C API of each system lives in a private module, `std::sys`, as in Rust's `std/src/sys`. It declares its functions, constants, structs and the reading of `errno`, chosen with `@cfg`. Only the modules of `std` see it, as they see the helpers of `std` itself, so a program cannot name it. What `std` exposes about each system (Rust's `std::os::unix` and `std::os::windows`) is decided by the audit of S2. |
| Q552 | The core | A function of the runtime that moves to Sagitario keeps its C name through `@export("sag_time_monotonic")`, so that the core, still C until S3, calls it as before. The compiler always compiles the modules of `std` that the core needs, even when the program does not use them. The C of the function is deleted. |
| Q553 | Calling conventions | `extern "C"` is the system's convention on every target: the Windows targets are 64-bit, where the `WINAPI` convention is C's. |
| Q554 | Order of S1 | `std::time` first. Then, from what the core needs least to most: `std::env`, `std::path`, `std::io`, `std::fs`, `std::process` with `Command`, dynamic libraries (`std::ffi::Library`), `std::thread` and `std::sync`. Each module is one increment. It needs a record of its own only when it changes the language or the spec of the module; otherwise its notes go in this record. |
| Q555 | The editor | The grammar already colors attributes. `sag lsp` completes `cfg` after `@`, and the conditions and their values inside `@cfg(`. It analyzes for the machine it runs on, and leaves the items it removes uncolored by semantic tokens. There is a snippet `cfg` (CLAUDE.md). |
| Q556 | Increments | **1** `@cfg`: the parser (its arguments `name = "value"` and the nested `all`, `any` and `not`), the removal of items, the conditions of each target in `TargetInfo`, its errors, the editor and tests on several targets with `sag check --target`. **2** `std::sys` and `std::time` in Sagitario. The three functions of the runtime are deleted, the core calls the exported ones, and the clocks and `sleep` behave as before on every platform of CI. |

### Example

```sag
use std::ffi::{c_int, c_long};

/// `struct timespec`, whose `time_t` has 32 bits on 32-bit Android.
@cfg(all(unix, not(all(target_os = "android", target_pointer_width = "32"))))
@repr(C)
struct Timespec {
  pub tv_sec: i64;
  pub tv_nsec: c_long;
}

@cfg(all(target_os = "android", target_pointer_width = "32"))
@repr(C)
struct Timespec {
  pub tv_sec: i32;
  pub tv_nsec: c_long;
}

@cfg(any(target_os = "linux", target_os = "android"))
const CLOCK_MONOTONIC: c_int = 1;
@cfg(any(target_os = "macos", target_os = "ios"))
const CLOCK_MONOTONIC: c_int = 6;

@cfg(unix)
extern "C" {
  fn clock_gettime(clock: c_int, now: *mut Timespec) -> c_int;
}

@cfg(windows)
extern "C" {
  fn QueryPerformanceCounter(count: *mut i64) -> c_int;
  fn QueryPerformanceFrequency(frequency: *mut i64) -> c_int;
}
```

## Alternatives considered

- **Files per system, as Go:** no new syntax, but even one constant needs a file of its own, and
  conditions cannot be combined (`any`, `not`).
- **`@cfg` only in the standard library:** a package that wraps a C library could not adapt to
  each system, which ADR-0084 wants C libraries to be usable for.
- **An expression form, `if cfg(windows) { ... }`:** in Rust, both branches must compile on every
  system, so it cannot call a function that only one system has. Items are what `std` needs.
- **A public `libc`-like module in `std`:** useful, but it freezes a large API before the audit of
  S2.
- **Keeping C copies of what the core uses until S3:** simpler, but it duplicates code and the
  runtime would shrink less.

## Consequences

- Programs and packages can choose code by system, which wrappers of C libraries need.
- The standard library grows a private layer of system declarations that the later modules of S1
  extend.
- Every program compiles the modules of `std` that the core needs, which costs a little compile
  time and code until S3 moves the core.
- Behavior on Linux, macOS and Android is checked by CI, since the development machine may not run
  them.
