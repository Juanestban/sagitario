# 9. Modules and packages (F1)

Rationale: [ADR-0010](../adr/0010-modules-and-packages.md),
[ADR-0024](../adr/0024-modules-imports-and-visibility.md),
[ADR-0025](../adr/0025-single-pub-visibility.md), [ADR-0040](../adr/0040-nova-package-manager.md),
[ADR-0041](../adr/0041-dependencies-by-path.md), [ADR-0065](../adr/0065-module-directories.md).

## Packages

- A package is the unit of compilation and distribution. It is described by `Nova.toml` and
  built by Nova, the package manager ([reference](../nova.md)). Its root module `pkg` is
  `src/main.sag` for a program or `src/lib.sag` for a library (F3), and its other modules are
  the files under `src/`.
- A single program can also be built from its entry file without a package,
  `sag build dir/main.sag` (F1). The entry file is the root module, `pkg`, and its directory is
  the package root.
- A library needs no `main`; a function called `main` in it is an ordinary function (F3).

## Modules

- **Each file is a module**, implicitly. There are no `mod` declarations.

### Files

- The directory structure is the module structure: `net/http.sag`, next to the entry file, is
  the module `pkg::net::http`.
- A module without submodules is the file `name.sag`. A module with submodules is the file
  `name/mod.sag`, in the directory that holds them: `net/mod.sag` is `pkg::net`, next to
  `net/http.sag`. A directory without `mod.sag` is an empty module that only holds submodules.
- `name.sag` next to a directory `name/` is an error (`E0209`): a module has one place for its
  file, `name/mod.sag` when it has submodules.
- `mod` cannot name a module: `mod.sag` is always the file of its directory's module, and a path
  such as `pkg::mod` names nothing (`E0200`). `mod` is not a keyword; it names anything else.
- The entry file is the root module whatever its name; a `mod.sag` next to it is no module.
- File and directory names must be identifiers and match the path exactly, also on
  case-insensitive file systems.
- Only the modules that the entry file reaches through paths (directly or through other
  modules) are compiled; other `.sag` files are ignored.
- Circular imports between modules of the same package are allowed (the whole package is
  analyzed together). Cycles between packages are not allowed.
- Items, imports and submodules share one namespace per module: a function or type cannot
  have the name of a submodule, and a name cannot be declared or imported twice.

## Paths

Paths are logical, never relative file paths. Their first segment is one of:

| Root | Refers to |
|---|---|
| `pkg` | The root of the current package (Rust's `crate`) |
| `self` | The current module |
| `super` | The parent module; it can repeat at the start: `super::super::x` |
| `std` | The standard library ([chapter 11](11-standard-library.md)) |
| A dependency's name | The root of that dependency of the current package (F3, [Nova](../nova.md#dependencies)) |
| A name in scope | An item or import of the current module, such as an imported module |

An item or import of the current module hides a dependency with the same name. In a dependency,
`pkg` is the root of that dependency.

Qualified paths can be written wherever a name can: `pkg::util::format(x)`,
`geo::Point { x: 1, y: 2 }`, `const p: geo::Point = ...`, `shapes::Shape::Circle(r)` in a
pattern.

## Imports

```sag
use std::io;
use pkg::net::http::{Client, Request};
use pkg::util::format;
use pkg::geometry::{self, Point as P};
use super::shapes;
```

- `use` brings a module, function, struct or enum into the current module under its last
  name, or under the name given with `as`. Inside braces, `self` names the module before the
  braces, and groups can nest: `use pkg::{a::B, c::{self, D}};`.
- A `use` path starts with `pkg`, `self`, `super`, `std` or the name of one of the package's
  dependencies.
- `use` declarations appear at module level, in any position, and apply to the whole module.
- An imported name shadows a prelude name, like a declared one. Importing a name that is
  already declared or imported in the module is an error; an import that is never used is a
  warning.
- Not in the MVP: re-exports (`pub use`), glob imports (`use a::*`), importing the variants
  or functions of a type (`use Color::Red`) and `use` inside blocks.

## Prelude

Every module implicitly sees these names, without `use`:

| Kind | Names | Milestone |
|---|---|---|
| Types and variants | `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `String`, `StringBuilder` | F1 |
| Functions | `print(s: String)`, `println(s: String)`, `eprintln(s: String)`, `panic(message: String)` | F1 |
| Types | `Error` (an interface) and `Error(message)` | F2 |
| Types | `List`, `Map`, `Iter`, `Entry`, `Ordering`; the interfaces `Eq`, `Ord`, `Hash` | F2 |
| Types | `Task` ([`async` and `await`](12-async.md)) | F8 |

- `print` and `println` write to stdout, `eprintln` to stderr (`println`/`eprintln` add a
  newline). If writing fails they **panic**, like Rust's `println!`. Code that must handle
  I/O errors uses the `std::io` functions, which return `Result<(), io::Error>`.
- The prelude cannot be extended by user code. A local declaration or an import with the same
  name shadows a prelude name.
- Part of the prelude is written in Sagitario (`Error` and the class behind `Error(...)`,
  `Ordering`, `Eq`, `Ord`, `Hash`, `Entry`, `Iter` and `Map`). The compiler embeds it as a module
  outside the package, whose public items every module sees, and compiles only the parts a
  program uses.

```sag
fn main() {
  println("hello");
}
```

## Visibility of items

| Modifier | Visible in |
|---|---|
| (none) | The declaring module and its submodules |
| `pub` | Every module that imports it, including other packages' |

- As in Rust, a submodule sees the private items of all its ancestors: code in
  `net/http.sag` can call a private function of the root with `pkg::helper()` or
  `super::super::helper()`. A private item of `net/http.sag` is not visible in `net/mod.sag`.
- There are no restricted forms such as Rust's `pub(crate)`: `pub(pkg)` is a syntax error.
- Modules themselves are visible in the whole package.
- Struct members follow their own rules (`pub`/`prv`, mandatory; a `prv` member is visible
  only inside its struct); enum variants have their enum's visibility. Class members follow
  [classes](04-classes.md).
- A `pub` item whose signature or fields mention a private type gets a warning: code elsewhere could receive values of a type it cannot name.
