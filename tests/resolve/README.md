# Resolver corpus

Edge cases of name resolution: scopes, captures and patterns (`scopes.sag`), unresolved names and
their suggestions (`suggestions.sag`), and a program of several modules with imports, visibility
and warnings (`program/`). They are not a test suite of their own: the test `selfhost`
(`crates/sagitario_cli/tests/selfhost.rs`) checks that `sagc` resolves them, and every other
`.sag` file of the repository, exactly as `sag` does
(docs/adr/0052-resolver-port-and-resolution-dumps.md).
