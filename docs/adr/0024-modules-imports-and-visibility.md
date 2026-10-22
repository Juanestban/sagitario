# 0024. Modules, imports, visibility and the entry point

- Status: Accepted
- Date: 2026-10-22
- Amends: [ADR-0010](0010-modules-and-packages.md) (paths, `use` forms and privacy),
  [ADR-0003](0003-static-typing-and-generics.md) and [ADR-0019](0019-prelude-and-printing.md)
  (`main` no longer needs `pub`)

## Context

F1 increment 7 brings multi-file programs. ADR-0010 fixed the shape (each file is a module,
logical `use` paths rooted at `pkg::`, `std::` or a dependency, three visibility levels) but
not how a program finds its files without a package manifest, which `use` forms exist, what a
private item means for nested modules, or how paths are written outside `use`. The entry point
also required `pub fn main()`, which adds nothing in a program's root file.

## Decision

| # | Topic | Decision |
|---|---|---|
| 1 | Package root | `sag build dir/main.sag`: the entry file is the root module `pkg` and its directory is the package root. `sagitario.toml` arrives with the package manager (post-MVP). |
| 2 | Module tree | Implicit, as ADR-0010: `pkg::net::http` is `net/http.sag`; a directory without a file of its own is an empty module. File and directory names must match exactly, also on case-insensitive file systems. |
| 3 | Files compiled | Only the modules the entry file reaches through paths (transitively); other `.sag` files are ignored. |
| 4 | Path roots | `pkg::` is the package root (Rust's `crate::`), `self::` the current module, `super::` its parent (repeatable at the start: `super::super::x`). `std::` is reserved for the standard library and reported as not implemented until it exists; dependency roots wait for the package manager. `super` becomes a keyword. |
| 5 | `use` | `use path;`, `use path as name;` and nested groups `use pkg::a::{self, b, c::{d, e as f}};`. A `use` path starts with `pkg`, `self`, `super` or `std`. It imports a module, function, struct or enum; `use` may appear anywhere at module level and applies to the whole module. |
| 6 | Qualified paths | Allowed in expressions, types, struct literals and patterns: `pkg::util::format(x)`, `geo::Point { .. }`, `shapes::Shape::Circle(r)`. Their first segment is `pkg`, `self`, `super`, `Self` or a name in scope (an item or import). |
| 7 | Privacy (Rust) | An item without modifier is visible in its module and in all of that module's submodules; `pub(pkg)` in the whole package; `pub` everywhere (the same as `pub(pkg)` until there are packages). Modules themselves are visible in the whole package. Struct members keep `pub`/`prv`; variants share their enum's visibility. |
| 8 | Names | Items, imports and child modules share one namespace per module: a duplicate is an error. Imports shadow prelude names, like items. An unused import is a warning. |
| 9 | Private interfaces | A `pub`/`pub(pkg)` item whose signature or fields mention a type only its module can see is a warning, as Rust's `private_interfaces` lint. |
| 10 | Deferred | `pub use` (re-exports), glob imports (`use a::*`), importing variants or functions of a type (`use Color::Red`) and `use` inside blocks. |
| 11 | Entry point | The root module declares `fn main()` (or `fn main() -> Result<(), String>`) with any visibility; `pub fn main()` stays valid. A function called `main` in another module is an ordinary function. |

## Alternatives considered

- **Explicit `mod` declarations (Rust)**: rejected again, as in ADR-0010; the file system
  already describes the tree.
- **Compiling every `.sag` file under the root**: would also compile unrelated files (test
  directories hold many programs side by side) and needs a manifest to be predictable.
- **Private meaning "this file only"**: Rust's rule lets submodules use their ancestors'
  helpers through `super::` without widening them to the whole package.
- **`use` relative to names in scope (Rust 2018)**: makes imports depend on other imports; with
  explicit roots every `use` resolves on its own.

## Consequences

- The driver loads the entry file, asks the resolver which module paths each file refers to,
  loads the ones that exist as files and repeats; then the package is analyzed together.
- Diagnostics and `#line` directives carry the file of each span; a label in another file is
  shown with its own excerpt.
- Test cases can be directories with a `main.sag` entry.
