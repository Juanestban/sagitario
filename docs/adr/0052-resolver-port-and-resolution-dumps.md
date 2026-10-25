# 0052. The resolver port and resolution dumps

- Status: Accepted
- Date: 2026-10-25
- Refines: [ADR-0051](0051-parser-port-and-ast-dumps.md)

## Context

F5 increment 4 ports name resolution (about 2 900 lines of Rust in `lib.rs` and `modules.rs`,
with 63 distinct diagnostics) and the part of the driver that loads a program's modules (about
220 lines). Unlike the parser, the resolver produces tables rather than a tree: functions,
types, modules and locals, and maps keyed by node id (paths, type paths, bindings, lambdas).
Increment 4 settles Q243–Q249.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q243 | The dump | `sag check --dump=resolve file.sag` does what `check` does up to and including `resolve`: it loads the module tree, the prelude and the `std` modules the program names. It prints the `Resolution` to stdout and the lexer, parser and resolver diagnostics to stderr, stopping on lexical and syntax errors as the driver does. |
| Q244 | Format | Sections in a fixed order, one entry per line: modules (path, `std`, package, root, source, parent, then children, items and imports, each sorted by name), functions, structs, enums, classes, interfaces, constants and externs (with their module, item and member), the global entries (`prelude`, `std_root`, `package_roots`, `error_interface`, `main`), the locals of each function, and the four maps sorted by node id: `path #12 -> fn 3`, `type_path #7 -> struct 0`, `binding #40 -> local 2`, `lambda #9 params=3 first=3 end=5 captures=0,1`. Hand-written dumpers in `sagitario_resolve::dump` and `compiler/src/resolve/dump.sag`. |
| Q245 | The prelude and `std` in `sagc` | Embedded with `include_str` (ADR-0049): `include_str("../../crates/sagitario_driver/src/prelude.sag")` and one constant per `std` module. Their file names in diagnostics (`<prelude>`, `<std>`, `<std::io>`) and the `range_<int>` functions generated from the template are the driver's. The template is written line by line, so its text does not depend on the line endings of the checkout. |
| Q246 | The loader | `compiler/src/loader.sag` ports the loading part of the driver: exact-name file lookup through `read_dir` (also on case-insensitive file systems) and the discovery of modules and `std` modules. Nova dependencies are left out until the driver is ported (increment 7), since `check` on a file has none. The resolver is ported whole, packages included. |
| Q247 | Tables in Sagitario | Ids (`FnId`, `ModId`, ...) are `u32`, and maps by node id are `Map<u32, ...>`, sorted when dumped. Modules, the resolution and lambda information are classes, which later phases fill in place; the rest are structs and enums. The set of used imports is a `Map<String, bool>` keyed by `module:name`. |
| Q248 | Corpus | `tests/resolve/`: scopes, captures and patterns, unresolved names and their suggestions, and a program of several modules with imports, aliases, `self`/`super`, visibility and the import warnings. With every `.sag` file of the repository, in both diagnostic formats; a file that is not a program's root reports a missing `main` in both compilers, which is also a valid comparison. |
| Q249 | Rust changes | The dump, and two fixes where the result depended on the iteration order of a `HashMap`, which is not deterministic: the candidates of the "similar name" suggestions among a module's items, imports and submodules are sorted, so among equally close names the first one in alphabetical order is suggested. |

## Alternatives considered

- **Dumping with `{:#?}`:** `HashMap`s print in an arbitrary order, and Sagitario has no
  equivalent of `Debug`.
- **Loading the prelude and `std` from files at run time:** `sagc` would depend on the location
  of the repository, and `include_str` exists for this.
- **Porting the Nova dependency loading now:** nothing would exercise it until the driver.

## Consequences

- The driver's loading code is now `load_package`, shared by `front_end` and the dump.
- `sagc` recognizes the entry file among a package's modules by name (a root-level module
  whose file name is the entry's), where the driver compares canonical paths: Sagitario has no
  `canonicalize`. They differ only for an entry reached through a link or another spelling of
  its directory.
- File paths in diagnostics are built as Rust's `PathBuf::push` builds them, with `\` on
  Windows, so both compilers name the same files.
