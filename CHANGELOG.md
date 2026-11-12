# Changelog

All notable changes to Sagitario: the language, the compiler `sag`, the package manager Nova,
the language server and the VS Code extension, which share one version. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and versions follow
[Semantic Versioning](https://semver.org/) (before 1.0, a minor version may break programs).

Released versions are the tags `vX.Y.Z`, published as GitHub Releases. Versions written in the
sources but never tagged (0.11.0, 0.18.0, 0.19.0, 0.21.0, 0.23.0) are part of the release that
followed them.

## [Unreleased]

The sources say 0.32.0.

### Added

- `Default`, as Rust's: implemented by hand or with the attribute `@default` before a struct (the
  default of each field) or an enum (the variant marked `@default`), and by numbers, `bool`,
  `char`, strings, lists, maps, `Option`, tuples and arrays; `Type::default()` and
  `T::default()`, `unwrap_or_default` on `Option` and `Result`, `mem::take`, `mem::replace` and
  `mem::swap`, and struct update, `Config { verbose: true, ..Config::default() }`
  ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 3).
- `From`, `Into`, `TryFrom` and `TryInto`, as Rust's: `Type::from(x)` chooses the implementation
  by the type of `x`, every type is `From` itself, `x.into()` converts into the type expected or
  that of a bound `T: Into<U>`, and the prelude has Rust's conversions between numbers
  (`i64::from(5i32)`, `u8::try_from(300i32)`), characters, strings, lists and maps; `?` converts
  an error through `From` ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 2).
- Static functions in interfaces, such as `fn zero() -> Self;`, with or without a default,
  implemented by a type's body or an `impl ... for` block and called as `Type::zero()`,
  `Self::zero()` or `T::zero()` for a type parameter bounded by the interface
  ([ADR-0094](docs/adr/0094-conversions-and-defaults.md), increment 1).
- Rust's adapters and consumers of iterators (`enumerate`, `zip`, `chain`, `flat_map`, `flatten`,
  `take_while`, `sum`, `min`, `max`, `position`, `partition`, `unzip`...), `peek` on every
  iterator, `DoubleEndedIter<T>` with `rev` and `next_back` for those of lists, slices, ranges
  and strings, and `collect` into a `Map`, a `String` or a `Result` when the context expects one
  ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 5).
- `Map::update`, `upsert` and `get_or_insert`, which do what Rust's `entry` is used for, with
  `retain`, `extend`, `iter` (pairs) and `Map::from` a list of pairs
  ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 4).
- Rust's methods of lists (`extend`, `append`, `retain`, `dedup`, `drain`, `split_off`,
  `binary_search`, `sort_by_key`, `windows`, `chunks`, `min`, `max`, `List::with_capacity`...), and
  those of slices, which arrays and lists have too (`contains`, `sort`, `reverse`, `fill`, `swap`,
  `copy_from_slice`...); lists and arrays are compared, ordered and hashed by their elements, and
  maps compared by their entries ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 3).
- Rust's methods of `String` (`rfind`, `split_once`, `strip_prefix`, `split_whitespace`,
  `char_indices`, `to_uppercase` with Unicode's full mappings...), `String::from_utf8` and
  `from_utf16`, the Unicode properties of `char` (`is_alphabetic`, `is_numeric`, `is_uppercase`...)
  and `char::from_digit`, and `StringBuilder::insert`, `truncate` and `pop`
  ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 2).
- `assert`, `assert_eq`, `assert_ne`, `unreachable` and `todo`, which panic at the line of their
  call, and Rust's other methods of `Option` and `Result` (`ok_or`, `or`, `and`, `xor`, `map_or`,
  `zip`, `flatten`, `ok`, `err`, `expect_err`...); `Option` and `Result` are compared, ordered and
  hashed when the values they hold are ([ADR-0093](docs/adr/0093-everyday-methods.md), increment 1).
- Tuples: `(i32, String)` types, `(a, b)` values and `t.0` reads, compared, ordered and hashed
  when their elements are, so they sort and key maps, with a debug form `(1, "a")`
  ([ADR-0092](docs/adr/0092-tuples.md), increment 1), and tuple patterns, `(a, b)` and
  `(first, .., last)`, in `match`, `if let`, `while let`, `const`, `var` and `for`
  (`for (index, value) in pairs`), with matches on tuples checked for every case (increment 2).
- Global state: `static` and `static var`, one `Send` value for the program computed on its first
  use ([ADR-0083](docs/adr/0083-global-state.md)), and `@thread_local static var`, one value per
  thread.
- Conditional compilation with `@cfg(...)` on items, `use` and the functions and statics of
  `extern` blocks: `windows`, `unix`, `target_os`, `target_arch`, `target_pointer_width`, `all`,
  `any` and `not` ([ADR-0085](docs/adr/0085-conditional-compilation-and-std-time.md)).
- C's global variables in `extern "C"` blocks, `static NAME: T;` and `static var NAME: T;`, used
  inside `unsafe` ([ADR-0086](docs/adr/0086-c-globals-and-std-io.md)).
