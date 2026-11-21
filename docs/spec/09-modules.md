# 9. Modules and packages (F1)

Rationale: [ADR-0010](../adr/0010-modules-and-packages.md),
[ADR-0024](../adr/0024-modules-imports-and-visibility.md),
[ADR-0025](../adr/0025-single-pub-visibility.md), [ADR-0040](../adr/0040-nova-package-manager.md),
[ADR-0041](../adr/0041-dependencies-by-path.md), [ADR-0065](../adr/0065-module-directories.md),
[ADR-0111](../adr/0111-modules-as-rust.md).

## Packages

- A package is the unit of compilation and distribution. It is described by `Nova.toml` and
  built by Nova, the package manager ([reference](../nova.md)). Its root module `pkg` is
  `src/main.sag` for a program or `src/lib.sag` for a library (F3), and its other modules are
  those it declares with `mod`. A build script, `build.sag`, is the root of its own tree.
- A single program can also be built from its entry file without a package,
  `sag build dir/main.sag` (F1). The entry file is the root module, `pkg`, and its directory is
  the package root.
- A library needs no `main`; a function called `main` in it is an ordinary function (F3).

## Modules

```sag
// main.sag
mod net;            // net.sag or net/mod.sag
pub mod util;       // util.sag or util/mod.sag

mod tests {         // an inline module
  use super::*;

  fn check() {
    util::helper();
  }
}
```

- A module is declared in its parent with `mod name;`, whose items are in a file, or
  `mod name { items }`, inline, as in Rust. The root module, `pkg`, is the entry file: `main.sag`,
  `lib.sag`, `build.sag`, or the file given to `sag`.
- **Only the files that the tree of `mod` declarations reaches are compiled.** Any other `.sag`
  file is not part of the program, whatever paths name it: `use pkg::esp;` without `mod esp;` is
  `E0200`.
