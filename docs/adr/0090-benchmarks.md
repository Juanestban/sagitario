# 0090. A suite of benchmarks against other languages, and `std::hint::black_box` (S2)

- Status: Accepted
- Date: 2026-11-11
- Refines: [ADR-0084](0084-a-complete-language-and-the-engine-apart.md) (Q542: the audit of S2) and
  the roadmap of S2, whose acceptance asked for the benchmarks in CI

## Context

One goal of Sagitario is speed close to Rust's. Nothing measures it: the estimates so far (near
Rust in numeric loops, slower where reference counting dominates) are reasoning, not data. The
audit of S2 ([`docs/std-audit.md`](../std-audit.md)) puts a suite of benchmarks third, so that
the work after it is measured, and asks that its results can be watched as a race of the
languages.

A benchmark needs a way to keep the optimizer from deleting the work it measures: Rust's
`std::hint::black_box`. The other languages need their toolchains: rustc, a C compiler, .NET,
Node.js and Python, which are tools of the benchmarks, never of Sagitario. The project adds no
dependency without asking (CLAUDE.md); the user chose that the video uses `ffmpeg` only when it
is installed, and that the benchmarks run locally, not in CI.

## Decision

| # | Topic | Decision |
|---|---|---|
| Q592 | Layout | `benchmarks/<name>/` holds one program per language, the same algorithm with the same input and output: `main.sag`, `main.rs`, `main.c`, `main.cs` (with a `bench.csproj`), `main.js` and `main.py`. Each is idiomatic for its language, single-threaded unless the benchmark is about threads, and without `unsafe` tricks the others cannot use. `benchmarks/README.md` lists them and their rules. |
| Q593 | Programs | **n-body** and **spectral-norm** (numeric loops over structs and arrays), **binary-trees** (allocation and reference counting), **word-count**, **sort** and **map-count** (strings and collections), **parallel-sum** and **channels** (threads, atomics, locks), and **hello** (the start of a program). |
| Q594 | The runner | `tools/bench.sag`, run with `sag run tools/bench.sag -- [names] [--runs N] [--langs sag,rust,...]`. It builds each program optimized (`sag build --release`; `rustc -C opt-level=3`; `clang -O2`, the level `sag` gives its C; `dotnet build -c Release`), runs it once to warm up and `N` times (5 by default), checks that every language prints the same output, and takes the median wall time of the whole process, so the start of a program counts. A language whose toolchain is missing is skipped and reported, never an error. |
| Q595 | Results | Each run writes `benchmarks/results/<date>-<os>-<arch>.json`: per benchmark and language, the times of every run, the median, the size of the program, the versions of the toolchains and the machine. Peak memory is recorded where the system gives it to a parent process (`wait4` on Linux and macOS, `GetProcessMemoryInfo` on Windows), as a later increment. Results are committed by whoever runs them; the reference machine of each one is the one named in its file. |
| Q596 | `std::hint` | A module with `black_box<T>(value: T) -> T`, as Rust's: it returns its argument, and the compiler and the C compiler must assume it was read and may have changed. It is a builtin of the compiler, as `mem::size_of`: the value goes through a local whose address the runtime's core receives in a function of its own translation unit, which C cannot see into without link-time optimization. As in Rust, it is a best effort, not a guarantee. |
| Q597 | The race | `tools/bench.sag --race` writes `benchmarks/results/<file>.html` from a results file: per benchmark, a bar per language that grows during its median time, scaled so the slowest one takes a few seconds, with its time written at the end. It needs no library. With `--video`, if `ffmpeg` is on `PATH`, the tool also writes an MP4 of the same race; without it, it says so and writes the HTML alone. `ffmpeg` is a tool of the race, never of Sagitario or its tests. |
| Q598 | Where they run | Locally, by whoever wants to measure: the benchmarks are not part of CI, since they need five toolchains, take minutes, and shared machines make their times noisy. The harness only checks that the Sagitario programs of `benchmarks/` compile, so they keep up with the language. |
| Q599 | Increments | **1** `std::hint::black_box`, the runner with Sagitario, Rust and C, and n-body, binary-trees and hello. **2** The other benchmarks, and C#, JavaScript and Python. **3** The race in HTML, and the MP4 with `ffmpeg`. **4** Peak memory, and `docs/benchmarks.md` with the first results and what they point at. |
| Q600 | The editor | `std::hint` is a standard library API: the completion of `sag lsp` offers `black_box` after `hint::`, and the editor's README lists it (CLAUDE.md). |

### Example

