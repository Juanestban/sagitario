# C generation corpus

Edge cases of the lowering to SIR and of the C generator (F5 increment 6): integers at their
limits, floats at their extremes and negative zero, characters outside ASCII, strings with
quotes, backslashes, trigraphs and control bytes, and strings too long for a C literal
(`literals.sag`); and combinations the lowering handles in separate places: struct values in
interface values, conversions to a parent interface, nested generic instances, closures capturing
closures, ranges counting down by steps and `parse` with `?` (`lowering.sag`). They are not a test
suite of their own: the test `selfhost` (`crates/sagitario_cli/tests/selfhost.rs`) checks that
`sagc` generates the same C for them, and for every other `.sag` file of the repository, as `sag`
does (docs/adr/0054-lowering-and-c-generation-port.md).