- A `mod` declaration is a module-level item: it takes a [visibility](#visibility-of-items) and
  `@cfg` (a module left out loads no file), and its name shares the module's namespace.

### Files

- The file of `mod name;` declared in the root or in a `mod.sag` is `name.sag`, or
  `name/mod.sag`, next to that file. Declared in another file, `parent.sag`, it is
  `parent/name.sag` or `parent/name/mod.sag`. A module of an inline `mod a { mod b; }` adds `a/`
  to the directory.
- A module with submodules can be `net.sag` next to the directory `net/`, or `net/mod.sag`; both
  files existing is an error (`E0209`), and neither is `E0210`.
- `mod` is a keyword: `mod.sag` is always the file of its directory's module.
- File and directory names must be identifiers and match the path exactly, also on
  case-insensitive file systems.
- Circular imports between modules of the same package are allowed (the whole package is
  analyzed together). Cycles between packages are not allowed.
- Items, imports and submodules share one namespace per module: a function or type cannot
  have the name of a submodule, and a name cannot be declared or imported twice (`E0201`).

## Paths

Paths are logical, never relative file paths. Their first segment is one of:

| Root | Refers to |
|---|---|
| `pkg` | The root of the current package (Rust's `crate`) |
| `self` | The current module |
| `super` | The parent module; it can repeat at the start: `super::super::x` |
| `std` | The standard library ([chapter 11](11-standard-library.md)) |
| A dependency's name | The root of that dependency of the current package (F3, [Nova](../nova.md#dependencies)) |
| A name in scope | An item, import or `mod` of the current module, such as an imported module |

An item or import of the current module hides a dependency with the same name. In a dependency,
`pkg` is the root of that dependency.

`pkg::a::b` follows the tree of `mod` declarations. Qualified paths can be written wherever a name
can: `pkg::util::format(x)`,
`geo::Point { x: 1, y: 2 }`, `const p: geo::Point = ...`, `shapes::Shape::Circle(r)` in a
pattern.

## Imports

```sag
use std::io;
use pkg::net::http::{Client, Request};
use pkg::util::format;
use pkg::geometry::{self, Point as P};
use super::shapes;
use shapes::*;
pub use http::Client as HttpClient;
```

- `use` brings a module, function, struct or enum into the current module under its last
  name, or under the name given with `as`. Inside braces, `self` names the module before the
  braces, and groups can nest: `use pkg::{a::B, c::{self, D}};`.
- A `use` path starts as any [path](#paths): `pkg`, `self`, `super`, `std`, a dependency, or a
  name in scope such as a module declared with `mod`.
- `use` declarations appear at module level, in any position, and apply to the whole module.
- An imported name shadows a prelude name, like a declared one. Importing a name that is
  already declared or imported in the module is an error; an import that is never used is a
  warning.
- **Re-exports.** `pub use path;`, and `pub(pkg) use`, `pub(super) use`, make the imported name a
  name of the module, which paths and other imports reach. A plain `use` is private: its name is
  seen by the module and its submodules. Re-exporting a name more widely than it is visible is
  `E0211`.
- **Globs.** `use path::*;` imports every name of the module at `path` that the importing module
  sees: its items, its re-exports, its submodules and its own globs (`use super::*;` in an inline
  `mod tests` sees the parent's private items). A name declared, imported by name or declared with
  `mod` in the module shadows a glob's. Two globs giving one name to different things is an error
  only where the name is used (`E0212`). A glob from which nothing is used is a warning.
- Not supported: importing the variants or functions of a type (`use Color::Red`,
  `use Color::*`) and `use` inside blocks.

## Prelude

Every module implicitly sees these names, without `use`:

| Kind | Names | Milestone |
|---|---|---|
| Types and variants | `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `String`, `StringBuilder` | F1 |
| Functions | `print(s: String)`, `println(s: String)`, `eprintln(s: String)`, `panic(message: String)` | F1 |
| Types | `Error` (an interface) and `Error(message)` | F2 |
| Types | `List`, `Map`, `Iter`, `Entry`, `Ordering`; the interfaces `Eq`, `Ord`, `Hash` | F2 |
| Types | `HashSet` ([sets](02-types.md#sets-s2)) | S2 |
| Types | The interface `Drop` ([classes](04-classes.md#drop)) | S2 |
| Types | The interfaces `Iterable` and `FromIterator` ([iteration](05-interfaces-generics.md#iteration-and-collection)) | S2 |
| Functions | `min<T: Ord>(a, b)`, `max<T: Ord>(a, b)` ([order](05-interfaces-generics.md#equality-order-and-hashing-f2)) | S2 |
| Types | The interface `Display` ([format strings](02-types.md#string-f1)) | S2 |
| Types | The interfaces `From`, `Into`, `TryFrom`, `TryInto` ([conversions](05-interfaces-generics.md#conversions)) and `Default` ([defaults](05-interfaces-generics.md#defaults)) | S2 |
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
| `pub(super)` | The parent module and its submodules |
| `pub(pkg)` | The whole package (Rust's `pub(crate)`) |
| `pub` | Everywhere its module is visible, other packages included |

- The modifiers apply to items, `mod` declarations and `use` declarations.
- As in Rust, a submodule sees the private items of all its ancestors: code in
  `net/http.sag` can call a private function of the root with `pkg::helper()` or
  `super::super::helper()`. A private item of `net/http.sag` is not visible in `net/mod.sag`.
- Using a name where it is not visible is `E0207`, also a private module on a path:
  `pkg::a::b`, with `mod b;` private in `a`, outside `a`. `pub(in path)` and `pub(self)` do not
  exist; `pub(crate)` is a syntax error that suggests `pub(pkg)`.
- Struct members follow their own rules (`pub`/`prv`, mandatory; a `prv` member is visible
  only inside its struct); enum variants have their enum's visibility. Class members follow
  [classes](04-classes.md).
- A `pub` item whose signature or fields mention a private type gets a warning: code elsewhere could receive values of a type it cannot name.

## Conditional compilation (S1)

Rationale: [ADR-0085](../adr/0085-conditional-compilation-and-std-time.md).

```sag
@cfg(windows)
extern "C" {
  fn GetCurrentProcessId() -> u32;
}

@cfg(unix)
extern "C" {
  fn getpid() -> c_int;
}

@cfg(any(target_os = "linux", target_os = "android"))
const CLOCK_MONOTONIC: c_int = 1;
@cfg(any(target_os = "macos", target_os = "ios"))
const CLOCK_MONOTONIC: c_int = 6;
```

- `@cfg(condition)`, as Rust's `#[cfg(...)]`, keeps an item only when its condition holds for
  the target being compiled: the one of `--target`, or the machine running the compiler.
- The conditions are a closed set (`E0105` lists them):

  | Condition | Holds on |
  |---|---|
  | `windows` | Windows |
  | `unix` | Linux, macOS, Android and iOS |
  | `target_os = "..."` | `"windows"`, `"linux"`, `"macos"`, `"android"` or `"ios"` |
  | `target_arch = "..."` | `"x86_64"`, `"aarch64"` or `"arm"` (32-bit ARM) |
  | `target_pointer_width = "..."` | `"32"` or `"64"` |
  | `all(c, ...)`, `any(c, ...)`, `not(c)` | every, one, or not the condition; `all()` holds and `any()` does not |

- It applies to module-level items: functions, structs, enums, unions, classes, interfaces,
  constants, statics, `use` declarations, `impl` blocks and `extern "C"` blocks, and to each
  function inside an `extern "C"` block. On a field or a method it is an error (`E0105`).
- An item has at most one `@cfg`; `all(...)` combines conditions.
- An item whose condition does not hold is parsed and left out before names are resolved: its
  syntax errors are reported, but its names and types are not checked, and a `use` left out
  loads no module. Two items may have the same name if their conditions exclude each other; if
  both hold, the name is repeated (`E0201`).
- `sag check --target` checks the program as that target sees it.

## Built-in macros (S4)

Rationale: [ADR-0103](../adr/0103-build-scripts-and-binding-generation.md) (Q723-Q726),
[ADR-0104](../adr/0104-include-str-as-a-macro.md).

```sag
include!(concat!(env!("OUT_DIR"), "/sdl.sag"));

const SHADER: String = include_str!("shaders/sprite.glsl");
const ICON: [u8; 1150] = include_bytes!("assets/icon.ico");

fn main() {
  println(concat!("built for ", env!("TARGET"), " with SDL ", 3));
}
```

Sagitario has Rust's built-in macros that read files and the environment when the program is
compiled. Each one is called as `name!(...)`, and its name has no path. Programs cannot define
macros, and any other `name!(...)` is an error (`E0346`).

- `include!(path);` is written where a module-level item may be, also after `@cfg`. It puts the
  items and imports of the file in the current module, as if written there; their errors are
  reported in that file. The file cannot call `include!` itself (`E0346`). Written as an
  expression, it is an error (`E0346`).
- `include_str!(path)` is a string literal with the text of the file, which must be UTF-8, and
  `include_bytes!(path)` an array literal `[u8; N]` with its bytes. Both are expressions, valid
  anywhere a literal is, also in constants.
- `concat!(a, b, ...)` is a string literal joining literals: strings, integers, floats (also
  negative), characters and `bool`s, and the macros among them. Anything else is `E0346`.
- `env!("NAME")` is a string literal with the value of the environment variable `NAME` of the
  compiler. A variable that is not set is an error (`E0346`); `env!("NAME", "message")` reports
  `message` instead.
- A path is a string literal, or a `concat!` and `env!` of them. A relative path starts from the
  directory of the source file that calls the macro; an embedded module of the standard library
  has none (`E0346`). A file that cannot be read is `E0335`, with the reason.
- The macros are expanded while the file is parsed, before names are resolved: what they give
  is a literal, or the items of a file.
