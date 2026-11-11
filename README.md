<div align="center">

<img src="docs/assets/logo.svg" alt="Sagitario logo: an archer's bow shooting an arrow under the Sagittarius constellation" width="128">

# Sagitario

**A compiled systems language with Rust's safety, the classes of TypeScript, Java and C#, and
the functional style of JavaScript. Compiled to portable C11, and written in itself.**

[![CI](https://github.com/Juanestban/sagitario/actions/workflows/ci.yml/badge.svg)](https://github.com/Juanestban/sagitario/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/Juanestban/sagitario?label=release)](https://github.com/Juanestban/sagitario/releases/latest)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%20OR%20Apache--2.0-blue.svg)](#license)
![Platforms](https://img.shields.io/badge/platforms-Windows%20%7C%20Linux%20%7C%20macOS%20%7C%20Android%20%7C%20iOS-6f42c1.svg)
![Output](https://img.shields.io/badge/compiles%20to-C11-555.svg)

[Install](#install) · [Getting started](#getting-started) · [A tour](#a-tour-of-the-language) ·
[Roadmap](#roadmap-what-is-missing) · [Documentation](#documentation)

</div>

Sagitario has no null, and errors are `Result` values passed with `?`. It has exhaustive `match`,
classes with inheritance and interfaces, generics, closures and iterators, and memory managed by
automatic reference counting, without a garbage collector. A program compiles to C11 and then,
with clang, gcc or MSVC, to a native executable for desktop and mobile. The compiler, the package
manager Nova and the language server are written in Sagitario.

```sag
abstract class Animal {
  prv const name: String;

  pub fn new(self, name: String) { self.name = name; }
  pub fn display_name(self) -> String { self.name }
  pub abstract fn speak(self) -> String;
}

class Dog extends Animal {
  pub fn new(self, name: String) { zod.new(name); }
  pub override fn speak(self) -> String { f"{self.display_name()} says woof" }
}

fn main() {
  const dog = new Dog("Rex");
  println(dog.speak());   // Rex says woof
}
```

> [!NOTE]
> Sagitario is young: the language is complete enough to write its own compiler, but it has no
> registry of packages yet and its syntax may still change between minor versions (`0.x`). What
> is done and what is missing is in [Roadmap: what is missing](#roadmap-what-is-missing).

## Principles

These rules hold for every change of the language and its tools
([CONTRIBUTING.md](CONTRIBUTING.md)):

- **The spec comes first.** The syntax and semantics are in [`docs/spec/`](docs/spec/README.md),
  and each decision and its rationale is in an [ADR](docs/adr/README.md). Nothing is implemented
  without a spec section.
- **No dependencies.** The compiler, Nova and the runtime need only a C compiler.
- **The compiler never panics on user input.** Every failure is a diagnostic with a code
  (`E0001`...), a span and a message, often with a `help` that shows the fix.
- **Strict C11 without undefined behavior.** Signed overflow goes through runtime helpers and
  panics in debug builds. Every test runs with AddressSanitizer and UndefinedBehaviorSanitizer.
- **Zero warnings.** The compiler, Nova and the test harness build without a single one.

## Install

On Linux and macOS:

```sh
curl -fsSL https://github.com/Juanestban/sagitario/releases/latest/download/install.sh | sh
```

On Windows (PowerShell):

```powershell
irm https://github.com/Juanestban/sagitario/releases/latest/download/install.ps1 | iex
```

The installers download the latest [release](https://github.com/Juanestban/sagitario/releases),
check its SHA-256 checksum, put `sag` and `nova` in `~/.sagitario/bin` and add it to `PATH`.

`sag` compiles through a C compiler. The installers look for one and, if there is none, say how
to install it:

| System  | C compiler                                                                       |
|---------|----------------------------------------------------------------------------------|
| Windows | [LLVM](https://releases.llvm.org/) (`clang`) and the Visual Studio Build Tools, for its linker |
| macOS   | the Xcode command line tools (`xcode-select --install`)                          |
| Linux   | `clang` or `gcc` (`sudo apt install clang`)                                      |

Keep it up to date with Nova:

```sh
nova update self            # to the latest release
nova update self --check    # only say whether there is one
nova update self --version 0.10.0   # a given release, also to go back
```

<details>
<summary>Other versions, uninstalling and downloaded archives</summary>

- Pass `--version X.Y.Z` to install another release (`sh -s -- --version X.Y.Z` after the pipe;
  on Windows, set `SAGITARIO_VERSION`), and `--help` for the other options.
- To uninstall, remove `~/.sagitario` and its line in `~/.profile` (and `~/.zshrc` on macOS), or
  on Windows its entry in the `PATH` of the user.
- The executables are not signed. An archive downloaded with a browser is marked by the system:
  clear it with `Unblock-File` on Windows or `xattr -d com.apple.quarantine` on macOS. To check an
  archive, compare its SHA-256 with `SHA256SUMS` of the same release.

</details>

You can also build it from source (see [Building from source](#building-from-source)).

## Getting started

### Your first program

Save this as `hello.sag`:

```sag
fn main() {
  println("Hello, Sagitario!");
}
```

Then compile it and run it:

```sh
sag run hello.sag            # compile and run
sag build hello.sag          # only compile: hello (hello.exe on Windows)
sag check hello.sag          # only check, without generating C
```

### A package with Nova

[Nova](docs/nova.md) creates, builds and runs packages, as Cargo does for Rust:

```sh
nova new hello && cd hello
nova run
```

```
hello/
├── Nova.toml        # name, version and dependencies
├── .gitignore
└── src/
    └── main.sag     # fn main() { println("Hello, world!"); }
```

| Command                     | What it does                                              |
|-----------------------------|-----------------------------------------------------------|
| `nova new <name> [--lib]`   | Create a program package (or a library)                   |
| `nova build [--release]`    | Build it in `target/debug/` (or `target/release/`)        |
| `nova run -- <args>`        | Build it and run it, forwarding the arguments             |
| `nova check`                | Check it without generating C                             |
| `nova clean`                | Remove `target/`                                          |
| `nova update self`          | Update Sagitario to the latest release                    |

Dependencies are other packages on disk, declared in `Nova.toml` as
`geometry = { path = "../geometry" }`.

### Your editor

The VS Code extension in [`editors/vscode/`](editors/vscode/README.md) highlights `.sag` files,
including the expressions inside format strings. It adds snippets and starts `sag lsp` for
diagnostics as you type, hover, go to definition and completion. Each release carries its
`sagitario.vsix`:

```sh
code --install-extension sagitario.vsix
```

It finds the `sag` of the installers without settings. To install the extension of this checkout
in your VS Code instead, run `sag run tools/vscode.sag` from the root of the repository.

### Useful flags

```sh
sag run --release main.sag          # optimized; integer overflow wraps
sag run --sanitize main.sag         # with AddressSanitizer and UndefinedBehaviorSanitizer
sag build main.sag --emit-c         # also write the generated C
sag check --dump=types main.sag     # the result of a pass: tokens, ast, resolve, types, c...
sag build main.sag --target aarch64-linux-android   # cross-compile (`sag targets` lists them)
```

## A tour of the language

```sag
struct Vec2 {
  pub x: f64;
  pub y: f64;
}

// Operators are interfaces: `a + b` calls `add`.
impl Add<Vec2, Vec2> for Vec2 {
  fn add(self, other: Vec2) -> Vec2 {
    Vec2 { x: self.x + other.x, y: self.y + other.y }
  }
}

enum Shape {
  Circle(f64),
  Rect(f64, f64),
}

// `match` must cover every variant.
fn area(shape: Shape) -> f64 {
  match shape {
    Shape::Circle(r) => 3.14159 * r * r,
    Shape::Rect(w, h) => w * h,
  }
}

// No exceptions and no null: errors are values.
fn parse_age(text: String) -> Result<i32, String> {
  match text.parse_i32() {
    Ok(age) => Ok(age),
    Err(_) => Err(f"`{text}` is not a number"),
  }
}

fn main() -> Result<(), String> {
  const position = Vec2 { x: 1.0, y: 2.0 } + Vec2 { x: 0.5, y: 0.5 };
  println(f"position ({position.x}, {position.y})");           // position (1.5, 2.5)

  const shapes = List::from([Shape::Circle(1.0), Shape::Rect(2.0, 3.0)]);
  const big = shapes.iter().map((s) => area(s)).filter((a) => a > 4.0).collect();
  println(f"{big.len()} big shape(s)");                        // 1 big shape(s)

  const age = parse_age("42")?;                                // `?` passes the error up
  println(f"next year: {age + 1}");                            // next year: 43

  Ok(())
}
```

| Area            | What there is                                                                                     | Spec |
|-----------------|---------------------------------------------------------------------------------------------------|------|
| Types           | integers with checked overflow, `f32`/`f64`, `String`, arrays, `List`, `Map`, slices, `Option`, `Result` | [02](docs/spec/02-types.md) |
| Memory          | value structs, classes with ARC, `weak` references, `const`/`var`, `&T` and `&mut T` with a borrow checker | [03](docs/spec/03-memory.md) |
| Classes         | constructors, inheritance, `abstract`, `override`, visibility (`pub`, `prv`)                       | [04](docs/spec/04-classes.md) |
| Interfaces      | generics with bounds, default methods, `impl ... for` blocks, operators, `Eq`/`Ord`/`Hash`         | [05](docs/spec/05-interfaces-generics.md) |
| Control flow    | exhaustive `match`, `if let`, `while let`, ranges, `f"..."` strings              | [06](docs/spec/06-control-flow.md) |
| Errors          | `Result` and `?`, the `Error` interface, panics with a message and a location                     | [07](docs/spec/07-errors.md) |
| Closures        | lambdas with captures, iterators (`map`, `filter`, `fold`...), functions as values                | [08](docs/spec/08-closures.md) |
| Modules         | files as modules, `pub`, packages with Nova                                                       | [09](docs/spec/09-modules.md) |
| FFI             | `extern "C"`, `@repr(C)`, unions, function pointers, dynamic libraries, without C files           | [10](docs/spec/10-ffi.md) |
| Standard library| `fs`, `path`, `io`, `env`, `process`, `time`, `math`, `mem`, `ffi`, `thread`, `sync`, `task`      | [11](docs/spec/11-standard-library.md) |
| Concurrency     | threads checked by `Send`, channels, `Mutex`, atomics, `async`/`await` with `Task<T>`              | [12](docs/spec/12-async.md) |

Larger programs that use them together:

- [`tests/run-pass/reference_zoo/`](tests/run-pass/reference_zoo/main.sag): classes, interfaces,
  generics, closures and collections.
- [`tests/run-pass/reference_async/`](tests/run-pass/reference_async/): an asynchronous pipeline
  of files.
- [`tests/run-pass/reference_ffi/`](tests/run-pass/reference_ffi/): a C library wrapped in
  Sagitario alone.
- [`tests/run-pass/g1_acceptance/`](tests/run-pass/g1_acceptance/main.sag): the matrices of a
  camera with operators, a binary mesh, and `qsort` loaded from the C library.
- [`compiler/`](compiler/): the compiler itself, about the largest Sagitario program there is.

## Roadmap: what is missing

The phases and their acceptance criteria are in [docs/roadmap.md](docs/roadmap.md) and
[docs/roadmap-v2.md](docs/roadmap-v2.md).

| Phase | What                                                                       | Status         |
|-------|----------------------------------------------------------------------------|----------------|
| F0–F2 | The MVP: the core language, classes, interfaces, generics, closures       | ✅ Done        |
| F3    | Platform: the standard library, Nova, C libraries, Android and iOS        | ✅ Done        |
| F4    | Concurrency: threads, channels, mutexes, atomics                          | ✅ Done        |
| F5    | Self-hosting: the compiler written in Sagitario                           | ✅ Done        |
| F6    | Language server, VS Code, and a bootstrap from a seed without Rust        | ✅ Done        |
| F7    | Distribution: releases, installers, `nova update self`                    | 🟡 Acceptance  |
| F8    | `async`/`await`                                                           | ✅ Done        |
| G1    | Language prerequisites of a game engine: math, FFI, references, operators | 🟡 In progress |
| G2–G9 | A game engine in Sagitario: windows, Vulkan, audio, physics, networking, an editor | ⬜ Planned |

What a complete language still needs, from the deferred decisions of the roadmaps. Each one
needs an ADR before it is implemented.

**The language**

- [ ] Mutable globals and static fields, or a sanctioned alternative
- [ ] Arenas or pools for many short-lived values, without one reference count each
- [ ] Derived `Eq`, `Ord` and `Hash` (today they are written by hand)
- [ ] A `Display` interface for interpolating user types in `f"..."`
- [ ] Automatic error conversion with `From<T>` in `?`, and `Error::source()`
- [ ] General tuples, open ranges (`a..`), raw strings
- [ ] `protected` visibility and `weak` captures in closures
- [ ] `async` lambdas, cancellation and a multi-threaded executor
- [ ] Inference of `None`, `Ok` and `Err` from later uses

**Modules and packages**

- [ ] Re-exports (`pub use`), glob imports (`use a::*`), `use` inside blocks
- [ ] Dependencies from git and a registry of packages, with `Nova.lock`

**The standard library**

- [ ] `std::net` (TCP, UDP), then HTTP
- [ ] Asynchronous I/O for `async` tasks
- [ ] Bounded channels, `try_receive` and timeouts

**Tools**

- [ ] `@test` and a test runner (`nova test`), and `@deprecated`
- [ ] A formatter (`sag fmt`) and a documentation generator (`sag doc`)
- [ ] Binding generation from C headers and `vk.xml`
- [ ] Debugging with the names and types of Sagitario (today `#line` maps the C back to the lines)
- [ ] Backtraces in panics, and diagnostics in other languages
- [ ] Signed executables, native packages (`.msi`, `.pkg`, `.deb`) and the VS Code Marketplace

## Building from source

Sagitario builds from a seed of the C that its compiler generates for itself
([ADR-0067](docs/adr/0067-bootstrap-from-the-seed.md)), so a C compiler is all it needs.
`--install` also copies `sag` and `nova` to `~/.sagitario/bin` and adds it to `PATH`.

### Windows

1. Install [LLVM](https://releases.llvm.org/) (`x86_64-pc-windows-msvc`) and the **Visual Studio
   Build Tools** with *Desktop development with C++*, for its linker and C library. If the `clang`
   on your `PATH` is an MSYS2/MinGW one, set `SAG_CC` to the LLVM one.
2. Clone and build:

   ```powershell
   git clone https://github.com/Juanestban/sagitario.git
   cd sagitario
   bootstrap/build.ps1 --install
   ```

3. `build\bin\sag.exe` and `build\bin\nova.exe` are ready.

MinGW and MSVC `cl` also work, best-effort and without sanitizers.

### macOS

```sh
xcode-select --install
git clone https://github.com/Juanestban/sagitario.git
cd sagitario
sh bootstrap/build.sh --install
```

### Linux

```sh
sudo apt install clang          # or gcc; on Fedora: sudo dnf install clang
git clone https://github.com/Juanestban/sagitario.git
cd sagitario
sh bootstrap/build.sh --install
```

A 64-bit system is required: the seed does not build on 32-bit hosts. For Android, set
`ANDROID_NDK_HOME`.

## Project structure

```
compiler/src/          the compiler: lexer, parser, resolve, types, sir (the IR), codegen
compiler/src/lsp/      the language server (`sag lsp`)
compiler/src/nova/     Nova, the package manager
compiler/std/          the standard library, in Sagitario
runtime/               the C runtime embedded in every program: ARC, strings, panics, threads
bootstrap/             the seed: the generated C of the compiler, and the build scripts
editors/vscode/        the VS Code extension
install/               install.sh and install.ps1
tests/                 the harness and its suites: run-pass, compile-fail, dumps, lsp...
docs/spec/             the language specification (normative)
docs/adr/              architecture decision records
```

## Development

```sh
sh bootstrap/build.sh                                         # build/bin/sag and nova from the seed
sag run tests/harness/main.sag -- --sag build/bin/sag --nova build/bin/nova   # every test
sag run tests/harness/main.sag -- --sag build/bin/sag suites lsp             # some groups
SAG_BLESS=1 sag run tests/harness/main.sag -- --sag build/bin/sag dumps      # rewrite expected files
SAG_TEST_SANITIZE=0 sag run tests/harness/main.sag -- --sag build/bin/sag    # faster, without sanitizers
```

The harness is described in [docs/testing.md](docs/testing.md) and the structure of the compiler
in [docs/architecture.md](docs/architecture.md). Every language feature needs a spec section, a
`run-pass` test and a `compile-fail` test for each rule it adds. To publish a version, see
[Releasing](CONTRIBUTING.md#releasing).

## Documentation

- [Language specification](docs/spec/README.md)
- [Nova, the package manager](docs/nova.md)
- [Design decisions (ADRs)](docs/adr/README.md)
- [Compiler architecture](docs/architecture.md)
- [Test harness](docs/testing.md)
- [Roadmap](docs/roadmap.md) and [roadmap v2: a game engine](docs/roadmap-v2.md)
- [Contributing](CONTRIBUTING.md)

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Unless you state otherwise, any contribution you submit is dual-licensed the same way.
