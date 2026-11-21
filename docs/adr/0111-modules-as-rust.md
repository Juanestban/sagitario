# 0111. Modules as Rust's

- Status: Accepted
- Date: 2026-11-21
- Supersedes: [ADR-0010](0010-modules-and-packages.md) and [ADR-0024](0024-modules-imports-and-visibility.md)
  in the parts saying that every file is a module without `mod`, that modules are visible in the
  whole package, and that re-exports and glob imports are not in the MVP;
  [ADR-0025](0025-single-pub-visibility.md) (`pub` only, `pub(pkg)` a syntax error);
  [ADR-0065](0065-module-directories.md) Q349 to Q351 (only `name/mod.sag`, and `mod` not a keyword)

## Context

A module was any file a path reached: `use pkg::net::http` loaded `net/http.sag` without a
declaration. The user found it confusing, as a Rust programmer:

- a file is part of the program only if some path happens to name it, so nothing says which
  files make the program, and a mistyped path is "no module" rather than "no file";
- the language server cannot tell which files belong to the program, and it does not offer the
  files of the directory after `use pkg::`, since nothing declares them;
- every module is visible in the whole package, and there are no re-exports, globs, inline
  modules or restricted visibilities, which Rust programs use everywhere (`mod tests`,
  `use super::*`, `pub use`, `pub(crate)`).

The user asked for Rust's module system, keeping Sagitario's names: `pkg` for Rust's `crate`, and
`self` and `super`.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q790 | Declarations | A module is declared in its parent with `mod name;` (its file) or `mod name { items }` (inline), as Rust's. The root module is the entry file (`main.sag`, `lib.sag`, `build.sag`, or the file given to `sag`); only the files the tree of `mod` declarations reaches are compiled, and any other `.sag` file is not part of the program. `mod` becomes a keyword. A `mod` declaration is a module-level item: it takes `@cfg` (a module left out loads no file) and a visibility, and its name shares the module's namespace (`E0201`). |
| Q791 | Files | As Rust's: the file of `mod name;` declared in the root or in a `mod.sag` is `name.sag` or `name/mod.sag` next to that file; declared in another file `parent.sag`, it is `parent/name.sag` or `parent/name/mod.sag`. A module of an inline `mod a { mod b; }` adds `a/` to the directory. Both files existing is `E0209`; neither is `E0210`, naming the two paths. `name.sag` next to a directory `name/` is how a module holds submodules, as `name/mod.sag` is. |
| Q792 | Visibility | Items, `mod` declarations and imports are private by default: visible in their module and its descendants. `pub` makes them visible wherever their module is, other packages included; `pub(pkg)` in the whole package (Rust's `pub(crate)`); `pub(super)` in the parent module and its descendants. Using a name where it is not visible is `E0207`, also for a private module on a path (`pkg::a::b` with `mod b;` private in `a`, outside `a`). Struct and class members keep their own `pub`/`prv` rules. |
| Q793 | Re-exports | `pub use path;` (and `pub(pkg) use`, `pub(super) use`) makes the imported name a name of the module, which paths and other imports reach, as Rust's. Re-exporting a name more widely than it is visible is `E0211`. |
| Q794 | Globs | `use path::*;` imports every name of a module visible from the importing one: items, imports made visible, and submodules, as Rust's (`use super::*;` in `mod tests` sees the parent's private items). A name declared or imported by name in the module shadows a glob's. Two globs giving the same name to different things is an error only where that name is used (`E0212`). A glob from which nothing is used is the unused-import warning. Globs of enums (`use Color::*`) stay out, as importing variants does. |
| Q795 | Paths | Unchanged: `pkg`, `self`, `super` (repeatable), `std`, the names of dependencies and names in scope start a path. `pkg::a::b` follows the declared tree. |
| Q796 | Standard library and prelude | Unchanged: the modules of `std` are the compiler's and need no declaration, as Rust's `std`. |
| Q797 | Tools | The language server analyzes the tree of the entry file: a file that no `mod` reaches gets a hint, "this file is not part of the program", with the `mod` that would add it. After `mod `, it offers the files of the directory that no `mod` declares yet; after `use pkg::`, `self::` and `super::`, the modules declared and their visible items. The VS Code grammar, snippets and semantic tokens know `mod`, `pub(pkg)` and `pub(super)`. |
| Q798 | Migration | Every package of the repository (the compiler, Nova, the harness, the tools, the tests, the examples of the documentation) declares its modules. The modules that were reachable become `pub mod`, which keeps what code saw. The seed is made in two steps: the compiler with the new rules is built by the old seed from sources without `mod`, then builds the sources with `mod` declarations, which give the seed. |

## Alternatives considered

- **Keeping implicit modules, with an optional `mod`**: two ways to make a module, and the
  language server still could not know the files of a program.
- **Only `name/mod.sag` (ADR-0065)**: Rust accepts both forms, and the user chose both.
- **`crate` instead of `pkg`**: the user kept Sagitario's names.
- **`pub(in path)` and `pub(self)`**: rare in Rust code; they can come later without changing
  these rules.

## Consequences

- Every multi-file program must declare its modules: a breaking change before 1.0, recorded in
  the changelog with the rule ("add `mod name;` to the file of the parent").
- A file that is not declared is not compiled and not analyzed, as in Rust.
- The modules a dependency exposes are those it declares `pub`.
