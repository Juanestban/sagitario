# Contributing to Sagitario

## Requirements

- Rust 1.90 or newer (edition 2024).
- clang (the official C compiler). On Windows: official LLVM (`x86_64-pc-windows-msvc`) plus
  Visual Studio Build Tools, which the Rust MSVC toolchain already needs. If `clang` on your
  `PATH` is an MSYS2/MinGW clang, set `SAG_CC` to the LLVM one; MinGW works best-effort,
  without sanitizers. gcc is optional on Linux.

## Commands

```sh
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace          # includes the end-to-end .sag suites

sag build main.sag              # compile
sag run main.sag -- a b         # compile and run, forwarding arguments
sag check main.sag              # front end only
sag build main.sag --emit-c     # also write the generated C
sag run --release main.sag      # optimized; integer overflow wraps
sag run --sanitize main.sag     # with ASan and UBSan (the test harness always uses it)
SAG_TEST_SANITIZE=0 cargo test --workspace   # faster local runs without sanitizers
SAG_CC=gcc sag build main.sag   # choose the C compiler (or --cc)
```

Formatting is defined by `rustfmt.toml` and lint policy by `Cargo.toml` and `clippy.toml`.

## Documentation map

- Language rules: [`docs/spec/`](docs/spec/README.md). Decisions and their rationale:
  [`docs/adr/`](docs/adr/README.md).
- Compiler structure: [`docs/architecture.md`](docs/architecture.md).
- Test harness: [`docs/testing.md`](docs/testing.md).
- Phases and acceptance criteria: [`docs/roadmap.md`](docs/roadmap.md).

## Definition of Done (every language feature)

1. The spec section is updated.
2. A `run-pass` test exists, plus one `compile-fail` test per rule that can be violated.
3. New diagnostics have a registered code, a clear message and a `compile-fail` case (enforced
   by the test suite); add labels or `help` when they point to the fix.
4. `cargo fmt --check`, clippy with `-D warnings` and `cargo test --workspace` pass on all CI
   platforms.

## Changing the language

Do not change syntax or semantics in code alone. Write a new ADR (superseding the old one if
needed), update the spec, then implement.

## Conventions

- Everything in English: code, comments, diagnostics, commit messages, documentation.
- [Conventional Commits](https://www.conventionalcommits.org/) with the crate as scope, e.g.
  `feat(parser): parse match expressions`, `fix(types): reject implicit i32 to i64`.
  Use `docs`, `ci` or `runtime` as scope for changes outside crates.
- Branches `feat/...` and `fix/...`; pull requests into `main`; CI must be green to merge.

## License

Licensed under either of [MIT](LICENSE-MIT) or [Apache-2.0](LICENSE-APACHE), at your option.
Unless you state otherwise, any contribution you submit is dual-licensed the same way.
