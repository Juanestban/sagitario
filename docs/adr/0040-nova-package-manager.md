# 0040. Nova, the package manager

- Status: Accepted
- Date: 2026-10-24
- Supersedes: the manifest of [ADR-0010](0010-modules-and-packages.md) (`sagitario.toml` and
  its `kind` field)

## Context

F3 increment 4 adds packages. ADR-0010 planned a manifest called `sagitario.toml` with a `kind`
field (`bin` or `lib`), built by the compiler itself. Following Cargo, the user chose a separate
package manager, Nova, with the manifest `Nova.toml` and the commands `nova new`, `nova build`
and `nova run`. Increment 4 settles Q114–Q125.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q114 | The tool | A `nova` binary in its own crate, `sagitario_nova`, which uses `sagitario_driver` as a library rather than running `sag`. `sag` remains the compiler for single files. The future lockfile is `Nova.lock`. |
| Q115 | Manifest format | A hand-written parser of a TOML subset (crates have no dependencies): tables, bare keys, strings, integers, booleans, arrays and comments. Anything else is reported as unsupported. |
| Q116 | Contents | `[package]` with the required `name` and `version`, and the optional `description`, `authors` and `license`. Unknown keys and tables are errors. `[dependencies]` is reserved until dependencies exist. |
| Q117 | Kind of package | Deduced from the files, as in Cargo: `src/main.sag` is a program, `src/lib.sag` a library; exactly one must exist for now. There is no `kind` key. |
| Q118 | Name and version | The name is a lowercase snake_case identifier that is neither a keyword nor a path root (`pkg`, `self`, `super`, `std`, `prelude`), since dependencies will be path roots. The version follows SemVer. |
| Q119 | `nova new` and `nova init` | `new` creates a directory with `Nova.toml`, `src/main.sag` (`--lib`: `src/lib.sag`) and a `.gitignore` of `/target`. `init` does the same in the current directory without replacing files. Neither runs `git`. |
| Q120 | `nova build` | Finds the nearest `Nova.toml` from the current directory up, compiles `src/main.sag` as the root module with modules relative to `src/`, and writes `target/debug/<name>` or `target/release/<name>`. `--release`, `--sanitize`, `--emit-c`, `--cc` and `--diagnostic-format` work as in `sag`. |
| Q121 | Libraries for now | A library is fully checked, without requiring `main`, and the user is told that library files are not available yet. |
| Q122 | Other commands | `nova run [-- args]` (exits with the program's code), `nova check` and `nova clean`. |
| Q123 | Manifest errors | Rendered like compiler diagnostics, with codes of their own: `N0001`… |
| Q124 | Tests | Integration tests run the `nova` binary in temporary directories; unit tests cover the TOML parser and the manifest. A test fails if an `N` code is never reported by an integration test. |
| Q125 | Documentation | This ADR, a reference in `docs/nova.md` and the packages section of the specification. |

## Alternatives considered

- **`sag new` and `sag build` in the compiler:** one tool fewer, but the user preferred the
  Cargo model, which keeps the compiler free of package concerns.
- **`nova` running `sag` as a process:** two binaries that must be installed and versioned
  together, and an extra executable lookup.
- **A complete TOML parser:** much more code for features the manifest does not need. The subset
  can grow when dependencies need inline tables.
- **Declaring `kind`:** redundant with the files present.

## Consequences

- The compiler has a library mode (`Package::library`, `CompileOptions::library`): `main` is not
  required and not special, and the program is not lowered. The driver can also write the
  executable to a chosen path (`CompileOptions::output`).
- In a library, generic code that instantiates itself endlessly is only reported once library
  outputs lower the library's public functions.
- `nova` exits with the program's code on `nova run`, so it cannot tell a failing build from a
  program that exits with 1. Messages tell them apart.
