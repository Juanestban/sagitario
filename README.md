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
> language server written in Sagitario. Distribution with installers comes next (see the
> [roadmap](docs/roadmap.md)).

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
definition and completion.

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
