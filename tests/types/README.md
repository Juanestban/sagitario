# Type checker corpus

Edge cases of the type checker. For its declarations (F5 increment 5a): constants at the limits of
every integer type, float constants and casts (`constants.sag`), classes, inheritance and vtables
(`classes.sag`), interfaces, generics and their bounds (`interfaces.sag`), and the errors of
declarations (`declaration_errors.sag`). They are not a test suite of their own: the test
`selfhost` (`crates/sagitario_cli/tests/selfhost.rs`) checks that `sagc` types them, and every
other `.sag` file of the repository, exactly as `sag` does
(docs/adr/0053-type-checker-port-and-type-dumps.md).
