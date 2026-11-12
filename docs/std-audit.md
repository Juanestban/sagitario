# Audit of the standard library against Rust's (S2)

S2 makes the standard library complete for a general-purpose language, with Rust's std as the
reference ([roadmap v2](roadmap-v2.md#s2-a-complete-standard-library),
[ADR-0084](adr/0084-a-complete-language-and-the-engine-apart.md), Q542). This audit compares
each module of Rust's std with what Sagitario has: what exists, what is missing, and what is out
of scope, with the reason. Each gap is filled by a record of its own; the rows say which.

The state is that of 2026-11-11, after S1: every module of `std` is written in Sagitario over the
system API (`std::sys`), and the language has F1–F6, G1 and the records up to ADR-0087.

**Legend.** ✅ done · 🟡 partial · ❌ missing · ➖ not needed: Sagitario covers it another way ·
⛔ out of scope, with its reason.

## How Sagitario differs from Rust

Some of Rust's std exists to manage ownership, which Sagitario does differently, so those rows
are ➖ rather than gaps:

- Classes are reference types counted by ARC, atomically when the program uses threads: they are
  Rust's `Box`, `Rc` and `Arc` at once. `weak` fields are `Weak`.
- A `var` field of a class can change through any reference, as Rust's `Cell` and `RefCell`
  allow; `Send` is inferred, and sharing a value between threads is capturing it.
- References (`&T`, `&mut T`, `&[T]`) are only the types of parameters, with a borrow checker but
  without lifetimes (ADR-0079): no type can hold a borrow, which rules out `Cow<'a, T>`, lock
  guards and scoped threads in their Rust form.
- There is no unwinding: a panic ends the program (exit code 101).

## Gaps found in the language

The audit of the library found gaps of the language itself, which the library needs first:

| Gap | Effect on the library | Record |
|---|---|---|
| No bitwise operators (`&`, `\|`, `^`, `!` on integers, `<<`, `>>`) | Hashing, flags, bit sets, UTF-8 and parsing are written with `*`, `/` and `%`, slower and harder to read; `BitAnd`... cannot be implemented | ✅ [ADR-0088](adr/0088-bitwise-operators-and-integer-methods.md), increment 1; the interfaces `BitAnd`... are still missing |
| `wrapping_*`, `checked_*` and `saturating_*` are promised by spec 2 ("Integer arithmetic") but not implemented | Code that must not panic on overflow in debug builds cannot say so | ✅ [ADR-0088](adr/0088-bitwise-operators-and-integer-methods.md), increment 2 |
| No `Display`: user types cannot be interpolated or turned into text (spec 2, "post-MVP") | No `to_string()`, no formatting of user types | ✅ [ADR-0089](adr/0089-display-debug-and-format-specifiers.md), increment 1 |
| No tuples (spec 2: "may be added later") | `enumerate`, `zip`, `unzip`, `partition`, `split_once` and `char_indices` cannot have Rust's signatures | ✅ [ADR-0092](adr/0092-tuples.md), increments 1 and 2; the methods returning tuples come with record 4 |
| No conversion interfaces (`From`, `Into`, `TryFrom`) nor `Default` | Conversions are named functions; `?` converts errors only to `Error` | ❌ a record of its own |
| `@cfg` is not allowed on fields | Classes of `std::sys` that differ by system are written twice whole | 🟡 a small record, when it hurts |

## Modules

### Collections (`vec`, `collections`, `slice`, `array`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Vec<T>` | `List<T>` | 🟡 | Has `push`, `pop`, `get`, `first`, `last`, `insert`, `remove`, `clear`, `reverse`, `contains`, `index_of`, `sort`, `sort_by`, `join`, `resize`, `iter`. Missing: `extend`, `append`, `truncate`, `retain`, `dedup`, `swap`, `split_off`, `drain`, `binary_search`, `sort_by_key`, `sort_unstable`, `with_capacity`/`reserve`, `windows`, `chunks`, `concat`, `fill`, `iter().rev()`, `min`/`max`, equality of lists |
| `[T; N]` | arrays | 🟡 | Values with `len`, indexing, `for`, `iter()` and the iterator methods, `as_ptr`. Missing: `contains`, `sort`, `==`, and the rest of the methods of lists that do not change the length |
| `&[T]`, `&mut [T]` | slices | 🟡 | Views of arrays and lists, only as parameters, with `len`, `is_empty`, indexing, `for` and `iter()`. Missing: `contains`, `sort`, `binary_search`, `split_at`, `windows`, `chunks`, `copy_from_slice`, `fill` |
| `HashMap<K, V>` | `Map<K, V>` | 🟡 | Keeps insertion order (like `IndexMap`). Missing: `entry` (insert-or-update), `get_mut`-style update (`update(k, f)`), `retain`, `extend`, `from` a list of pairs, equality |
| `HashSet<T>` | — | ❌ | A `Set<T: Hash>` class in the prelude, on `Map` |
| `BTreeMap`, `BTreeSet` | — | ❌ | Ordered by `Ord`, with ranges |
| `VecDeque<T>` | — | ❌ | A ring buffer; `Channel` keeps two lists for want of it |
| `BinaryHeap<T>` | — | ❌ | A priority queue on `Ord` |
| `LinkedList<T>` | — | ⛔ | Rarely the right structure; Rust itself discourages it |

### Strings and characters (`string`, `str`, `char`, `ascii`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `String`, `&str` | `String` (immutable, ARC; `trim` and `substring` share the buffer) | 🟡 | Has `len`, `is_empty`, `contains`, `starts_with`, `ends_with`, `find`, the `trim`s, ASCII case, `replace`, `repeat`, `substring`, `split`, `lines`, `chars`, `bytes`, `parse_*`. Missing: `rfind`, `split_whitespace`, `splitn`, `rsplit`, `split_once`, `strip_prefix`, `strip_suffix`, `char_indices`, `to_uppercase`/`to_lowercase` of Unicode, `matches`, `replacen`, `pad`, `char_count`, `from_utf8` (public; `std` has it privately), `from_utf8_lossy`, `from_utf16` |
| `String` (growable) | `StringBuilder` | 🟡 | Missing: `insert`, `truncate`, `pop`, `capacity` |
| `char` | `char` | 🟡 | Has ASCII classification, `to_digit`, `char::from_u32`. Missing: Unicode `is_alphabetic`, `is_numeric`, `is_whitespace`, `is_uppercase`, `to_uppercase`, `len_utf8`, `char::from_digit` |
| `format!`, `fmt` | format strings `f"{expr}"` | 🟡 | `Display`, the specifiers (width, alignment, precision, radix) and the debug form `{:?}`, `{:#?}` are done (ADR-0089). Missing: widths and precisions given at run time (`{:>width$}`), and the debug form of an interface value (`dyn Name` for now) |

### Numbers (`num`, `f32`, `f64`, integer modules)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| Arithmetic, casts | ✅ | ✅ | Overflow panics in debug, wraps in release |
| `f64::consts`, float methods | `std::math`, methods of numbers | ✅ | G1 |
| `checked_*`, `wrapping_*`, `saturating_*` | methods of integers | ✅ | ADR-0088; `overflowing_*` is missing |
| `MIN`, `MAX` of integers | `i32::MAX`... | ✅ | ADR-0088 |
| `count_ones`, `leading_zeros`, `rotate_left`, `swap_bytes`, `to_le_bytes` | methods of integers | 🟡 | ADR-0088; `to_le_bytes`, `from_le_bytes` and the other conversions to bytes are missing |
| `from_str_radix`, formatting in hex | — | ❌ | With `fmt` |
| `NonZero*` | — | ⛔ | A layout optimization of `Option` that Sagitario's ARC model does not use |

### Iterators (`iter`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Iterator` | `Iter<T>`, a class over `fn() -> Option<T>` | 🟡 | Has `map`, `filter`, `take`, `skip`, `step_by`, `collect`, `count`, `fold`, `for_each`, `any`, `all`, `find`, and `(a..b).rev()` in `for`. Missing: `enumerate`, `zip`, `chain`, `rev` of any iterator, `flat_map`, `flatten`, `take_while`, `skip_while`, `peekable`, `sum`, `product`, `min`, `max`, `min_by`, `position`, `last`, `nth`, `inspect`, `partition`, `unzip`, `cycle`, `scan`; `collect` into a `Map` or a `String` |
| `IntoIterator` for user types | — | ❌ | `for` over a user collection needs `.iter()` |

### Options, results and errors (`option`, `result`, `error`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Option`, `Result` | ✅ | ✅ | Rust's methods, `Eq`, `Ord` and `Hash` ([ADR-0093](adr/0093-everyday-methods.md), increment 1). `take` and `replace` are `mem::replace` (➖); `unwrap_or_default` waits for `Default` (record 5) |
| `std::error::Error` | the prelude's `Error` interface | 🟡 | Missing: `source()` chains and backtraces (spec 7, post-MVP) |
| `panic!`, `assert!`, `unreachable!`, `todo!` | `panic`, `assert`, `assert_eq`, `assert_ne`, `unreachable`, `todo` | ✅ | Functions that panic at the line of their call, without a message argument (ADR-0093, increment 1) |

### Comparison, hashing, operators, conversion (`cmp`, `hash`, `ops`, `convert`, `default`, `marker`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `PartialEq`, `Eq`, `PartialOrd`, `Ord`, `Ordering` | `Eq`, `Ord`, `Ordering` | ✅ | ADR-0081; floats have `total_compare` |
| `cmp::min`, `max`, `Reverse`, `then_with` | — | 🟡 | Numbers have `min`/`max`; generic `min`/`max` and `Ordering::then` are missing |
| `Hash`, `Hasher` | `Hash` | 🟡 | Types implement `hash`; a `Hasher` to combine fields, and a choice of hash function, are missing |
| `Add`, `Sub`, `Mul`, `Div`, `Rem`, `Neg` | ✅ | ✅ | ADR-0081 |
| `Index`, `IndexMut`, `Bit*`, `Shl`, `Shr`, `*Assign` | — | ❌ | `Index` for user collections; the bitwise ones after the operators exist |
| `Range`, `RangeInclusive` | `a..b`, `a..=b` | 🟡 | In `for` and slices; not values with methods (`contains`, `len`) |
| `From`, `Into`, `TryFrom`, `AsRef` | — | ❌ | See the language gaps |
| `Default` | — | ❌ | See the language gaps |
| `Clone`, `Copy` | ➖ | ➖ | Structs copy, classes share |
| `Send`, `Sync` | `Send` | ➖ | Inferred; sharing is capturing a `Send` value |
| `Drop` | — | ⛔ for now | No destructors; the runtime frees with ARC. A `Drop` for resources (files, sockets) would be a record of its own |

### Memory and pointers (`mem`, `ptr`, `boxed`, `rc`, `cell`, `borrow`, `alloc`, `pin`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `mem::size_of`, `align_of`, `zeroed` | `std::mem` | ✅ | G1 |
| `mem::swap`, `replace`, `take` | — | ❌ | Useful with `var` fields |
| `ptr::null`, `read`, `write`, `copy`, `copy_nonoverlapping` | `0 as *const T`, `*p`, `memcpy` through the FFI | 🟡 | Named functions for raw memory are missing |
| `Box`, `Rc`, `Arc`, `Weak` | classes, `weak` | ➖ | See above |
| `Cell`, `RefCell` | `var` fields of classes | ➖ | |
| `Cow` | — | ⛔ | Needs references held in a type, which Sagitario does not have |
| `alloc`, custom allocators | — | ⛔ | The runtime's allocator; S3 decides whether to expose one |
| `Pin` | — | ⛔ | Instances of classes never move; `async` frames are classes |

### Concurrency (`sync`, `thread`, `future`, `task`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Mutex` | `Mutex<T: Send>` | 🟡 | `get`, `set`, `update(f)`, no guards; `try_lock` is missing |
| `RwLock` | — | ❌ | `read(f)` and `update(f)` in the style of `Mutex` |
| `Condvar` | — | ❌ | `std::sys::Lock` has one; a public API waits for a design without guards |
| `atomic::*` | six atomics | 🟡 | Sequentially consistent only (ADR-0087); missing: orderings, `AtomicI8`/`U8`/`I16`/`U16`/`Isize`/`Ptr`, `fetch_max`/`min`/`and`/`or` |
| `Once`, `OnceLock`, `LazyLock` | `static` | ➖ | A `static` is computed once, on its first use, by one thread (ADR-0083) |
| `Barrier` | — | ❌ | |
| `mpsc::channel`, `sync_channel` | `Channel<T: Send>` | 🟡 | Missing: a bounded channel, `try_receive`, `receive_timeout` |
| `thread::spawn`, `join`, `sleep`, `available_parallelism` | ✅ | ✅ | |
| `thread::Builder` (name, stack size), `current`, `ThreadId`, `park`, `yield_now` | — | ❌ | |
| `thread::scope` | — | ⛔ for now | Scoped threads borrow the stack, which needs references held by closures |
| `thread_local!` | `@thread_local static var` | ✅ | ADR-0083 |
| `Future`, `task` | `async fn`, `std::task` | ✅ | F8; a different model from Rust's (a single-threaded executor, `spawn_blocking`) |

### Input and output (`io`, `fs`, `path`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `stdin`, `stdout`, `stderr`, `print!` | `std::io` | 🟡 | Lines, whole input, exact bytes, terminals. Missing: reading bytes (`List<u8>`) from stdin, writing bytes, locking for a series of writes |
| `Read`, `Write`, `BufRead`, `BufReader`, `BufWriter` | — | ❌ | Interfaces for streams, so files, sockets and processes share code |
| `fs::read`, `write`, `read_to_string`, `create_dir(_all)`, `remove_*`, `rename`, `copy`, `read_dir` | ✅ | ✅ | |
| `File` (open, read in parts, seek, append) | — | ❌ | Whole files only; large files need a handle. Needs `Drop` or an explicit `close` |
| `fs::metadata` (size, times, permissions), `remove_dir_all`, `canonicalize`, `symlink` | — | ❌ | Spec 11 lists them as "not yet" |
| `env::set_current_dir` | — | ❌ | Process-wide state |
| `Path`, `PathBuf` | `std::path` over `String` | 🟡 | `join`, `file_name`, `parent`, `extension`, `stem`. Missing: `components`, `is_absolute`, `with_extension`, `strip_prefix`, normalization |

### Networking (`net`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `TcpListener`, `TcpStream`, `UdpSocket`, `IpAddr`, `SocketAddr`, `ToSocketAddrs` | — | ❌ | A record of its own: sockets of each system in `std::sys`, on the `Read` and `Write` interfaces. TLS and HTTP are Nova packages, as in Rust |

### Processes and environment (`process`, `env`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Command` with `output`, `status` | `Command` | 🟡 | Arguments, variables, directory, input. Missing: `spawn` and a `Child` to `wait`, `kill` or stream from; redirecting to files; `env_remove`, `env_clear` (spec 11, "not yet") |
| `process::exit`, `abort`, `id` | `exit` | 🟡 | `abort` and `id` are missing |
| `env::args`, `var` | `args`, `variable` | ✅ | |
| `env::vars`, `set_var`, `remove_var`, `current_exe`, `temp_dir`, `home_dir` | — | ❌ | `set_var` is process-wide state, as in Rust (`unsafe` there since 2024) |
| `env::consts::OS`, `ARCH` | `@cfg` | 🟡 | Known at compile time; constants for code that prints them are missing |

### Time (`time`)

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `Duration`, `Instant`, `SystemTime`, `sleep` | ✅ | ✅ | Arithmetic is named methods (`plus`, `minus`, `times`) |
| `checked_add`, `saturating_sub`, `Duration * f64`, `Display` | — | ❌ | With the numeric helpers and `fmt` |
| Calendars, time zones | — | ⛔ | A Nova package, as `chrono` is for Rust |

### Other modules

| Rust | Sagitario | State | Notes |
|---|---|---|---|
| `ffi` (`CString`, `CStr`, C types, `c_void`) | `std::ffi`, `Library` | ✅ | ADR-0078; `OsString` is ⛔: paths and arguments are UTF-8 `String`s, UTF-16 on Windows inside `std::sys` |
| `os` (`unix::fs::PermissionsExt`, `windows::io::AsRawHandle`...) | — | 🟡 | Programs reach the system through `extern "C"`; raw descriptors of `std` types, once they exist (`File`, sockets), would be a record |
| `hint::black_box` | `std::hint::black_box` | ✅ | ADR-0090 |
| `any`, `TypeId` | — | ⛔ | Reflection; interfaces cover the cases of a general-purpose program |
| `backtrace` | — | ❌ | With error chains |
| `panic::catch_unwind`, hooks | — | ⛔ | No unwinding; a panic hook (a message before exiting) could come with the backtraces |
| `primitive`, `prelude` | the prelude | ✅ | |

## Benchmarks

`benchmarks/` measures how far Sagitario is from Rust and C, and from C#, JavaScript (Node.js) and
Python, with the same programs in each language, built or run optimized. It is part of S2, and
its record settles the open questions below.

| Benchmark | What it measures |
|---|---|
| n-body, spectral norm | Numeric loops over structs and arrays |
| binary trees | Allocation and reference counting |
| word count, sorting, a map of counts | Strings and collections |
| a parallel sum, a channel pipeline | Threads, atomics and locks |
| hello, world | The start of a program: time to `main` and to its end |

- Each run records the time, the peak memory and the size of the program, per platform, in
  `benchmarks/results/`.
- A tool of the suite renders each benchmark as an animated race of the languages: an HTML page
  without dependencies, and an MP4 when `ffmpeg` is installed ([ADR-0090](adr/0090-benchmarks.md)).
- The work it should point at: retains and releases removed when a value moves or is lent,
  atomic counts only for values that cross threads, `restrict` for `&mut` parameters and slices,
  and closures that never escape kept off the heap. The goal is the speed of Rust in code without
  allocation, and close to it elsewhere.
- They run locally, with the median of five runs after a warm-up by default; each results file
  names its machine, and files of different machines are not compared (ADR-0090).
- ✅ Done: the first full run and what it points at are in [`docs/benchmarks.md`](benchmarks.md).

## Order of the records

From what most programs need to what fewer do. Each line is one record, or a few small ones:

1. **The language gaps:** bitwise operators and shifts; `wrapping_*`, `checked_*` and
   `saturating_*` (or the spec corrected); integer `MIN`/`MAX`. ✅
2. **`fmt`:** `Display` and `to_string()` for user types, a `Debug` form of any value, and
   width, precision, alignment and radix in format strings. ✅
3. **The benchmarks** (above), with `hint::black_box`, so that later work is measured. ✅
4. **The everyday methods:** those missing from `String`, `char` (Unicode), `List`, slices, `Map`,
   `Iter`, `Option` and `Result`, and `assert`, `assert_eq`, `unreachable`, `todo`.
5. **Conversion and defaults:** `From`, `Into`, `TryFrom`, `Default`, generic `min`/`max`,
   `Index`.
6. **Collections:** `Set`, `Deque`, `SortedMap`, `SortedSet` and `PriorityQueue` (names to be
   settled), and `collect` into them.
7. **Streams and files:** `Read`, `Write`, buffered readers and writers, `File`, metadata,
   `remove_dir_all`, bytes on stdin and stdout; and with them the question of `Drop`.
8. **Processes and environment:** `spawn` and `Child`, redirection, the rest of `env`.
9. **Concurrency:** `RwLock`, `Condvar`, `Barrier`, bounded channels, `try_lock`, thread names and
   ids, and atomic orderings.
10. **Networking:** TCP and UDP sockets on the streams of 7.

**Acceptance of S2** ([roadmap](roadmap-v2.md#s2-a-complete-standard-library)): every row above is
✅, ➖ or ⛔ with its reason, and the benchmarks run, locally, with their results recorded
([ADR-0090](adr/0090-benchmarks.md)).
