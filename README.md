<p align="center">
  <img src="docs/assets/logo.svg" alt="Sagitario logo: an archer's bow shooting an arrow under the Sagittarius constellation" width="160">
</p>

# Sagitario Language

Sagitario is a compiled systems programming language that combines Rust-like safety
(no null, `Result` errors, `match`) with TypeScript/Java/C#-style classes and JavaScript-style
functional programming. It uses automatic reference counting, compiles to portable C11, and
runs on desktop and mobile platforms, and compiles itself.

> **Status:** phases F0–F6 are done: modules, structs, enums and `match`, classes with
> inheritance, interfaces, generics, closures, lists, maps and iterators, compiled to C11 with ARC
> (the [reference zoo program](tests/run-pass/reference_zoo/main.sag) shows them together); the
> [standard library](docs/spec/11-standard-library.md), packages with Nova, C libraries and
> Android and iOS targets; threads, channels, mutexes and atomics; and a compiler, a package manager and a
> language server written in Sagitario. Distribution is in progress: releases, installers and
> `nova update self` are done (see the [roadmap](docs/roadmap.md)).

```sag
abstract class Animal {
  prv const name: String;

  pub fn new(self, name: String) { self.name = name; }
  pub fn display_name(self) -> String { self.name }
  pub abstract fn speak(self) -> String;
}

class Dog extends Animal {
  pub fn new(self, name: String) { zod.new(name); }
  pub override fn speak(self) -> String { `${self.display_name()} says woof` }
}

fn main() {
  const dog = new Dog("Rex");
  println(dog.speak());
}
```

Packages are created and built with Nova, the package manager:

```sh
nova new hello && cd hello
nova run
```

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
check its checksum, put `sag` and `nova` in `~/.sagitario/bin` and add it to `PATH`. `sag` compiles
through a C compiler, which the installers look for and, if there is none, say how to install:
clang or gcc, the Xcode command line tools on macOS, or LLVM and the Visual Studio Build Tools on
Windows. `nova update self` updates them to the next release. Pass `--version X.Y.Z` to install
another release (`sh -s -- --version X.Y.Z` after the
pipe; on Windows, set `SAGITARIO_VERSION`), and `--help` for the other options.

To uninstall, remove `~/.sagitario` and its line in `~/.profile` (and `~/.zshrc` on macOS), or on
Windows its entry in the `PATH` of the user. An archive downloaded with a browser instead is
marked by the system: clear it with `Unblock-File` on Windows or
`xattr -d com.apple.quarantine` on macOS.

## Building

Sagitario builds from a seed of the C its compiler generates for itself, with a C compiler alone
(clang, gcc, MinGW or MSVC `cl`):

```sh
sh bootstrap/build.sh --install      # Linux and macOS
bootstrap/build.ps1 --install        # Windows (PowerShell)
```

This builds `sag` and `nova` in `build/bin/` and, with `--install`, copies them to
`~/.sagitario/bin` and adds it to `PATH` (see [Contributing](CONTRIBUTING.md)).

## Editor support

The VS Code extension in [`editors/vscode/`](editors/vscode/README.md) highlights `.sag` files,
gives them an icon and adds snippets, and starts `sag lsp` for diagnostics, hover, go to
definition and completion. Each release has its `sagitario.vsix`
(`code --install-extension sagitario.vsix`); it finds the `sag` of the installers without settings.

## Documentation

- [Language specification](docs/spec/README.md)
- [Nova, the package manager](docs/nova.md)
- [Design decisions (ADRs)](docs/adr/README.md)
- [Compiler architecture](docs/architecture.md)
- [Roadmap](docs/roadmap.md)
- [Roadmap v2: a game engine](docs/roadmap-v2.md)
- [Contributing](CONTRIBUTING.md)

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
