# Testing

Rationale: [ADR-0016](adr/0016-engineering-process.md).

## Levels

| Level | Location | What it checks |
|---|---|---|
| Pass dumps | `tests/lexer/`, `tests/parser/`, `tests/resolve/`, `tests/types/`, `tests/codegen/` | The tokens, syntax tree, resolution, types or C of each case, with its diagnostics, against `<case>.expected`. |
| `run-pass` | `tests/run-pass/*.sag` | Program compiles, runs, and matches expected stdout and exit code. |
| `compile-fail` | `tests/compile-fail/*.sag` | Compiler rejects the program with the expected diagnostic codes at the expected lines. |
| `run-panic` | `tests/run-panic/*.sag` | Program panics with the expected message and exit code 101. |
| Language server | `tests/lsp/` | Sessions of `sag lsp` against the answers they expect. |
| Command line, Nova, editor | `tests/harness/` | Exit codes and messages of `sag`, Nova end to end, and the VS Code grammar. |

A case is a single `.sag` file, or a directory holding a multi-file program whose entry file
is `main.sag` (for example `tests/run-pass/modules/main.sag` with `geometry.sag` and
`util/strings.sag` beside it).

Every level runs through the harness in Sagitario (`tests/harness/`, below), in the style of
rustc's `compiletest`. It runs the `sag` under test: `sag run <file>` for `run-pass` and
`run-panic`, and `sag check --diagnostic-format short <file>` for `compile-fail` (expected exit
code 1). They require a C compiler (see [architecture](architecture.md#c-toolchain)).

## Annotations

Annotations are line comments inside the `.sag` test file.

```sag
// expect-stdout: total = 6
// expect-exit: 0
fn main() { ... }
```

```sag
const x: i32 = "hello";   //~ ERROR E0042
```

```sag
// expect-panic: index out of bounds
fn main() { ... }
```

- `expect-stdout` may appear several times; lines are matched in order. In a multi-file case,
  `expect-*`, `sag-flags`, `args`, `stdin` and `env` annotations are read from `main.sag`, and `//~` annotations
  from every file of the program (each one matched against its own file).
- `// sag-flags: --release` passes extra options to `sag run` (e.g. to test release-mode
  wrapping).
- `// sanitize: thread` runs the case with `--sanitize=thread` instead of `--sanitize`:
  ThreadSanitizer on Linux and macOS, ASan and UBSan elsewhere
  ([ADR-0045](adr/0045-threads-and-send.md)).
- `// args: a b` passes command-line arguments to the program, split on spaces.
  `// stdin: text` writes a line (`text` and `\n`) to its stdin and may repeat; stdin is closed
  after the last line, or at once without any. `// env: NAME=value` sets an environment
  variable for the program and may repeat.
- `run-pass` and `run-panic` programs run in a new empty working directory, removed afterwards,
  so they can create files with relative paths.
- Panics print `panic at <file>:<line>: <message>`; `expect-panic` matches a substring.
- `//~ ERROR Ennnn` and `//~ WARN Wnnnn` assert a diagnostic with that code on the same line.
  Several can share a line: `//~ ERROR E0311, WARN W0001 WARN W0002`.
- In `compile-fail`, every reported error **and warning** must be annotated, and every
  annotation must be reported. A file with only `WARN` annotations must compile (exit code 0);
  otherwise the exit code must be 1. Prefix incidental bindings with `_` to keep tests focused.
- `every_diagnostic_code_has_a_compile_fail_test` fails if a code in
  `compiler/src/diagnostics/codes.sag`, the registry of codes, has no `compile-fail` case.

## Memory checks

- The runtime counts live ARC objects. When the environment variable `SAG_LEAK_CHECK` is set
  (the harness always sets it), a program that exits normally with objects still alive prints
  `leak check: N ARC object(s) still alive at exit` and exits with code 102, failing the test.
- The harness runs every `run-pass` and `run-panic` case with `sag run --sanitize`: ASan and
  UBSan, with any undefined behavior aborting the program, so a report fails the test. Toolchains
  without sanitizers (MinGW) build without them. `SAG_TEST_SANITIZE=0` turns them off for faster
  local runs ([ADR-0036](adr/0036-sanitizers-and-the-reference-zoo.md)).
- `tests/run-pass/reference_zoo/` is the reference program of the F2 acceptance criteria.
- `tests/run-pass/reference_wordcount/` is the reference program of the F4 acceptance
  criteria, run with ThreadSanitizer ([ADR-0048](adr/0048-async-await.md)).
- `tests/acceptance/f3/` is the F3 acceptance program: the `wc` and `textcount` packages and a C
  program using the library, run by the test `f3_acceptance` of the harness
  ([ADR-0044](adr/0044-c-toolchains.md)).

## Rules

- The compiler must never panic on user input. Any internal compiler error found is fixed
  and gets a regression test.
- Every language feature needs at least one `run-pass` test, and one `compile-fail` test per
  rule a program can violate.
- Benchmarks (fibonacci, n-body, binary-trees versus equivalent C) are informational and do
  not gate the MVP.

## The harness in Sagitario

`tests/harness/` is a program in Sagitario that runs every test
([ADR-0066](adr/0066-test-harness-in-sagitario.md)). From the root of the repository:

```sh
sag run tests/harness/main.sag -- --sag path/to/sag            # every group
sag run tests/harness/main.sag -- --sag path/to/sag suites lsp # some of them
SAG_BLESS=1 sag run tests/harness/main.sag -- --sag path/to/sag dumps lsp   # rewrite expected files
```

`--sag` (or `SAG_COMPILER`) is the compiler under test; `--nova` (or `NOVA_BINARY`) is Nova, which
the harness otherwise builds from `compiler/src/nova_main.sag` with that compiler. The groups:

| Group | What it checks |
|---|---|
| `suites` | `run-pass`, `compile-fail` and `run-panic`, as above, and that every diagnostic code has a `compile-fail` case. |
| `dumps` | Each case of `tests/lexer`, `tests/parser`, `tests/resolve`, `tests/types` and `tests/codegen` against `<case>.expected` next to it: the exit code, the dump of that pass (`--dump=tokens`, `ast`, `resolve`, `types`, `c`) and the diagnostics, made for `x86_64-linux-android` so that they are the same on every host. These directories hold the edge cases of each pass. |
| `lsp` | Each `tests/lsp/<case>.session` sent to `sag lsp` (one message per line, `raw ` for a body sent as written) about `<case>.sag`, against `<case>.expected`, one message per line, with `{uri}` for the document, `{dir}` for its directory and `{root}` for the repository ([ADR-0058](adr/0058-language-server.md)). |
| `editor` | The grammar of the VS Code extension (`editors/vscode/`) has every keyword of `docs/spec/01-lexical.md` ([ADR-0057](adr/0057-editor-tooling-and-independence-from-rust.md)). |
| `cli` | Exit codes and messages of the command line. |
| `nova` | Nova end to end, in temporary packages, also building C programs with its libraries and, with the Android NDK, cross-compiling. |

A change to the compiler that changes a dump or an answer of the language server fails the
harness; once the change is intended, `SAG_BLESS=1` rewrites the expected files and their diff is
reviewed with the change.

## The bootstrap from the seed

`bootstrap/build.sh` (Linux, macOS) and `bootstrap/build.ps1` (Windows) build the compiler from
`bootstrap/sag.c` with a C compiler alone ([ADR-0067](adr/0067-bootstrap-from-the-seed.md)): the
seed is stage 0, which builds stage 1, which builds stage 2. They check that stages 1 and 2
generate the same C for the compiler, and that `sag check` reports nothing for the compiler, Nova
and the harness, then build Nova; the result is `build/bin/sag` and `build/bin/nova`. `--debug`
builds the stages in debug, `--update-seed` writes the C of stage 2 and the runtime to
`bootstrap/`, and `--install` copies `sag` and `nova` to `~/.sagitario/bin` and adds it to `PATH`.

## CI

GitHub Actions, with a `bootstrap` job per system and C compiler: on Windows with official clang,
`cl` (without sanitizers) and MinGW gcc, on Ubuntu with clang and gcc, on Ubuntu for AArch64 with
clang (where C's `char` is unsigned), and on macOS with clang. Each builds the compiler from the
seed with the script of its system, in debug, and runs the harness with stage 2 and its Nova. The
Ubuntu jobs on x86-64 require the Android NDK (`SAG_REQUIRE_NDK=1`). The `same-c` job
checks that the C stage 2 generates for the compiler is the same in all of them, and the `vscode`
job packages the VS Code extension.

A tag `vX.Y.Z` runs the release workflow instead (`.github/workflows/release.yml`,
[ADR-0068](adr/0068-releases-and-distribution.md)): on each platform of the releases, also macOS on
x86-64, it builds in release mode with `--dist` and runs the harness with
what it built, before publishing the archives with the installers. Then it installs the release
with the commands of the README on Windows, Linux and macOS, and builds and runs a new package with
it. In CI, the `installers` job installs the archive that `--dist` writes, through
`--from file://…`, on the same three systems ([ADR-0069](adr/0069-installers.md)).
