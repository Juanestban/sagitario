# Contributing to Sagitario

## Requirements

- clang (the official C compiler). On Windows: official LLVM (`x86_64-pc-windows-msvc`) plus
  Visual Studio Build Tools, for its linker and C library. If `clang` on your `PATH` is an
  MSYS2/MinGW clang, set `SAG_CC` to the LLVM one; MinGW works best-effort, without sanitizers.
  MSVC `cl` also works best-effort, without sanitizers, from a Developer Command Prompt. gcc is
  optional on Linux.
- A 64-bit system: the seed of the compiler does not build on 32-bit hosts.
- Optionally, the Android NDK (`ANDROID_NDK_HOME`) for the cross-compilation tests, and Node.js
  for the VS Code extension.

## Commands

```sh
sh bootstrap/build.sh           # build/bin/sag and build/bin/nova from the seed, with a C compiler alone
bootstrap/build.ps1             # the same on Windows (PowerShell)
sh bootstrap/build.sh --install # also copy them to ~/.sagitario/bin and add it to PATH
sh bootstrap/build.sh --debug   # the compiler in debug: an integer overflow in it panics
sh bootstrap/build.sh --update-seed   # write the C of the compiler and the runtime to bootstrap/
SAG_CC=gcc sh bootstrap/build.sh      # choose the C compiler

sag build main.sag              # compile
sag run main.sag -- a b         # compile and run, forwarding arguments
sag check main.sag              # front end only
sag check --dump=types main.sag # the result of a pass: tokens, ast, resolve, declarations, types, c
sag build main.sag --emit-c     # also write the generated C
sag run --release main.sag      # optimized; integer overflow wraps
sag run --sanitize main.sag     # with ASan and UBSan (the test harness always uses it)
sag run --sanitize=thread main.sag   # with TSan instead, on Linux and macOS
SAG_CC=gcc sag build main.sag   # choose the C compiler (or --cc)

sag run tests/harness/main.sag -- --sag build/bin/sag --nova build/bin/nova   # every test (docs/testing.md)
sag run tests/harness/main.sag -- --sag build/bin/sag suites lsp             # some groups
SAG_BLESS=1 sag run tests/harness/main.sag -- --sag build/bin/sag dumps lsp  # rewrite expected files
SAG_TEST_SANITIZE=0 sag run tests/harness/main.sag -- --sag build/bin/sag   # faster, without sanitizers
ANDROID_NDK_HOME=/path/to/ndk sag run tests/harness/main.sag -- --sag build/bin/sag   # also Android

nova new demo                   # a package (docs/nova.md); then, inside it:
nova run -- a b                 # build target/debug/demo and run it
nova check                      # front end only

cd editors/vscode && npm ci && npm run package   # the VS Code extension: sagitario.vsix
code --install-extension editors/vscode/sagitario.vsix   # it starts `sag lsp` from PATH
```

On Windows, `build/bin/sag.exe` and `build/bin/nova.exe`; with `cl`, run the commands from a
Developer Command Prompt and set `SAG_TEST_SANITIZE=0`.

The compiler, Nova and the harness must build without warnings: the bootstrap scripts fail if
`sag check` reports anything for them.

## Documentation map

- Language rules: [`docs/spec/`](docs/spec/README.md). Decisions and their rationale:
  [`docs/adr/`](docs/adr/README.md).
- Compiler structure: [`docs/architecture.md`](docs/architecture.md).
- Test harness: [`docs/testing.md`](docs/testing.md).
- Package manager: [`docs/nova.md`](docs/nova.md).
- Phases and acceptance criteria: [`docs/roadmap.md`](docs/roadmap.md).

## Definition of Done (every language feature)

1. The spec section is updated.
2. A `run-pass` test exists, plus one `compile-fail` test per rule that can be violated.
3. New diagnostics have a registered code, a clear message and a `compile-fail` case (enforced
   by the test suite); add labels or `help` when they point to the fix.
4. The bootstrap (`bootstrap/build.sh` or `build.ps1`) and the harness pass on all CI platforms.

## Changing the language

Do not change syntax or semantics in code alone. Write a new ADR (superseding the old one if
needed), update the spec, then implement.

## Conventions

- Everything in English: code, comments, diagnostics, commit messages, documentation.
- [Conventional Commits](https://www.conventionalcommits.org/) with the part of the compiler as
  scope, e.g. `feat(parser): parse match expressions`, `fix(types): reject implicit i32 to i64`.
  Use `docs`, `ci`, `runtime`, `nova`, `lsp` or `harness` as scope for other changes.
- Branches `feat/...` and `fix/...`; pull requests into `main`; CI must be green to merge.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Unless you state otherwise, any contribution you submit is dual-licensed the same way.
