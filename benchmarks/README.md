# Benchmarks

The same programs in Sagitario, Rust, C, C#, JavaScript and Python, to measure how far Sagitario
is from each and where ([ADR-0090](../docs/adr/0090-benchmarks.md)). They run locally, never in
CI; the harness only checks that the Sagitario programs compile.

```sh
sag run tools/bench.sag                              # every benchmark, 5 runs each
sag run tools/bench.sag -- n-body binary-trees --runs 10
sag run tools/bench.sag -- --langs sag,rust,c        # some languages: sag rust c cs js py
SAG=build/bin/sag SAG_CC=clang sag run tools/bench.sag   # the sag and C compiler to use
```

Each program is built optimized (`sag build --release`; `rustc -C opt-level=3`; `clang -O2`, the
level `sag` gives its C; `dotnet build -c Release`, from a project the runner writes in
`build/bench/`), or run by its interpreter (`node`, `python3` or `python`). It runs once to warm up
and then `--runs` times, or once more when the warm-up takes over 5 seconds, so a slow
interpreter does not take minutes; every language must print the same output. The runner prints
the median wall time of the whole process, so the start of a program counts, its ratio to the
fastest, the size of the program and its peak memory, and writes them with the versions of the
toolchains to `results/<date>-<os>-<arch>.json`. Peak memory comes from one more run, with its
output thrown away, that the runner starts through the system itself, since `std::process` does
not give a child's process: the peak working set on Windows (`K32GetProcessMemoryInfo`), the
largest resident set on Linux and macOS (`wait4`). An interpreter's memory includes the
interpreter's own. A language whose toolchain is not installed is skipped.
Results from different machines are not compared with each other.

```sh
sag run tools/bench.sag -- --race benchmarks/results/<file>.json           # the race, as HTML
sag run tools/bench.sag -- --race benchmarks/results/<file>.json --video   # and as an MP4
```

`--race` turns a results file into a race of the languages, `<file>.html` beside it: a page with
no dependency, one heat per benchmark, where each language's bar grows for a time in proportion to
its median, the fastest in one second, and stops at eight seconds for the slowest. `--video` also
writes `<file>.mp4` with `ffmpeg`, if it is on `PATH` (H.264, or MPEG-4 when `ffmpeg` has no
H.264), drawing its frames in `build/bench/race/`; without `ffmpeg`, it says so and writes the
page alone. `ffmpeg` is a tool of the race only, never of Sagitario or its tests.

| Benchmark | Measures | Argument |
|---|---|---|
| `n-body` | Numeric loops over an array of structs | 2000000 steps |
| `spectral-norm` | Numeric loops over lists, and a function called in them | size 1000 |
| `binary-trees` | Allocation and reference counting (or garbage collection) | depth 16 |
| `word-count` | Building strings, splitting them, a map of strings | 1000000 words |
| `sort` | The standard sort of a list of integers | 2000000 integers |
| `map-count` | A hash map of integers | 2000000 keys |
| `parallel-sum` | Four threads and their joins | `n` = 40000000 |
| `channels` | A channel between two threads | 500000 values |
| `hello` | The start of a program | — |

Each directory holds `main.sag`, `main.rs`, `main.c`, `main.cs`, `main.js`, `main.py` and `args`,
the arguments every language gets. The inputs come from the generator MINSTD
(`x = x * 48271 % 2147483647`), whose products are exact even in JavaScript's numbers. The rules:
the same algorithm with the same input and output, idiomatic code with the standard library of
each language, one thread unless the benchmark is about threads, and no `unsafe` tricks the
others cannot use. The C has no hash map or channel of its own library, so `word-count`,
`map-count` and `channels` write a small one; JavaScript's threads are `worker_threads`, and
CPython's run Python one at a time.
