# Benchmarks: the first results

The suite of [`benchmarks/`](../benchmarks/README.md) runs the same programs in Sagitario, Rust,
C, C#, JavaScript and Python ([ADR-0090](adr/0090-benchmarks.md)). This page records its first
full run, and what the gaps point at. The numbers are those of one machine; another machine's
results go in a file of their own and are not compared with these.

## The machine

- Windows 11, x86-64, AMD Ryzen 7 5700G, 23 GiB of memory.
- `sag 0.31.0` with the official LLVM's clang 23.1; `rustc 1.99.0`; .NET SDK 10.0.201;
  Node.js 26.2.0; Python 3.14.7.
- The results file: [`benchmarks/results/2026-11-12-windows-x86_64.json`](../benchmarks/results/2026-11-12-windows-x86_64.json),
  with its race beside it (`.html` and `.mp4`).

## The results

The median wall time of five runs after a warm-up, the start of each process included, and the
peak memory of one more run. The time in parentheses is the ratio to the fastest language.

| Benchmark | Sagitario | Rust | C | C# | JavaScript | Python |
|---|---|---|---|---|---|---|
| n-body | 117 ms (1.33) | 88 ms (1.00) | 119 ms (1.35) | 198 ms (2.25) | 240 ms (2.73) | 11.5 s (131) |
| spectral-norm | 52 ms (1.00) | 55 ms (1.05) | 55 ms (1.05) | 118 ms (2.25) | 117 ms (2.24) | 6.95 s (133) |
| binary-trees | 1169 ms (4.64) | 745 ms (2.96) | 621 ms (2.47) | 347 ms (1.38) | 252 ms (1.00) | 3.40 s (13.5) |
| word-count | 446 ms (6.83) | 65 ms (1.00) | 110 ms (1.68) | 222 ms (3.40) | 397 ms (6.08) | 430 ms (6.58) |
| sort | 207 ms (2.71) | 76 ms (1.00) | 266 ms (3.48) | 324 ms (4.24) | 1210 ms (15.8) | 1198 ms (15.7) |
| map-count | 143 ms (4.50) | 49 ms (1.53) | 32 ms (1.00) | 313 ms (9.83) | 248 ms (7.78) | 625 ms (19.7) |
| parallel-sum | 18 ms (1.01) | 19 ms (1.04) | 18 ms (1.00) | 85 ms (4.75) | 270 ms (15.1) | 2.86 s (160) |
| channels | 44 ms (3.26) | 14 ms (1.00) | 19 ms (1.37) | 147 ms (10.7) | 852 ms (62.5) | 587 ms (43.0) |
| hello | 8.4 ms (1.09) | 7.7 ms (1.00) | 10.0 ms (1.29) | 63 ms (8.19) | 64 ms (8.37) | 32 ms (4.19) |

Peak memory, in MiB:

| Benchmark | Sagitario | Rust | C | C# | JavaScript | Python |
|---|---|---|---|---|---|---|
| n-body | 4.4 | 4.7 | 4.3 | 23.5 | 41.4 | 12.0 |
| spectral-norm | 4.4 | 4.7 | 4.3 | 23.2 | 41.5 | 11.9 |
| binary-trees | 24.3 | 12.5 | 12.1 | 49.8 | 88.5 | 23.7 |
| word-count | 40.1 | 13.4 | 12.9 | 90.9 | 165.2 | 128.7 |
| sort | 35.2 | 28.2 | 19.1 | 57.1 | 139.7 | 99.6 |
| map-count | 13.1 | 8.5 | 10.9 | 32.6 | 54.6 | 21.7 |
| parallel-sum | 4.4 | 4.8 | 4.3 | 23.1 | 93.0 | 12.0 |
| channels | 8.5 | 5.3 | 8.9 | 27.4 | 157.4 | 17.1 |
| hello | 4.3 | 4.7 | 4.3 | 22.8 | 35.2 | 11.4 |

The machine was not quiet: run again as the least of nine runs, Sagitario's `word-count` takes
271 ms and its `binary-trees` 963 ms, against Rust's 63 and 659. Medians of five are what the
file keeps; differences under about 10 % between two languages are within the noise here.

## What they say

- **Code without allocation is at the speed of C and Rust.** `n-body`, `spectral-norm` and
  `parallel-sum` are within the noise of C, and so is the start of a program (`hello`), with the
  same memory. The goal of S2 for such code is met.
- **Counting references is the gap.** `binary-trees` and `map-count` are 1.5 to 4.5 times
  slower than Rust, and `word-count` 4 to 7 times: every copy of a reference to a class, a list
  or a string is a count, where Rust counts nothing. The first pass that removes counts
  ([ADR-0091](adr/0091-fewer-reference-counts.md), increment 1) took 2 to 17 % off; the lent
  reads of its increment 2 are the larger part that remains.
- **Objects are larger.** `binary-trees` uses twice Rust's memory: each object carries a
  header of 32 bytes (its strong and weak counts, its drop function and its vtable) besides its
  fields. `word-count` and `sort` keep more memory too, since strings and lists are objects of
  their own.
- **`sort` waits for `restrict`.** Its merge reloads the length and the data of its lists after
  every store, since a `u64` element and the length of a list have the same C type; Rust's
  `noalias` is what removes it.
- **Channels** are three times slower than Rust's: a lock and a condition variable around two
  lists, where Rust's channel is lock-free.
- **Against the managed languages,** Sagitario is faster than C#, JavaScript and Python
  everywhere except `binary-trees`, where a tracing collector that allocates by bumping a
  pointer beats counting, at three to seven times the memory.

## The work they point at

In the order of what they would gain, each a record of its own:

1. Lent reads without counts ([ADR-0091](adr/0091-fewer-reference-counts.md), increment 2).
2. In programs that use threads, counts that are not atomic for values that never cross them.
3. `restrict` for `&mut` parameters and slices, and for the buffers of lists in the standard
   library.
4. Smaller headers of objects, and closures that never escape kept off the heap.
5. A channel without a lock.
