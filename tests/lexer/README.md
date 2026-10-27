# Lexer corpus

Edge cases of the lexer, valid and invalid. They are not a test suite of their own: the test
`selfhost` (`crates/sagitario_cli/tests/selfhost.rs`) checks that `sagc` lexes them exactly as `sag`
does (docs/adr/0050-lexer-port-and-token-dumps.md), and the harness in Sagitario compares the
`--dump=tokens` of each case, with its diagnostics, with `<case>.expected` next to it
(docs/adr/0066-test-harness-in-sagitario.md). The files keep `\n` line endings on every system, and
`crlf.sag` keeps `\r\n` (`.gitattributes`).
