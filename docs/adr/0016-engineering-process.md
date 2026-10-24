# 0016. Testing, quality gates, conventions and license

- Status: Accepted; refined by [ADR-0036](0036-sanitizers-and-the-reference-zoo.md)
- Date: 2026-10-21

## Context

A compiler must be trustworthy: it must reject invalid programs with clear errors, never
crash on user input, and produce code that does not leak memory.

## Decision

- Unit tests per crate with snapshots (`insta`), plus end-to-end suites `run-pass`,
  `compile-fail` and `run-panic` driven by an own harness. Details in
  [testing.md](../testing.md).
- ARC leak detection: the test runtime counts live objects; ASan/UBSan where supported.
- The compiler never panics on user input. Clippy denies `unwrap_used`, `expect_used`,
  `panic`, `indexing_slicing` and `todo` outside tests.
- CI (GitHub Actions) on Windows, Linux and macOS, plus a gcc job on Linux.
- A Definition of Done for every language feature (in [CONTRIBUTING.md](../../CONTRIBUTING.md)).
- Code, diagnostics, commits and documentation in English.
- License `MIT OR Apache-2.0`. Conventional Commits with the crate as scope.

## Alternatives considered

- **Only unit tests**: cannot validate the full source-to-executable pipeline.
- **Spanish documentation**: the intended audience is international.

## Consequences

- Every feature ships with spec text and tests.
- Internal compiler errors become regression tests.
