# Lexer corpus

Edge cases of the lexer, valid and invalid. They are not a test suite of their own: the test
`selfhost` (`crates/sagitario_cli/tests/selfhost.rs`) checks that `sagc` lexes them exactly as
`sag` does (docs/adr/0050-lexer-port-and-token-dumps.md). On Windows, git checks the files out
with `\r\n` line endings, which the test covers there.
