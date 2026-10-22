# 0014. MVP scope: F0 + F1 + F2, desktop only

- Status: Accepted
- Date: 2026-10-21

## Context

The designed language is large. Without a cut, the project never reaches a working
end-to-end state.

## Decision

The MVP is F0 (infrastructure) + F1 (procedural core) + F2 (OOP, generics, closures), with
Windows, Linux and macOS as native hosts. F3 (platform, libraries, mobile), F4 (concurrency)
and F5 (self-hosting) are out of scope. Phase contents and acceptance criteria are in
[roadmap.md](../roadmap.md).

## Alternatives considered

- **F1 only**: does not validate the value proposition (Rust + classes + functional).
- **Including mobile cross-compilation**: mostly toolchain work that the C backend allows
  adding later without compiler changes.
- **Self-hosting early**: every language change would have to be made in two compilers.

## Consequences

- A clear finish line for the first release.
- Mobile and concurrency must stay possible but unimplemented.