- Bitwise operators and shifts, as Rust's: `&`, `|`, `^`, `!` on integers, `<<` and `>>`, with
  `&=`, `|=`, `^=`, `<<=` and `>>=` and Rust's precedence; a shift by the width or more panics in
  debug builds ([ADR-0088](docs/adr/0088-bitwise-operators-and-integer-methods.md)).
- The integer methods spec 2 promised: `wrapping_*`, `checked_*` (giving `Option`) and
  `saturating_*`, with `count_ones`, `leading_zeros`, `trailing_zeros`, `rotate_left`,
  `rotate_right`, `swap_bytes` and `is_power_of_two`; and `i32::MIN`, `i32::MAX` and those of
  every integer type.
- `Display`, a prelude interface with `to_string()`: format strings interpolate the structs,
  enums and classes implementing it, and numbers, `bool`, `char` and `String` have
  `to_string()` ([ADR-0089](docs/adr/0089-display-debug-and-format-specifiers.md)).
- Format specifiers, Rust's mini-language after a `:` in the braces of a format string:
  width, fill and alignment, `+`, zero padding, precision, and `x`, `X`, `o`, `b`, `e`, `E`
  (`f"{price:>8.2}"`, `f"{mask:#06x}"`).
- The debug form of any value, `{x:?}` and the multi-line `{x:#?}`, which the compiler gives every
  type: `Point { x: 1, y: 2 }`, `Some(1)`, `[1, 2]`, `{"a": 1}`, strings with Rust's escapes, and
  cycles of classes cut as `Name { .. }`.
- `std::hint::black_box`, and a suite of nine benchmarks against Rust, C, C#, JavaScript and
  Python, run with `sag run tools/bench.sag` ([ADR-0090](docs/adr/0090-benchmarks.md)); with
  `--race`, the results as a race of the languages, an HTML page, and with `--video` an MP4 when
  `ffmpeg` is installed. The runner records the peak memory of each program, and
  [`docs/benchmarks.md`](docs/benchmarks.md) the first results and the work they point at.
- An audit of the standard library against Rust's ([`docs/std-audit.md`](docs/std-audit.md)), the
  plan of S2, with a suite of benchmarks against Rust, C, C#, JavaScript and Python.
- The language server completes `@cfg` conditions, attributes and their representations, and
  shows the types of constants; the VS Code extension gains snippets for `@thread_local`, `@cfg`
  and C globals.
- `tools/vscode.sag` packages the VS Code extension and installs it in the local VS Code.

### Changed

- `rev()` works on any iterator with two ends, and is an error on the others, as in Rust;
  `list.iter()` and the other iterators with two ends have the type `DoubleEndedIter<T>`, which
  is an `Iter<T>` ([ADR-0093](docs/adr/0093-everyday-methods.md), Q627).
- The standard library is written in Sagitario on the C API of each system, gathered in the
  private module `std::sys`, instead of C in the runtime (S1 of the
  [roadmap](docs/roadmap-v2.md)): `std::time`, `std::env`, `std::path`, `std::io`, `std::fs`,
  `std::process::Command`, the dynamic libraries of `std::ffi::Library`, `std::sync` and
  `std::thread`. About 2,000 lines of C leave the runtime. On Windows, `Command` reads the pipes
  of the child with overlapped I/O instead of threads. The atomics of `std::sync` are `_Atomic`
  fields of an intrinsic type of the standard library, `std::sys::Atomic`, instead of runtime
  objects ([ADR-0087](docs/adr/0087-atomic-storage-and-std-sync.md)).
- Generated programs count references through inline functions, reach list elements with the
  size of their type as a constant, push inline while a list has room, and compare primitives in
  generic code with C's own operators.
- A pass over SIR removes the retain and the release of a temporary moved into a binding, a
  field or an element, and pairs of counts with nothing between them
  ([ADR-0091](docs/adr/0091-fewer-reference-counts.md)).
- `List::sort` and `sort_by` are a merge sort through one buffer with runs sorted by insertion,
  about 5 times faster; `Map` compares keys only when their hashes match, and a `Channel` wakes
  its receivers only when one waits.
- The standard library's C declarations go into a C file of their own (`sag_std_ffi.c`), so they
  never clash with a program's, and it declares only the C functions the program uses.
- The roadmap is replanned as a complete language with a standard library like Rust's; the game
  engine moves to a project of its own ([ADR-0084](docs/adr/0084-a-complete-language-and-the-engine-apart.md)).

### Fixed

- An interface that cannot be a type (one that mentions `Self`, has methods with type parameters
  or, now, static functions) written as a type inside a function body is an error (`E0329`), as
  it already was in declarations; it was accepted there.
- On Windows, `Command` runs a program given by a relative path written with `/`
  (`build/tool.exe`), which `CreateProcessW` did not find.
- A statement starting with a block-like expression (`if`, `match`, `unsafe { }`, a block) ends at
  its `}`: `unsafe { f() } * p` no longer parses as a multiplication. Another operator there is
  an error (`E0100`) that asks for parentheses.

