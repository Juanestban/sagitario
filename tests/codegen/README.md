# C generation corpus

Edge cases of the lowering to SIR and of the C generator (F5 increment 6): integers at their limits,
floats at their extremes and negative zero, characters outside ASCII, strings with quotes,
backslashes, trigraphs and control bytes, and strings too long for a C literal (`literals.sag`); and
combinations the lowering handles in separate places: struct values in interface values, conversions
to a parent interface, nested generic instances, closures capturing closures, ranges counting down
by steps and `parse` with `?` (`lowering.sag`) (docs/adr/0054-lowering-and-c-generation-port.md);
and the frame of an `async fn`, with a loop around an `await` (`async_frames.sag`,
docs/adr/0072-async-await-increments.md).
The harness in Sagitario compares the `--dump=c` of each case, with its diagnostics, with
`<case>.expected` next to it (docs/adr/0066-test-harness-in-sagitario.md).
