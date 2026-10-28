# 0062. The language server on the standard library and on lone files

- Status: Accepted
- Date: 2026-10-26
- Refines: [ADR-0058](0058-language-server.md)

## Context

Opening the sources of the standard library (`std/io.sag`, `prelude.sag`) showed errors that are
not errors: the server checked them as programs of their own, where declaring `sag_*` functions
of the runtime, `super::` up to the file of `std` and `std::prelude` are not allowed. And every
file analysed alone, such as a module whose program has no `Nova.toml` or `main.sag` next to it,
reported a missing `main`, which matters when running a program, not while editing a module.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q338 | Sources of the standard library | A file is a source of `std` when it is `prelude.sag` next to a directory `std`, or `std/<module>.sag` (or `std/std.sag`) next to a `prelude.sag`. The server checks it as the standard library it is: it analyses a program it writes, which uses that module, with the open document in place of the embedded source (`Loader::std_overrides`), and reports the diagnostics of that source on the document. Hover and definitions work inside it. |
| Q339 | Files alone | A file the server analyses alone is a library, which needs no `main`; `sag run` and `sag build` still report a missing `main`. |

## Consequences

- The program the server writes for a source of `std` is not a file and gets no diagnostics.
- Going to the definition of an item of `std` from a program still has no file to open: the
  compiler embeds the standard library and does not know where its sources are.
