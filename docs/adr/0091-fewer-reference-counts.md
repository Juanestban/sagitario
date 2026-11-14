# 0091. Fewer reference counts: an optimization pass over SIR (S2)

- Status: Accepted
- Date: 2026-11-12
- Refines: [ADR-0090](0090-benchmarks.md) (the work the benchmarks point at) and the roadmap of S2
  ("retains and releases removed when a value moves or is lent")

## Context

The benchmarks of ADR-0090 put Sagitario near C and Rust in numeric loops, threads and the start
of a program, but 3 to 5 times slower than Rust where programs copy references: `binary-trees`,
`word-count`, `map-count`. Rust counts nothing there, since ownership is known at compile time;
Sagitario counts every copy of a class instance, a list, a string or a closure (spec 3, ARC).

The C the compiler generates counts more than ARC needs. Reading a field into a binding,
`const slots = self.slots;`, becomes three operations: the field read retains a temporary (a
field may change during the statement), the binding retains its copy, and the end of the
statement releases the temporary. Every read of a list element of a managed type, and every
field read used only to call a method or index a list, retains and later releases too. Swift
removes such pairs with an optimization pass over its intermediate code; the inline counts of
ADR-0090 made each operation cheaper, but not fewer.

Spec 3 says that an object is destroyed when its count reaches zero, not when a binding's scope
ends. A `weak` field, though, shows when an object is destroyed: a pass that released a value
earlier than today could make a weak reference become `None` sooner. This record only removes
counts that cannot change when any object is destroyed.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q601 | The pass | A pass over the body of each function in SIR, after lowering and before C (`sir/arc.sag`), removes retains and releases that do not change any count at the moments an object could be destroyed or a weak reference read. It runs in debug and release builds alike, so both behave the same. |
| Q602 | Moves of temporaries | A temporary that holds a new reference (`owned_copy`, the result of a call, `owned_result`) and is retained into a binding, a field or an element, then released without any other use, gives its reference instead: the retain and the release go. This is `const x = self.f;` in one count instead of three. |
| Q603 | Adjacent pairs | A retain of a value followed by a release of the same value, with only statements between them that neither call a function nor store to a place, cancels out: no object can be destroyed in between. |
| Q604 | Lent reads | A read of a field of a class instance, or of an element of a list, that only lends its value until the end of the statement (the receiver of a method, a list indexed, an argument, an operand) needs no copy when nothing between the read and its last use in the statement can release it: no call of a function other than the runtime's reads of lists and strings, and no store to a place. The read is then the place itself, as reads of `const` bindings already are. |
| Q605 | What stays | A release of the last reference keeps its place in the statement: no object is destroyed earlier or later than without the pass, and `weak` references observe the same. Releasing a binding after its last use rather than at the end of its scope, as Swift does, would destroy objects earlier; it is left for a record of its own, which must say what a `weak` reference may observe. |
| Q606 | Checking it | The pass only removes operations. The harness runs every suite with the sanitizers and the leak check, as today, and a test compares the counts of a program built with and without the pass (`SAG_NO_ARC_PASS`, an environment variable of the compiler for tests only). The dumps of `tests/codegen` show the C after the pass. |
| Q607 | Increments | **1** Moves of temporaries and adjacent pairs (Q602, Q603). **2** Lent reads (Q604), with the effects of runtime functions known to the pass. **3** The benchmarks measured again, and the results in ADR-0090. |

### Example

```sag
fn slot_of(self, key: K, hash: u64) -> Option<usize> {
  const slots = self.slots;   // today: retain, retain, release; with Q602: one retain
  ...
  if hashes[index] == hash {  // `hashes` is lent to the index: no count (Q604)
```

## Alternatives considered

- **Releasing at last use (Swift's lifetimes):** the largest saving, and Rust's drops at the end
  of a scope are no guide to it, but it changes when objects are destroyed and what `weak`
  references see; it needs its own record and a change of spec 3.
- **Borrowing everywhere, with a checker as Rust's:** the language would change; references are
  only parameters (ADR-0079).
- **Making counts cheaper only (inline, biased or deferred counting):** the inline counts are done
  (ADR-0090); deferred counting would delay destruction, against spec 3's determinism.

## Consequences

- Programs that copy references count less, with the same semantics; the benchmarks measure how
  much.
- The generated C changes, and with it the dumps of `tests/codegen`.
- A fault in the pass would free an object in use, or leak one: the sanitizers, the leak check
  and the comparison of Q606 are what catch it, and the environment variable turns the pass off
  to tell a fault of the pass from one elsewhere.

## Notes

- Increment 1 is done. `sir/arc.sag` runs on the body of every function that is not a template,
  inner bodies first, and only deletes statements. A move (Q602) is a `Retain(t)` followed, past
  the markers of lines, by `t` stored into another binding or into a place that does not read
  `t`, and later a `Release(t)` with no mention of `t` between them; a pair (Q603) is a
  `Retain(x)` and a `Release(x)` with only markers of lines between them, which is narrower than
  Q603 allows and leaves calls and stores to increment 2. A statement the pass does not know
  counts as mentioning every local, so it never moves past one. `SAG_NO_ARC_PASS` turns the pass
  off; `cli::the_arc_pass_keeps_what_programs_do` runs five `run-pass` programs built with and
  without it, under the sanitizers and the leak check, and checks that they print the same and
  that the C has no more counts with it. The compiler built with the pass reaches the fixed point
  of the bootstrap. The C of `word-count` goes from 206 lines that count to 148; the times
  improve less, since most counts left are the lent reads of Q604 (Windows, x86-64, release, the
  least of 15 alternated runs, without and with the pass): binary-trees 1032 and 978 ms,
  map-count 240 and 205 ms, word-count 282 and 275 ms, sort 199 and 196 ms.
