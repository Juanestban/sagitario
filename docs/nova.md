# Nova, the package manager

Rationale: [ADR-0040](adr/0040-nova-package-manager.md).

Nova creates, builds, runs and checks Sagitario packages. It compiles them with the same
compiler as `sag`, which stays the tool for single files.

```sh
nova new hello        # creates hello/ with Nova.toml and src/main.sag
cd hello
nova run              # Compiling hello v0.1.0 / Hello, world!
```

## A package

```
hello/
  Nova.toml           the manifest
  .gitignore          `/target`
  src/
    main.sag          the root module `pkg` of a program
    util/strings.sag  the module `pkg::util::strings`
  target/
    debug/hello       built by `nova build` (`hello.exe` on Windows)
    release/hello     built by `nova build --release`
```

A package is a **program** if it has `src/main.sag`, or a **library** if it has `src/lib.sag`;
it must have exactly one of them. Modules are the files under `src/`, as described in
[modules](spec/09-modules.md).

Programs use libraries through [dependencies](#dependencies). Libraries are checked but not
built into files yet: building `.a`, `.so` and `.dll` files comes with library outputs.

## Commands

| Command | Effect |
|---|---|
| `nova new <name>` | Creates the directory `<name>` with a new program (`--lib`: a library) |
| `nova init` | Creates a package in the current directory, named after it (`--name <name>` to choose; `--lib`). It never replaces a file: an existing `src/main.sag` or `src/lib.sag` decides the kind of package |
| `nova build` | Compiles the package into `target/debug/<name>` |
| `nova run [-- args]` | Compiles and runs the program with `args`; Nova exits with the program's exit code |
| `nova check` | Checks the package without generating code |
| `nova clean` | Removes `target/` |
| `nova help`, `nova version` | Usage and version |

`build`, `run`, `check` and `clean` work in the package's directory or any directory below it:
Nova uses the nearest `Nova.toml` above.

| Option | Commands | Effect |
|---|---|---|
| `--release` | build, run | Optimizes, and integer overflow wraps; output in `target/release/` |
| `--sanitize` | build, run | AddressSanitizer and UndefinedBehaviorSanitizer, where the C compiler has them |
| `--emit-c` | build, run | Also writes the generated C next to the executable |
| `--cc <compiler>` | build, run | The C compiler; overrides `SAG_CC` |
| `--diagnostic-format <fmt>` | build, run, check | `human` (default) or `short` |

Exit codes: 0 on success, 1 when the package has errors or a command fails, 2 for invalid
usage, and the program's own code for `nova run`.

## `Nova.toml`

```toml
[package]
name = "hello"
version = "0.1.0"
description = "Greets the world"   # optional
authors = ["Ada <ada@example.com>"] # optional
license = "MIT OR Apache-2.0"        # optional
```

| Key | Value |
|---|---|
| `name` | Required. A lowercase identifier in snake_case (`my_app`) that is neither a keyword nor `pkg`, `self`, `super`, `std` or `prelude`, since other packages will import it by this name |
| `version` | Required. A [Semantic Versioning](https://semver.org) version: `MAJOR.MINOR.PATCH`, optionally followed by `-pre.release` and `+build` |
| `description`, `license` | Optional strings |
| `authors` | An optional array of strings |

`[dependencies]` lists the package's [dependencies](#dependencies). Any other key or table is
an error.

### Syntax

`Nova.toml` uses a subset of [TOML](https://toml.io): table headers (`[package]`), `key = value`
lines with bare keys, comments (`#`) and these values:

| Value | Example |
|---|---|
| Strings, with the escapes `\"` `\\` `\n` `\t` `\r` `\b` `\f` `\uXXXX` `\UXXXXXXXX` | `"hello"` |
| Literal strings, without escapes | `'C:\path'` |
| Decimal integers, with `_` between digits | `1_000`, `-5` |
| Booleans | `true`, `false` |
| Arrays, over several lines if needed, with an optional trailing comma | `["a", "b"]` |
| Inline tables, on one line and without a trailing comma | `{ path = "../util" }` |

Other TOML features (dotted and quoted keys, arrays of tables, multi-line strings, floats and
dates) are reported as unsupported.

## Dependencies

Rationale: [ADR-0041](adr/0041-dependencies-by-path.md).

```toml
[dependencies]
geometry = { path = "../geometry" }
```

```sag
use geometry::shapes::Circle;

fn main() {
  println(`${geometry::area(Circle { radius: 2.0 })}`);
}
```

- A dependency is a library package found by its `path`, relative to the directory of the
  `Nova.toml` that names it (or absolute). Its key must be the dependency's own `name`.
- Its name is a path root in the dependent package, like `std`: `use geometry::shapes::Circle;`
  or `geometry::area(c)`. A module-level item or import with the same name hides it.
- Only its `pub` items can be used, through the path of the module declaring them: the
  dependency's modules are all reachable, and `pub` marks its API.
- Each package sees only its own dependencies by name. A package reached through several
  dependencies (the same directory) is compiled once.
- Packages cannot depend on each other in a cycle.
- The whole graph is compiled together into one program, so a generic function of a dependency
  is instantiated with the types its users pass.
- Errors in a dependency's code are reported with the paths of its files.
- Not yet: dependencies from git or a registry (and with them `Nova.lock`), renaming a
  dependency, development dependencies, features and optional dependencies.

## Errors

Errors in `Nova.toml` and in the package's layout have codes with the prefix `N`, and are shown
like the compiler's diagnostics, with their location:

```
error[N0006]: `Hello App` is not a valid package name: it must start with a lowercase letter and have only lowercase letters, digits and `_`
 --> Nova.toml:2:8
  |
2 | name = "Hello App"
  |        ^^^^^^^^^^^
  = note: the name of a package is the path root other packages import it with
  = help: use `hello_app`
```

| Code | Meaning |
|---|---|
| N0001 | Text that is not the TOML subset of `Nova.toml` |
| N0002 | A key or table defined twice |
| N0003 | A key or table that `Nova.toml` does not have |
| N0004 | A missing `[package]` table, `name` or `version` |
| N0005 | A value of the wrong type |
| N0006 | An invalid package name |
| N0007 | An invalid version |
| N0008 | A part of `Nova.toml` that is not available yet (dependencies by `version` or `git`) |
| N0009 | A package without exactly one of `src/main.sag` and `src/lib.sag` |
| N0010 | A dependency whose `path` holds no package |
| N0011 | A dependency whose package has another name |
| N0012 | A dependency that is a program, not a library |
| N0013 | Packages that depend on each other in a cycle |