```sag
use std::hint;
use std::time::Instant;

fn main() {
  const start = Instant::now();
  var sum = 0u64;

  for i in 0..1_000_000u64 {
    sum = sum.wrapping_add(hint::black_box(i) * 3);
  }

  println(f"{sum} in {start.elapsed().as_millis()} ms");
}
```

```sh
sag run tools/bench.sag -- n-body binary-trees --runs 10
sag run tools/bench.sag -- --race benchmarks/results/2026-11-11-windows-x86_64.json --video
```

## Alternatives considered

- **The benchmarks in CI** (a workflow of their own, or every push): the user chose to run them
  locally; CI machines are shared, so their times vary, and installing five toolchains on every
  run is slow. The harness still compiles the Sagitario programs.
- **A video without `ffmpeg`:** encoding MP4 by hand is a project of its own. The HTML race needs
  nothing, and any screen recorder films it; `ffmpeg` only adds the file when present.
- **An existing benchmark framework (hyperfine, Criterion):** they measure one language at a time
  and would be dependencies; the runner is small, and written in Sagitario it exercises the
  language.
- **`black_box` written in Sagitario over a pointer:** a pointer cannot point to every type
  (`String` and classes are ARC references), so a builtin, as `size_of`, takes any `T`.

## Consequences

- Sagitario's speed has numbers, per benchmark and machine, and the work on reference counting,
  atomics and aliasing can show what it changes.
- Results are as good as the machine that produced them; files from different machines are not
  compared with each other.
- The acceptance of S2 changes from "the benchmarks run in CI" to "the benchmarks run, and their
  results are recorded".

## Notes

- Increment 1 is done. `std::hint::black_box` is a builtin of the compiler (`StdItem::BlackBox`,
  `hint::black_box`): SIR stores the value in a local and gives its address to `sag_black_box` of
  the runtime's core, which writes it to a `volatile`; `tests/run-pass/std_hint.sag` tests it with
  numbers, strings, lists and structs under the leak check. `tools/bench.sag` builds, checks and
  times Sagitario, Rust and C, and writes the results file; the size of a program is read with
  `fs::read`, since `std::fs` has no metadata yet. The benchmarks are `n-body`, `binary-trees` and
  `hello`, and `cli::the_benchmarks_compile` checks them and the runner in the harness. Running it
  found a bug of `std::process` on Windows: `CreateProcessW` without an application name finds no
  file in a relative path written with `/`, so `std::sys` now gives it the path of a program that
  has a separator (`application_name`), with `.exe` added when the name alone is not a file, as
  spec 11 promises. A first run on Windows (x86-64, 3 runs): n-body 264 ms in Sagitario, 201 in
  Rust and 262 in C; binary-trees 975, 655 and 552 ms; hello 10.4, 8.2 and 7.9 ms.
- Increment 2 is done: `spectral-norm`, `word-count`, `sort`, `map-count`, `parallel-sum` and
  `channels` join the suite, and C#, JavaScript and Python every benchmark. Two details differ
  from Q592 and Q594: the runner writes the `.csproj` of each C# program in `build/bench/`, for
  the version of the SDK it finds, instead of one per directory of `benchmarks/`; and a warm-up
  longer than 5 seconds is followed by a single timed run, so the interpreters do not take
  minutes. The inputs come from the generator MINSTD, exact in every language, and the outputs
  match byte for byte. The first runs already point at the work: Sagitario is about C's speed in
  numeric loops (`n-body`, `spectral-norm`, `parallel-sum`), but `List::sort` is about 15 times
  slower than Rust's, the map of strings of `word-count` about 7 times and a `Channel` about 13
  times, which increment 4 records with the full results.
- The first bottlenecks, attacked in the standard library before increment 3: `List::sort` was a
  recursive merge sort that made two lists per level and compared through a function value; it
  is now insertion sort in runs of 32 and a bottom-up merge through one buffer, still stable, and
  `sort` compares with `<` (2,000,000 integers: 1245 ms before, 221 ms after, faster than C's
  `qsort` and .NET's sort). `Map` keeps the hash of each entry beside it, so a probe compares keys
  only when the hashes match, masks instead of dividing, reads its lists from its fields once per
  lookup, and its `get` makes no closure; the hash of an integer is inline in the runtime's
  header (`map-count`: 259 ms before, about 185 after). A `Channel` wakes its receivers only when
  one waits (`channels`: 50 ms before, 45 after). What remains of `Map` and `word-count` is the
  reference counting of every read of a list and of a field holding one, which the compiler must
  remove where a value is only lent: the next work on speed, outside the standard library.
