# 9. Modules and packages (F1)

## Packages

- A package is the unit of compilation and distribution, described by `sagitario.toml`
  (name, version, kind `bin` or `lib`).
- Entry points: `src/main.sag` for binaries, `src/lib.sag` for libraries.

## Modules

- **Each file is a module**, implicitly. There are no `mod` declarations.
- The directory structure is the module structure: `src/net/http.sag` is the module
  `net::http`.
- Circular imports between modules of the same package are allowed (the whole package is
  analyzed together). Cycles between packages are not allowed.

## Imports

```sag
use std::io;
use net::http::{Client, Request};
use pkg::util::format;
```

- Paths are logical, never relative file paths.
- `pkg::` refers to the root of the current package; `std::` to the standard library;
  other roots are dependency package names.

## Prelude

Every module implicitly sees these names, without `use`:

| Kind | Names | Milestone |
|---|---|---|
| Types and variants | `Option`, `Some`, `None`, `Result`, `Ok`, `Err`, `String` | F1 |
| Functions | `print(s: String)`, `println(s: String)`, `eprintln(s: String)` | F1 |
| Types | `List`, `Map`, `Error` | F2 |

- `print` and `println` write to stdout, `eprintln` to stderr (`println`/`eprintln` add a
  newline). If writing fails they **panic**, like Rust's `println!`. Code that must handle
  I/O errors uses the `std::io` functions, which return `Result<(), io::Error>`.
- The prelude cannot be extended by user code. A local declaration with the same name
  shadows a prelude name.

```sag
pub fn main() {
  println("hello");
}
```

## Visibility of items

| Modifier | Visible in |
|---|---|
| (none) | The declaring module (file) only |
| `pub(pkg)` | The whole package |
| `pub` | Everywhere, including other packages |

Class members follow their own rules (`pub`/`prv`, mandatory); see [classes](04-classes.md).