## [0.20.0] - 2026-11-10

### Added

- A complete FFI: C layouts with `@repr(C)` structs, unions and enums, C function pointers,
  `&raw const` and `&raw mut`, buffers in lists, `std::mem` and dynamic libraries with
  `std::ffi::Library` ([ADR-0078](docs/adr/0078-ffi-without-c-shims.md)).
- References in parameters (`&T`, `&mut T`) with a borrow checker, slices and borrowed lists
  ([ADR-0079](docs/adr/0079-references-and-borrowing.md)); `fs::write_bytes` for slices of bytes.
- Operator overloading through interfaces: `impl ... for` blocks, several implementations of an
  interface, arithmetic through `Add`, `Sub`, `Mul`, `Div`, `Rem` and `Neg`, and comparison of
  structs, enums and type parameters through `Eq` and `Ord`
  ([ADR-0081](docs/adr/0081-operator-overloading.md)).
- Format strings `f"..."` with `{expr}` interpolation
  ([ADR-0082](docs/adr/0082-format-strings.md)).
- `std::math`: constants, methods of numbers, non-finite float constants, powers, roots,
  logarithms and trigonometry ([ADR-0080](docs/adr/0080-std-math-trigonometry.md)).
- `tools/version.sag` changes the version in every file that holds it.
- The language server completes the methods of numbers, slices, pointers and references, and the
  editor highlights the expressions inside format strings and templates.

### Fixed

- The threads of `spawn_blocking` jobs that end after their executor are detached.

## [0.10.0] - 2026-10-28

### Added

- `async fn`, `await` and tasks with `std::task` (F8), also as methods, generic functions and
  values; blocking work on threads with `task::spawn_blocking`
  ([ADR-0073](docs/adr/0073-spawn-blocking.md)).
- The F8 acceptance program, an asynchronous pipeline of files.

## [0.9.0] - 2026-10-28

### Added

- `nova update self` updates Sagitario to the latest release.
- The VS Code extension is published with the releases
  ([ADR-0071](docs/adr/0071-vscode-extension-with-the-releases.md)).
- One-command installers for each platform ([ADR-0069](docs/adr/0069-installers.md)).

### Fixed

- Nova hashes downloaded archives with .NET on Windows instead of `Get-FileHash`.
- `c_char` is `u8` when the compiler runs on Linux for ARM.

## [0.8.0] - 2026-10-27

The first release: the language through F6, with a compiler written in Sagitario.

### Added

- The core language (F1): variables, operators, control flow, ARC strings, structs, methods,
  fixed arrays, enums, `match`, `Option`, `Result` and `?`, the `String` API, `StringBuilder`,
  `panic`, modules with `use` and visibility, and an FFI with `extern "C"`, `unsafe`, raw
  pointers, `@export` and `@repr(C)`; diagnostics with codes, suggestions and colors.
- Classes (F2) with ARC, inheritance with `override`, `abstract` and dynamic dispatch, `weak`
  fields, interfaces with fields and default methods, monomorphized generics with bounds,
  closures and function values, and builds with sanitizers.
- The standard library (F3): `std::env`, `std::io`, `std::fs`, `std::path`, `std::time` and
  `std::process` with `Command`.
- Nova (F3), the package manager: `Nova.toml`, `new`, `init`, `build`, `run`, `check`, `clean`,
  dependencies by path, libraries (`.a`, `.so`, `.dylib`, `.dll`) with a C header, `[link]` for
  C libraries and sources, and cross-compilation to Android and iOS with `--target`.
- Concurrency (F4): `std::thread` with an inferred `Send`, `Mutex`, atomics and channels in
  `std::sync`.
- Module-level constants, `include_str`, `if let` and `while let` (F5).
- The compiler in Sagitario (F5, F6): lexer, parser, resolver, type checker, SIR and C code
  generation, with dumps of each pass, built from a seed of the C it generates for itself and
  checked to reach a fixed point.
- The language server `sag lsp` with semantic tokens, hover and definitions, and the VS Code
  extension with highlighting, snippets and a file icon (F6).
- Nova and the test harness written in Sagitario.
- Release archives built from the seed and published when a tag is pushed
  ([ADR-0068](docs/adr/0068-releases-and-distribution.md)).
- MSVC `cl` support.

### Removed

- The first compiler, written in Rust: Sagitario compiles itself. `sagc` is renamed `sag`, and the
  standard library moves to `compiler/std`.

[Unreleased]: https://github.com/Juanestban/sagitario/compare/v0.20.0...HEAD
[0.20.0]: https://github.com/Juanestban/sagitario/compare/v0.10.0...v0.20.0
[0.10.0]: https://github.com/Juanestban/sagitario/compare/v0.9.0...v0.10.0
[0.9.0]: https://github.com/Juanestban/sagitario/compare/v0.8.0...v0.9.0
[0.8.0]: https://github.com/Juanestban/sagitario/releases/tag/v0.8.0
