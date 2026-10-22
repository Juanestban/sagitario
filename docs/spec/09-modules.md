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

## Visibility of items

| Modifier | Visible in |
|---|---|
| (none) | The declaring module (file) only |
| `pub(pkg)` | The whole package |
| `pub` | Everywhere, including other packages |

Class members follow their own rules (`pub`/`prv`, mandatory); see [classes](04-classes.md).
