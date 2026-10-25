# Nova, the package manager

Rationale: [ADR-0040](adr/0040-nova-package-manager.md).

Nova creates, builds, runs and checks Sagitario packages. It compiles them with the same
compiler as `sag`, which stays the tool for single files.

```sh
nova new hello        # creates hello/ with Nova.toml and src/main.sag
cd hello
nova run              # Compiling hello v0.1.0 / Hello, world!
```

## A package

```
hello/
  Nova.toml           the manifest
  .gitignore          `/target`
  src/
    main.sag          the root module `pkg` of a program
    util/strings.sag  the module `pkg::util::strings`
  target/
    debug/hello       built by `nova build` (`hello.exe` on Windows)
    release/hello     built by `nova build --release`
```

A package is a **program** if it has `src/main.sag`, or a **library** if it has `src/lib.sag`;
it must have exactly one of them. Modules are the files under `src/`, as described in
[modules](spec/09-modules.md).

Programs use libraries through [dependencies](#dependencies), and C uses them through the
[library outputs](#library-outputs) that `nova build` writes.

## Commands

| Command | Effect |
|---|---|
| `nova new <name>` | Creates the directory `<name>` with a new program (`--lib`: a library) |
| `nova init` | Creates a package in the current directory, named after it (`--name <name>` to choose; `--lib`). It never replaces a file: an existing `src/main.sag` or `src/lib.sag` decides the kind of package |
| `nova build` | Compiles the package into `target/debug/<name>` |
| `nova run [-- args]` | Compiles and runs the program with `args`; Nova exits with the program's exit code |
| `nova check` | Checks the package without generating code |
| `nova clean` | Removes `target/` |
| `nova help`, `nova version` | Usage and version |

`build`, `run`, `check` and `clean` work in the package's directory or any directory below it:
Nova uses the nearest `Nova.toml` above.

| Option | Commands | Effect |
|---|---|---|
| `--release` | build, run | Optimizes, and integer overflow wraps; output in `target/release/` |
| `--sanitize` | build, run | AddressSanitizer and UndefinedBehaviorSanitizer, where the C compiler has them |
| `--emit-c` | build, run | Also writes the generated C next to the executable or the library |
| `--cc <compiler>` | build, run | The C compiler; overrides `SAG_CC` |
| `--target <target>` | build, check | Compiles for another machine ([targets](#targets)) |
| `--diagnostic-format <fmt>` | build, run, check | `human` (default) or `short` |

Exit codes: 0 on success, 1 when the package has errors or a command fails, 2 for invalid
usage, and the program's own code for `nova run`.

## `Nova.toml`

```toml
[package]
name = "hello"
version = "0.1.0"
description = "Greets the world"   # optional
authors = ["Ada <ada@example.com>"] # optional
license = "MIT OR Apache-2.0"        # optional
```

| Key | Value |
|---|---|
| `name` | Required. A lowercase identifier in snake_case (`my_app`) that is neither a keyword nor `pkg`, `self`, `super`, `std` or `prelude`, since other packages will import it by this name |
| `version` | Required. A [Semantic Versioning](https://semver.org) version: `MAJOR.MINOR.PATCH`, optionally followed by `-pre.release` and `+build` |
| `description`, `license` | Optional strings |
| `authors` | An optional array of strings |

`[dependencies]` lists the package's [dependencies](#dependencies), `[library]` chooses a
library's [outputs](#library-outputs), and `[link]` adds [C libraries and sources](#linking-c).
Any other key or table is an error.

### Syntax

`Nova.toml` uses a subset of [TOML](https://toml.io): table headers (`[package]`), `key = value`
lines with bare keys, comments (`#`) and these values:

| Value | Example |
|---|---|
| Strings, with the escapes `\"` `\\` `\n` `\t` `\r` `\b` `\f` `\uXXXX` `\UXXXXXXXX` | `"hello"` |
| Literal strings, without escapes | `'C:\path'` |
| Decimal integers, with `_` between digits | `1_000`, `-5` |
| Booleans | `true`, `false` |
| Arrays, over several lines if needed, with an optional trailing comma | `["a", "b"]` |
| Inline tables, on one line and without a trailing comma | `{ path = "../util" }` |

Other TOML features (dotted and quoted keys, arrays of tables, multi-line strings, floats and
dates) are reported as unsupported.

## Dependencies

Rationale: [ADR-0041](adr/0041-dependencies-by-path.md).

```toml
[dependencies]
geometry = { path = "../geometry" }
```

```sag
use geometry::shapes::Circle;

fn main() {
  println(`${geometry::area(Circle { radius: 2.0 })}`);
}
```

- A dependency is a library package found by its `path`, relative to the directory of the
  `Nova.toml` that names it (or absolute). Its key must be the dependency's own `name`.
- Its name is a path root in the dependent package, like `std`: `use geometry::shapes::Circle;`
  or `geometry::area(c)`. A module-level item or import with the same name hides it.
- Only its `pub` items can be used, through the path of the module declaring them: the
  dependency's modules are all reachable, and `pub` marks its API.
- Each package sees only its own dependencies by name. A package reached through several
  dependencies (the same directory) is compiled once.
- Packages cannot depend on each other in a cycle.
- The whole graph is compiled together into one program, so a generic function of a dependency
  is instantiated with the types its users pass.
- Errors in a dependency's code are reported with the paths of its files.
- Not yet: dependencies from git or a registry (and with them `Nova.lock`), renaming a
  dependency, development dependencies, features and optional dependencies.

## Library outputs

Rationale: [ADR-0042](adr/0042-library-outputs-and-linking-c.md).

`nova build` in a library builds files that C (or any language calling C) links against, with a
header declaring what they export:

| | Static library | Dynamic library |
|---|---|---|
| Linux | `libgeo.a` | `libgeo.so` |
| macOS | `libgeo.a` | `libgeo.dylib` |
| Windows (MSVC) | `geo.lib` | `geo.dll`, with the import library `geo.dll.lib` |
| Windows (MinGW) | `libgeo.a` | `geo.dll`, with the import library `libgeo.dll.a` |

All of them, and the header `geo.h`, go to `target/debug/` (or `target/release/`).
`[library] outputs = ["static"]` (or `["dynamic"]`) builds only one kind.

```sag
/// The squared distance between two points.
@export("geo_distance2")
pub extern "C" fn distance2(a: Point, b: Point) -> f64 { ... }
```

```c
#include "geo.h"
double d = geo_distance2(a, b);   /* cc main.c libgeo.a -lm */
```

- A library exports the `extern "C" fn` functions of its package and of its dependencies
  ([FFI](spec/10-ffi.md#exporting-functions)); a dynamic library exports nothing else, not even
  the runtime's functions. A library without exported functions builds with a warning.
- The header declares the exported functions with fixed-width C types (`int32_t` for `i32` and
  `c_int`, `double` for `f64`; `c_char` is `char`), defines the `@repr(C)` structs they use, and copies the `///`
  comments. It works from C and C++.
- The library contains the runtime. Programs linking the static library on Linux also need
  `-lm`.
- A panic in an exported function prints its message and ends the whole process with exit code
  101: it never unwinds into C.
- Two static libraries built by Sagitario cannot be linked into one program, since both contain
  the runtime; dynamic libraries can.
- Building a static library needs an archiver: `SAG_AR`, else `llvm-ar` next to the C
  compiler, else `llvm-ar` or `ar` on `PATH`.
- Libraries are never built with `--sanitize`, since the programs using them would need the
  sanitizer runtime.

## Linking C

```toml
[link]
libraries = ["sqlite3"]          # -lsqlite3
search_paths = ["vendor/lib"]     # -Lvendor/lib
sources = ["c/helpers.c"]         # compiled as C11 and linked
```

- `[link]` applies to programs and libraries, and the `[link]` of every dependency applies to
  the program using it. Paths are relative to the directory of the `Nova.toml` naming them.
- The C functions are then declared in `extern "C"` blocks ([FFI](spec/10-ffi.md)).
- A source that does not compile, or a library the linker cannot find, is an error of the
  package, shown with the C compiler's or the linker's message.

## Targets

Rationale: [ADR-0043](adr/0043-cross-compilation.md).

`--target` compiles for another machine; `sag targets` lists the targets:

| Target | Machine | Builds |
|---|---|---|
| `aarch64-linux-android` | Android on 64-bit ARM | programs and libraries |
| `armv7-linux-androideabi` | Android on 32-bit ARM | programs and libraries |
| `x86_64-linux-android` | Android on x86-64 (the emulator) | programs and libraries |
| `aarch64-apple-ios` | iOS devices | libraries |
| `aarch64-apple-ios-sim` | the iOS simulator on Apple silicon | libraries |
| `x86_64-apple-ios` | the iOS simulator on Intel Macs | libraries |

```sh
nova build --target aarch64-linux-android   # target/aarch64-linux-android/debug/
nova check --target armv7-linux-androideabi
```

- Builds for a target go to `target/<target>/debug/` (or `release/`). `sag build --target`
  works too, and `nova check --target` checks the program as the target sees it.
- **Android** uses the NDK: `ANDROID_NDK_HOME`, else `ANDROID_NDK_ROOT`, else the newest NDK in
  the `ndk` directory of `ANDROID_HOME` or `ANDROID_SDK_ROOT`. The minimum API level is 24
  (Android 7.0). A library's `.so` is what an app loads with JNI; programs run with `adb shell`.
- **iOS** needs macOS with Xcode, found through `xcrun`; the minimum version is iOS 13.0. iOS runs
  no standalone programs, so only libraries build, and only the static one unless
  `[library] outputs` asks for the dynamic one too.
- The sizes of `isize`, `usize`, `c_long` and the sign of `c_char` follow the target
  ([FFI](spec/10-ffi.md#stdffi)): a literal that fits on one target may not fit on another.
- Programs built for a target cannot run on this machine (`nova run --target` is an error), and
  `--sanitize` only works for this machine. Running on a device or an emulator is not
  automated yet.
- Other targets, such as Linux from Windows, are not supported yet.

## Errors

Errors in `Nova.toml` and in the package's layout have codes with the prefix `N`, and are shown
like the compiler's diagnostics, with their location:

```
error[N0006]: `Hello App` is not a valid package name: it must start with a lowercase letter and have only lowercase letters, digits and `_`
 --> Nova.toml:2:8
  |
2 | name = "Hello App"
  |        ^^^^^^^^^^^
  = note: the name of a package is the path root other packages import it with
  = help: use `hello_app`
```

| Code | Meaning |
|---|---|
| N0001 | Text that is not the TOML subset of `Nova.toml` |
| N0002 | A key or table defined twice |
| N0003 | A key or table that `Nova.toml` does not have |
| N0004 | A missing `[package]` table, `name` or `version` |
| N0005 | A value of the wrong type |
| N0006 | An invalid package name |
| N0007 | An invalid version |
| N0008 | A part of `Nova.toml` that is not available yet (dependencies by `version` or `git`) |
| N0009 | A package without exactly one of `src/main.sag` and `src/lib.sag` |
| N0010 | A dependency whose `path` holds no package |
| N0011 | A dependency whose package has another name |
| N0012 | A dependency that is a program, not a library |
| N0013 | Packages that depend on each other in a cycle |
| N0014 | A value that the key does not accept (a library output, a library name, a source not ending in `.c`) |
| N0015 | A table that does not apply to this kind of package (`[library]` in a program) |
| N0016 | A path in `[link]` that does not exist |
