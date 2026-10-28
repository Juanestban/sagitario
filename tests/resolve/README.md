# Resolver corpus

Edge cases of name resolution: scopes, captures and patterns (`scopes.sag`), unresolved names and
their suggestions (`suggestions.sag`), and a program of several modules with imports, visibility and
warnings (`program/`) (docs/adr/0052-resolver-port-and-resolution-dumps.md). The harness in
Sagitario compares the `--dump=resolve` of each case, with its diagnostics, with `<case>.expected`
next to it (docs/adr/0066-test-harness-in-sagitario.md).
