# Benchmarks

The same programs in Sagitario and other languages, to measure how far Sagitario is from them
and where ([ADR-0090](../docs/adr/0090-benchmarks.md)). They run locally, never in CI; the
harness only checks that the Sagitario programs compile.

```sh
sag run tools/bench.sag                              # every benchmark, 5 runs each
sag run tools/bench.sag -- n-body binary-trees --runs 10
sag run tools/bench.sag -- --langs sag,rust          # some languages
SAG=build/bin/sag SAG_CC=clang sag run tools/bench.sag   # the sag and C compiler to use
```

Each program is built optimized (`sag build --release`, `rustc -C opt-level=3`, `clang -O2`, the
level `sag` gives its C), run once to warm up and then `--runs` times; every language must print
the same output. The runner prints the median wall time of the whole process, so the start of a
program counts, its ratio to the fastest, and the size of the program, and writes them with the
versions of the toolchains to `results/<date>-<os>-<arch>.json`. A language whose toolchain is
not installed is skipped. Results from different machines are not compared with each other.

| Benchmark | Measures | Argument |
|---|---|---|
| `n-body` | Numeric loops over an array of structs | steps |
| `binary-trees` | Allocation and reference counting | depth of the largest tree |
| `hello` | The start of a program | — |

Each directory holds `main.sag`, `main.rs` and `main.c`, and `args`, the arguments every
language gets. The rules: the same algorithm with the same input and output, idiomatic code for
each language, one thread unless the benchmark is about threads, and no `unsafe` tricks the
others cannot use. C#, JavaScript and Python, the other benchmarks and the race of the languages
come in the next increments of ADR-0090.
